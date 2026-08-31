use super::content::{self, TextItem, TextPart};
use super::font::Font;
use super::script;
use super::{self as pdf, Object, PdfError, XrefTable};

const ROOT_KEY: &str = "Root";
const PAGES_KEY: &str = "Pages";
const KIDS_KEY: &str = "Kids";
const TYPE_KEY: &str = "Type";
const CONTENTS_KEY: &str = "Contents";
const RESOURCES_KEY: &str = "Resources";
const FONT_KEY: &str = "Font";
const MEDIA_BOX_KEY: &str = "MediaBox";
const PAGE_TYPE: &str = "Page";
const PAGES_TYPE: &str = "Pages";
const MAX_TREE_DEPTH: usize = 32;
const CONTENT_SEPARATOR: u8 = b'\n';
const SPACE_ADJUSTMENT: f64 = -200.0;
const MEDIA_BOX_VALUES: usize = 4;
const DEFAULT_MEDIA_BOX: [f64; MEDIA_BOX_VALUES] = [0.0, 0.0, 612.0, 792.0];

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub spans: Vec<Span>,
    pub media_box: [f64; MEDIA_BOX_VALUES],
}

#[derive(Debug, Clone, Default)]
struct Inherited {
    resources: Option<Object>,
    media_box: Option<[f64; MEDIA_BOX_VALUES]>,
}

struct Reader<'a> {
    data: &'a [u8],
    table: XrefTable,
    loaded: Vec<(u32, Font)>,
}

pub fn read_pages(data: &[u8]) -> Result<Vec<Page>, PdfError> {
    let table = pdf::read_xref_table(data)?;
    let (trailer, _) = pdf::parse_object(data, table.trailer_offset)?;
    let mut reader = Reader {
        data,
        table,
        loaded: Vec::new(),
    };

    let root = reader.entry(&trailer, ROOT_KEY)?;
    let tree = reader.entry(&root, PAGES_KEY)?;
    let mut pages = Vec::new();
    reader.collect(&tree, &Inherited::default(), 0, &mut pages)?;

    Ok(pages)
}

impl Reader<'_> {
    fn resolve(&self, object: &Object) -> Result<Object, PdfError> {
        pdf::resolve(self.data, &self.table, object)
    }

    fn entry(&self, object: &Object, key: &str) -> Result<Object, PdfError> {
        self.resolve(object.get(key).ok_or(PdfError::ObjectNotFound)?)
    }

    fn resolve_optional(&self, object: &Object, key: &str) -> Result<Option<Object>, PdfError> {
        match object.get(key) {
            Some(object) => Ok(Some(self.resolve(object)?)),
            None => Ok(None),
        }
    }

    fn collect(
        &mut self,
        node: &Object,
        inherited: &Inherited,
        depth: usize,
        pages: &mut Vec<Page>,
    ) -> Result<(), PdfError> {
        if depth > MAX_TREE_DEPTH {
            return Err(PdfError::CircularReference);
        }

        let inherited = Inherited {
            resources: match node.get(RESOURCES_KEY) {
                Some(object) => Some(self.resolve(object)?),
                None => inherited.resources.clone(),
            },
            media_box: media_box(&self.resolve_optional(node, MEDIA_BOX_KEY)?)
                .or(inherited.media_box),
        };

        let leaf = match node.get(TYPE_KEY).and_then(Object::as_name) {
            Some(PAGE_TYPE) => true,
            Some(PAGES_TYPE) => false,
            _ => node.get(KIDS_KEY).is_none(),
        };
        if leaf {
            let page = self.page(node, &inherited)?;
            pages.push(page);
            return Ok(());
        }

        let kids = self.resolve_optional(node, KIDS_KEY)?;
        for kid in kids.as_ref().and_then(Object::as_array).unwrap_or_default() {
            let kid = self.resolve(kid)?;
            self.collect(&kid, &inherited, depth + 1, pages)?;
        }

        Ok(())
    }

    fn page(&mut self, node: &Object, inherited: &Inherited) -> Result<Page, PdfError> {
        let fonts = self.fonts(inherited.resources.as_ref())?;
        let operations = content::parse_content(&self.contents(node)?)?;

        Ok(Page {
            spans: content::extract_text_items(&operations)
                .iter()
                .filter_map(|item| span(&fonts, item))
                .collect(),
            media_box: inherited.media_box.unwrap_or(DEFAULT_MEDIA_BOX),
        })
    }

    fn contents(&self, node: &Object) -> Result<Vec<u8>, PdfError> {
        let Some(object) = self.resolve_optional(node, CONTENTS_KEY)? else {
            return Ok(Vec::new());
        };

        let Some(items) = object.as_array() else {
            return pdf::decode_stream(&object);
        };

        let mut content = Vec::new();
        for item in items {
            let stream = self.resolve(item)?;
            if !content.is_empty() {
                content.push(CONTENT_SEPARATOR);
            }
            content.extend(pdf::decode_stream(&stream)?);
        }

        Ok(content)
    }

    fn fonts(&mut self, resources: Option<&Object>) -> Result<Vec<(String, Font)>, PdfError> {
        let Some(resources) = resources else {
            return Ok(Vec::new());
        };
        let Some(dictionary) = self.resolve_optional(resources, FONT_KEY)? else {
            return Ok(Vec::new());
        };
        let Object::Dictionary(entries) = dictionary else {
            return Ok(Vec::new());
        };

        Ok(entries
            .iter()
            .map(|(name, value)| (name.clone(), self.font(value)))
            .collect())
    }

    fn font(&mut self, value: &Object) -> Font {
        let Object::Reference { object_number, .. } = value else {
            return self.load(value);
        };

        if let Some((_, font)) = self
            .loaded
            .iter()
            .find(|(number, _)| number == object_number)
        {
            return font.clone();
        }

        let font = self.load(value);
        self.loaded.push((*object_number, font.clone()));

        font
    }

    fn load(&self, value: &Object) -> Font {
        self.resolve(value)
            .map(|dictionary| Font::load(self.data, &self.table, &dictionary))
            .unwrap_or_default()
    }
}

fn span(fonts: &[(String, Font)], item: &TextItem) -> Option<Span> {
    let font = fonts
        .iter()
        .find(|(name, _)| *name == item.font)
        .map(|(_, font)| font)
        .cloned()
        .unwrap_or_default();

    let mut text = String::new();
    let mut spaced = false;
    for part in &item.parts {
        match part {
            TextPart::Adjustment(value) => spaced |= *value <= SPACE_ADJUSTMENT,
            TextPart::Text(bytes) => {
                let decoded = font.decode(bytes);
                if spaced && script::separable(&text, &decoded) {
                    text.push(' ');
                }
                spaced = false;
                text.push_str(&decoded);
            }
        }
    }

    (!text.trim().is_empty()).then_some(Span {
        text,
        x: item.x,
        y: item.y,
        size: item.size,
    })
}

fn media_box(object: &Option<Object>) -> Option<[f64; MEDIA_BOX_VALUES]> {
    let values: Option<Vec<f64>> = object
        .as_ref()?
        .as_array()?
        .iter()
        .map(Object::as_f64)
        .collect();

    <[f64; MEDIA_BOX_VALUES]>::try_from(values?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = "BT /F1 12 Tf 50 700 Td (Hello) Tj ET";
    const OTHER_CONTENT: &str = "BT /F1 12 Tf 50 680 Td (World) Tj ET";
    const A4_MEDIA_BOX: [f64; MEDIA_BOX_VALUES] = [0.0, 0.0, 595.0, 842.0];

    fn stream(content: &str) -> String {
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    }

    fn span(text: &str, x: f64, y: f64, size: f64) -> Span {
        Span {
            text: text.to_string(),
            x,
            y,
            size,
        }
    }

    fn pdf_with(objects: &[String], trailer: &str) -> Vec<u8> {
        let mut body = String::from("%PDF-1.7\n");
        let mut entries = format!("{:010} {:05} f \n", 0, 65535);
        for (index, object) in objects.iter().enumerate() {
            entries.push_str(&format!("{:010} {:05} n \n", body.len(), 0));
            body.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let table = format!(
            "xref\n0 {}\n{entries}trailer\n{trailer}\n",
            objects.len() + 1
        );

        format!("{body}{table}startxref\n{}\n%%EOF\n", body.len()).into_bytes()
    }

    fn pdf(objects: &[String]) -> Vec<u8> {
        pdf_with(objects, "<< /Size 9 /Root 1 0 R >>")
    }

    fn one_page(page: &str, content: &str) -> Vec<u8> {
        pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 595 842] >>".to_string(),
            page.to_string(),
            stream(content),
        ])
    }

    fn spans_of(content: &str) -> Vec<Span> {
        let data = one_page("<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>", content);

        read_pages(&data).unwrap().remove(0).spans
    }

    mod read_pages {
        use super::*;

        #[test]
        fn when_read_pages_with_one_page_then_returns_its_text_positioned() {
            assert_eq!(spans_of(CONTENT), vec![span("Hello", 50.0, 700.0, 12.0)]);
        }

        #[test]
        fn when_read_pages_with_a_wide_adjustment_then_reads_it_as_a_space() {
            let spans = spans_of("BT /F1 12 Tf 50 700 Td [(Hello)-300(world)] TJ ET");

            assert_eq!(spans, vec![span("Hello world", 50.0, 700.0, 12.0)]);
        }

        #[test]
        fn when_read_pages_with_a_narrow_adjustment_then_keeps_the_word_together() {
            let spans = spans_of("BT /F1 12 Tf 50 700 Td [(Hel)-20(lo)] TJ ET");

            assert_eq!(spans, vec![span("Hello", 50.0, 700.0, 12.0)]);
        }

        #[test]
        fn when_read_pages_with_blank_text_then_drops_it() {
            assert_eq!(spans_of("BT /F1 12 Tf 50 700 Td ( ) Tj ET"), Vec::new());
        }

        #[test]
        fn when_read_pages_with_malformed_content_then_returns_error() {
            let data = one_page(
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
                "BT /F1 12 Tf 50 700 Td (unterminated",
            );

            assert_eq!(read_pages(&data), Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_read_pages_with_media_box_on_the_parent_then_inherits_it() {
            let data = one_page("<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>", CONTENT);
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages[0].media_box, A4_MEDIA_BOX);
        }

        #[test]
        fn when_read_pages_with_media_box_on_the_page_then_prefers_it() {
            let data = one_page(
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /MediaBox [0 0 200 300] >>",
                CONTENT,
            );
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages[0].media_box, [0.0, 0.0, 200.0, 300.0]);
        }

        #[test]
        fn when_read_pages_without_media_box_then_returns_the_default() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 2 0 R >>".to_string(),
            ]);
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages[0].media_box, DEFAULT_MEDIA_BOX);
        }

        #[test]
        fn when_read_pages_with_nested_kids_then_returns_them_in_reading_order() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".to_string(),
                "<< /Type /Pages /Parent 2 0 R /Kids [4 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 3 0 R /Contents 5 0 R >>".to_string(),
                stream(CONTENT),
                "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>".to_string(),
                stream(OTHER_CONTENT),
            ]);
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages.len(), 2);
            assert_eq!(pages[0].spans, vec![span("Hello", 50.0, 700.0, 12.0)]);
            assert_eq!(pages[1].spans, vec![span("World", 50.0, 680.0, 12.0)]);
        }

        #[test]
        fn when_read_pages_with_several_content_streams_then_reads_them_all() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 2 0 R /Contents [4 0 R 5 0 R] >>".to_string(),
                stream(CONTENT),
                stream(OTHER_CONTENT),
            ]);
            let pages = read_pages(&data).unwrap();

            assert_eq!(
                pages[0].spans,
                vec![
                    span("Hello", 50.0, 700.0, 12.0),
                    span("World", 50.0, 680.0, 12.0)
                ]
            );
        }

        #[test]
        fn when_read_pages_without_contents_then_returns_no_text() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
                "<< /Type /Page /Parent 2 0 R >>".to_string(),
            ]);
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages[0].spans, Vec::new());
        }

        #[test]
        fn when_read_pages_with_resources_on_the_parent_then_decodes_with_those_fonts() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /Resources << /Font << /F1 5 0 R >> >> >>"
                    .to_string(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".to_string(),
                stream("BT /F1 12 Tf 50 700 Td <92> Tj ET"),
                "<< /Type /Font /Subtype /Type1 /Encoding /WinAnsiEncoding >>".to_string(),
            ]);
            let pages = read_pages(&data).unwrap();

            assert_eq!(pages[0].spans, vec![span("\u{2019}", 50.0, 700.0, 12.0)]);
        }

        #[test]
        fn when_read_pages_with_an_unknown_font_name_then_decodes_as_standard() {
            assert_eq!(
                spans_of("BT /F9 12 Tf 50 700 Td (Hello) Tj ET"),
                vec![span("Hello", 50.0, 700.0, 12.0)]
            );
        }

        #[test]
        fn when_read_pages_without_root_then_returns_error() {
            let data = pdf_with(
                &["<< /Type /Catalog >>".to_string()],
                "<< /Size 2 /Info 1 0 R >>",
            );

            assert_eq!(read_pages(&data), Err(PdfError::ObjectNotFound));
        }

        #[test]
        fn when_read_pages_without_page_tree_then_returns_error() {
            let data = pdf(&["<< /Type /Catalog >>".to_string()]);

            assert_eq!(read_pages(&data), Err(PdfError::ObjectNotFound));
        }

        #[test]
        fn when_read_pages_with_a_page_tree_cycle_then_returns_error() {
            let data = pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
                "<< /Type /Pages /Kids [2 0 R] /Count 1 >>".to_string(),
            ]);

            assert_eq!(read_pages(&data), Err(PdfError::CircularReference));
        }
    }
}
