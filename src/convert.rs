use crate::epub::xhtml;
use crate::epub::zip;
use crate::epub::{self, EpubError};
use crate::ir::{Anchor, Block, ResourcePath};
use crate::markdown;
use crate::pdf::PdfError;
use crate::pdf::layout;
use crate::pdf::page;

const PDF_MAGIC: &[u8] = b"%PDF-";
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const MAGIC_SEARCH: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedImage {
    pub path: ResourcePath,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Epub,
    Pdf,
}

impl Format {
    fn of(data: &[u8]) -> Option<Self> {
        if data.starts_with(ZIP_MAGIC) {
            return Some(Self::Epub);
        }

        let head = &data[..data.len().min(MAGIC_SEARCH)];
        head.windows(PDF_MAGIC.len())
            .any(|window| window == PDF_MAGIC)
            .then_some(Self::Pdf)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConvertError {
    UnknownFormat,
    Epub(EpubError),
    Pdf(PdfError),
}

impl core::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConvertError::UnknownFormat => write!(f, "input is neither an EPUB nor a PDF"),
            ConvertError::Epub(error) => write!(f, "{error}"),
            ConvertError::Pdf(error) => write!(f, "{error}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub markdown: String,
    pub images: Vec<ExtractedImage>,
}

pub fn convert(data: &[u8]) -> Result<Conversion, ConvertError> {
    match Format::of(data) {
        Some(Format::Epub) => convert_epub(data).map_err(ConvertError::Epub),
        Some(Format::Pdf) => convert_pdf(data).map_err(ConvertError::Pdf),
        None => Err(ConvertError::UnknownFormat),
    }
}

pub fn convert_pdf(data: &[u8]) -> Result<Conversion, PdfError> {
    let pages = page::read_pages(data)?;

    Ok(Conversion {
        markdown: markdown::emit(&layout::document_blocks(&pages)),
        images: Vec::new(),
    })
}

pub fn convert_epub(data: &[u8]) -> Result<Conversion, EpubError> {
    let mut rendered = Vec::new();
    let mut images: Vec<ExtractedImage> = Vec::new();

    for (path, content) in read_spine_documents(data)? {
        let blocks = document_blocks(&path, &content);
        rendered.push(markdown::emit(&blocks));

        for source in markdown::collect_image_sources(&blocks) {
            if images.iter().any(|image| image.path == source) {
                continue;
            }
            if let Ok(bytes) = zip::extract_by_name(data, source.as_str().as_bytes()) {
                images.push(ExtractedImage {
                    path: source,
                    data: bytes,
                });
            }
        }
    }

    Ok(Conversion {
        markdown: rendered.join("\n\n"),
        images,
    })
}

pub fn epub_to_markdown(data: &[u8]) -> Result<String, EpubError> {
    Ok(convert_epub(data)?.markdown)
}

pub fn extract_images(data: &[u8]) -> Result<Vec<ExtractedImage>, EpubError> {
    Ok(convert_epub(data)?.images)
}

fn document_blocks(path: &ResourcePath, content: &[u8]) -> Vec<Block> {
    let mut blocks = vec![Block::Anchor(Anchor::new(path, None))];
    blocks.extend(xhtml::parse_blocks(content, path));
    blocks
}

fn read_spine_documents(data: &[u8]) -> Result<Vec<(ResourcePath, Vec<u8>)>, EpubError> {
    let opf_path = epub::find_opf_path_from_epub(data)?;
    let opf = zip::extract_by_name(data, opf_path.as_bytes()).map_err(EpubError::ZipError)?;
    let opf_path = ResourcePath::resolve("", &opf_path);

    let mut documents = Vec::new();
    for href in epub::parse_spine(&opf)? {
        let path = ResourcePath::resolve(opf_path.parent(), &href);
        let content =
            zip::extract_by_name(data, path.as_str().as_bytes()).map_err(EpubError::ZipError)?;
        documents.push((path, content));
    }

    Ok(documents)
}

#[cfg(test)]
pub mod tests_support {
    const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
    const CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
    const EOCD_SIGNATURE: u32 = 0x06054b50;

    pub fn zip(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut local_headers: Vec<u8> = Vec::new();
        let mut offsets = Vec::new();

        for (name, content) in entries {
            let offset = local_headers.len() as u32;
            let name = name.as_bytes();
            local_headers.extend_from_slice(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes());
            local_headers.extend_from_slice(&20u16.to_le_bytes());
            local_headers.extend_from_slice(&0u16.to_le_bytes());
            local_headers.extend_from_slice(&0u16.to_le_bytes());
            local_headers.extend_from_slice(&0u16.to_le_bytes());
            local_headers.extend_from_slice(&0u16.to_le_bytes());
            local_headers.extend_from_slice(&0u32.to_le_bytes());
            local_headers.extend_from_slice(&(content.len() as u32).to_le_bytes());
            local_headers.extend_from_slice(&(content.len() as u32).to_le_bytes());
            local_headers.extend_from_slice(&(name.len() as u16).to_le_bytes());
            local_headers.extend_from_slice(&0u16.to_le_bytes());
            local_headers.extend_from_slice(name);
            local_headers.extend_from_slice(content);
            offsets.push((name.to_vec(), content.len() as u32, offset));
        }

        let cd_offset = local_headers.len() as u32;
        let mut cd = Vec::new();
        for (name, size, offset) in &offsets {
            cd.extend_from_slice(&CENTRAL_DIR_SIGNATURE.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&offset.to_le_bytes());
            cd.extend_from_slice(name);
        }

        let mut eocd = Vec::new();
        eocd.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());
        eocd.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        eocd.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        eocd.extend_from_slice(&(cd.len() as u32).to_le_bytes());
        eocd.extend_from_slice(&cd_offset.to_le_bytes());
        eocd.extend_from_slice(&0u16.to_le_bytes());

        let mut result = local_headers;
        result.extend_from_slice(&cd);
        result.extend_from_slice(&eocd);
        result
    }

    pub fn pdf(content: &str) -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            concat!(
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792]",
                " /Resources << /Font << /F1 5 0 R >> >> >>"
            )
            .to_string(),
            "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /Encoding /WinAnsiEncoding >>".to_string(),
        ];

        let mut body = String::from("%PDF-1.7\n");
        let mut entries = format!("{:010} {:05} f \n", 0, 65535);
        for (index, object) in objects.iter().enumerate() {
            entries.push_str(&format!("{:010} {:05} n \n", body.len(), 0));
            body.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let table = format!(
            "xref\n0 {}\n{entries}trailer\n<< /Size {} /Root 1 0 R >>\n",
            objects.len() + 1,
            objects.len() + 1
        );

        format!("{body}{table}startxref\n{}\n%%EOF\n", body.len()).into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::ResourcePath;

    const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
    const CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
    const EOCD_SIGNATURE: u32 = 0x06054b50;

    struct ZipBuilder {
        local_headers: Vec<u8>,
        entries: Vec<(Vec<u8>, u32, u32)>,
    }

    impl ZipBuilder {
        fn new() -> Self {
            Self {
                local_headers: Vec::new(),
                entries: Vec::new(),
            }
        }

        fn add(mut self, name: &str, content: &[u8]) -> Self {
            let offset = self.local_headers.len() as u32;
            let name = name.as_bytes();

            let mut header = Vec::new();
            header.extend_from_slice(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes());
            header.extend_from_slice(&20u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes()); // stored
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(&0u32.to_le_bytes());
            header.extend_from_slice(&(content.len() as u32).to_le_bytes());
            header.extend_from_slice(&(content.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            header.extend_from_slice(name);
            header.extend_from_slice(content);

            self.local_headers.extend_from_slice(&header);
            self.entries
                .push((name.to_vec(), content.len() as u32, offset));
            self
        }

        fn build(self) -> Vec<u8> {
            let cd_offset = self.local_headers.len() as u32;
            let mut cd = Vec::new();
            for (name, size, offset) in &self.entries {
                cd.extend_from_slice(&CENTRAL_DIR_SIGNATURE.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes()); // stored
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&size.to_le_bytes());
                cd.extend_from_slice(&size.to_le_bytes());
                cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&offset.to_le_bytes());
                cd.extend_from_slice(name);
            }

            let mut eocd = Vec::new();
            eocd.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());
            eocd.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            eocd.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
            eocd.extend_from_slice(&(cd.len() as u32).to_le_bytes());
            eocd.extend_from_slice(&cd_offset.to_le_bytes());
            eocd.extend_from_slice(&0u16.to_le_bytes());

            let mut result = self.local_headers;
            result.extend_from_slice(&cd);
            result.extend_from_slice(&eocd);
            result
        }
    }

    const CONTAINER: &[u8] = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    mod epub_to_markdown {
        use super::*;

        #[test]
        fn when_link_points_to_another_chapter_then_becomes_anchor_reference() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="Text/chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let ch1 = br#"<p>Go to <a href="chapter2.xhtml">next</a>.</p>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/Text/chapter1.xhtml", ch1)
                .add("OEBPS/Text/chapter2.xhtml", b"<h1>Two</h1>")
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert!(
                markdown.contains("[next](#OEBPS-Text-chapter2-xhtml)"),
                "{markdown}"
            );
            assert!(
                markdown.contains("<a id=\"OEBPS-Text-chapter2-xhtml\"></a>"),
                "{markdown}"
            );
        }

        #[test]
        fn when_link_targets_an_element_then_resolves_to_that_anchor() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let ch1 = br##"<p>See <a href="#fn1">note</a>.</p><p id="fn1">The note.</p>"##;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", ch1)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert!(
                markdown.contains("[note](#OEBPS-chapter1-xhtml--fn1)"),
                "{markdown}"
            );
            assert!(
                markdown.contains("<a id=\"OEBPS-chapter1-xhtml--fn1\"></a>"),
                "{markdown}"
            );
        }

        #[test]
        fn when_link_is_external_then_kept_as_is() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let ch1 = br#"<p><a href="https://example.com/a?b=1">site</a></p>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", ch1)
                .build();

            assert!(
                epub_to_markdown(&epub)
                    .unwrap()
                    .contains("[site](https://example.com/a?b=1)")
            );
        }

        #[test]
        fn when_same_id_appears_in_two_chapters_then_anchors_do_not_collide() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let chapter = br#"<p id="note">Note.</p>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/chapter2.xhtml", chapter)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert!(
                markdown.contains("<a id=\"OEBPS-chapter1-xhtml--note\"></a>"),
                "{markdown}"
            );
            assert!(
                markdown.contains("<a id=\"OEBPS-chapter2-xhtml--note\"></a>"),
                "{markdown}"
            );
        }

        #[test]
        fn when_single_chapter_then_converts_to_markdown() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = b"<html><body><h1>Chapter One</h1><p>Hello world.</p></body></html>";
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(
                markdown,
                "<a id=\"OEBPS-chapter1-xhtml\"></a>\n\n# Chapter One\n\nHello world."
            );
        }

        #[test]
        fn when_multiple_chapters_then_joins_in_spine_order() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch2"/><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", b"<h1>First</h1>")
                .add("OEBPS/chapter2.xhtml", b"<h1>Second</h1>")
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(
                markdown,
                "<a id=\"OEBPS-chapter2-xhtml\"></a>\n\n# Second\n\n<a id=\"OEBPS-chapter1-xhtml\"></a>\n\n# First"
            );
        }

        #[test]
        fn when_chapter_has_mixed_blocks_then_preserves_document_order() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = br#"<body>
<h1>Title</h1>
<p>Intro.</p>
<ul><li>One</li><li>Two</li></ul>
<p>After list.</p>
<img src="figure.png" alt="Figure"/>
</body>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();

            assert_eq!(
                markdown,
                "<a id=\"OEBPS-chapter1-xhtml\"></a>\n\n# Title\n\nIntro.\n\n- One\n- Two\n\nAfter list.\n\n![Figure](OEBPS/figure.png)"
            );
        }

        #[test]
        fn when_opf_at_root_then_resolves_hrefs_at_root() {
            let container = br#"<container><rootfiles>
<rootfile full-path="content.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", container)
                .add("content.opf", opf)
                .add("chapter1.xhtml", b"<h1>Root</h1>")
                .build();

            assert_eq!(
                epub_to_markdown(&epub).unwrap(),
                "<a id=\"chapter1-xhtml\"></a>\n\n# Root"
            );
        }

        #[test]
        fn when_spine_file_is_missing_then_returns_zip_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="missing.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .build();

            assert_eq!(
                epub_to_markdown(&epub),
                Err(EpubError::ZipError(
                    crate::epub::zip::ZipError::EntryNotFound
                ))
            );
        }

        #[test]
        fn when_container_is_missing_then_returns_zip_error() {
            let epub = ZipBuilder::new()
                .add("OEBPS/content.opf", b"<package/>")
                .build();

            assert_eq!(
                epub_to_markdown(&epub),
                Err(EpubError::ZipError(
                    crate::epub::zip::ZipError::EntryNotFound
                ))
            );
        }

        #[test]
        fn when_chapter_is_empty_then_keeps_only_its_anchor() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", b"<html><body></body></html>")
                .add("OEBPS/chapter2.xhtml", b"<h1>Only</h1>")
                .build();

            assert_eq!(
                epub_to_markdown(&epub).unwrap(),
                "<a id=\"OEBPS-chapter1-xhtml\"></a>\n\n<a id=\"OEBPS-chapter2-xhtml\"></a>\n\n# Only"
            );
        }
    }

    mod extract_images {
        use super::*;

        #[test]
        fn when_chapter_is_nested_then_markdown_and_extracted_paths_agree() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = br#"<img src="../Images/fig.png" alt="Figure"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/Text/chapter1.xhtml", chapter)
                .add("OEBPS/Images/fig.png", b"PNG")
                .build();

            let markdown = epub_to_markdown(&epub).unwrap();
            let extracted = extract_images(&epub).unwrap();

            assert_eq!(extracted.len(), 1);
            assert!(
                markdown.contains(extracted[0].path.as_str()),
                "markdown {markdown:?} does not reference extracted {:?}",
                extracted[0].path
            );
        }

        const SINGLE_CHAPTER_OPF: &[u8] = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;

        #[test]
        fn when_chapter_references_image_then_extracts_it() {
            let chapter = br#"<p><img src="figure.png" alt="Figure"/></p>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/figure.png", b"PNGDATA")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: ResourcePath::resolve("", "OEBPS/figure.png"),
                    data: b"PNGDATA".to_vec(),
                }]
            );
        }

        #[test]
        fn when_src_is_relative_to_chapter_then_resolves_from_chapter_dir() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="Text/chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let chapter = br#"<img src="../Images/figure.png" alt="Figure"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/Text/chapter1.xhtml", chapter)
                .add("OEBPS/Images/figure.png", b"PNGDATA")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: ResourcePath::resolve("", "OEBPS/Images/figure.png"),
                    data: b"PNGDATA".to_vec(),
                }]
            );
        }

        #[test]
        fn when_multiple_images_then_extracts_in_document_order() {
            let chapter = br#"<img src="a.png" alt="A"/><p>text</p><img src="b.png" alt="B"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/a.png", b"AAA")
                .add("OEBPS/b.png", b"BBB")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
                vec!["OEBPS/a.png", "OEBPS/b.png"]
            );
        }

        #[test]
        fn when_same_image_referenced_from_two_chapters_then_extracts_once() {
            let opf = br#"<package>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="ch1"/><itemref idref="ch2"/></spine>
</package>"#;
            let chapter = br#"<img src="shared.png" alt="Shared"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/chapter2.xhtml", chapter)
                .add("OEBPS/shared.png", b"SHARED")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: ResourcePath::resolve("", "OEBPS/shared.png"),
                    data: b"SHARED".to_vec(),
                }]
            );
        }

        #[test]
        fn when_referenced_image_is_missing_then_skips_it() {
            let chapter =
                br#"<img src="missing.png" alt="Gone"/><img src="present.png" alt="Here"/>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", chapter)
                .add("OEBPS/present.png", b"HERE")
                .build();

            let images = extract_images(&epub).unwrap();

            assert_eq!(
                images,
                vec![ExtractedImage {
                    path: ResourcePath::resolve("", "OEBPS/present.png"),
                    data: b"HERE".to_vec(),
                }]
            );
        }

        #[test]
        fn when_no_images_referenced_then_returns_empty() {
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", SINGLE_CHAPTER_OPF)
                .add("OEBPS/chapter1.xhtml", b"<h1>No images</h1>")
                .add("OEBPS/unused.png", b"UNUSED")
                .build();

            assert_eq!(extract_images(&epub).unwrap(), vec![]);
        }

        #[test]
        fn when_spine_file_is_missing_then_returns_zip_error() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="missing.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .build();

            assert_eq!(
                extract_images(&epub),
                Err(EpubError::ZipError(
                    crate::epub::zip::ZipError::EntryNotFound
                ))
            );
        }
    }

    mod format {
        use super::*;

        #[test]
        fn when_format_with_zip_then_returns_epub() {
            let epub = tests_support::zip(&[("mimetype", b"application/epub+zip".to_vec())]);

            assert_eq!(Format::of(&epub), Some(Format::Epub));
        }

        #[test]
        fn when_format_with_pdf_header_then_returns_pdf() {
            assert_eq!(Format::of(b"%PDF-1.7\n"), Some(Format::Pdf));
        }

        #[test]
        fn when_format_with_pdf_header_after_junk_then_returns_pdf() {
            assert_eq!(Format::of(b"junk\n%PDF-1.7\n"), Some(Format::Pdf));
        }

        #[test]
        fn when_format_with_pdf_header_beyond_the_search_then_returns_none() {
            let mut data = vec![b' '; 2048];
            data.extend_from_slice(b"%PDF-1.7\n");

            assert_eq!(Format::of(&data), None);
        }

        #[test]
        fn when_format_with_other_bytes_then_returns_none() {
            assert_eq!(Format::of(b"not a document"), None);
            assert_eq!(Format::of(b""), None);
        }
    }

    mod convert_pdf {
        use super::*;

        #[test]
        fn when_convert_pdf_with_text_then_returns_it_as_a_paragraph() {
            let pdf = tests_support::pdf("BT /F1 12 Tf 50 700 Td (Hello world.) Tj ET");

            assert_eq!(convert_pdf(&pdf).unwrap().markdown, "Hello world.");
        }

        #[test]
        fn when_convert_pdf_with_a_larger_line_then_returns_it_as_a_heading() {
            let pdf = tests_support::pdf(concat!(
                "BT /F1 24 Tf 50 700 Td (Title) Tj ET\n",
                "BT /F1 12 Tf 50 600 Td (Body text is longer than the title.) Tj ET",
            ));

            assert_eq!(
                convert_pdf(&pdf).unwrap().markdown,
                "# Title\n\nBody text is longer than the title."
            );
        }

        #[test]
        fn when_convert_pdf_then_extracts_no_images() {
            let pdf = tests_support::pdf("BT /F1 12 Tf 50 700 Td (Hello world.) Tj ET");

            assert_eq!(convert_pdf(&pdf).unwrap().images, Vec::new());
        }

        #[test]
        fn when_convert_pdf_with_a_broken_document_then_returns_error() {
            assert_eq!(convert_pdf(b"%PDF-1.7\n"), Err(PdfError::StartxrefNotFound));
        }
    }

    mod convert {
        use super::*;

        #[test]
        fn when_convert_with_a_pdf_then_converts_it() {
            let pdf = tests_support::pdf("BT /F1 12 Tf 50 700 Td (Hello world.) Tj ET");

            assert_eq!(convert(&pdf).unwrap().markdown, "Hello world.");
        }

        #[test]
        fn when_convert_with_an_epub_then_converts_it() {
            let opf = br#"<package>
  <manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = ZipBuilder::new()
                .add("META-INF/container.xml", CONTAINER)
                .add("OEBPS/content.opf", opf)
                .add("OEBPS/ch1.xhtml", b"<h1>Chapter One</h1>")
                .build();

            assert!(convert(&epub).unwrap().markdown.contains("# Chapter One"));
        }

        #[test]
        fn when_convert_with_another_format_then_returns_error() {
            assert_eq!(convert(b"not a document"), Err(ConvertError::UnknownFormat));
        }
    }
}
