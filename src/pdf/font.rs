use super::cmap::ToUnicode;
use super::encoding;
use super::japan1;
use super::{self as pdf, Object, XrefTable};

const SUBTYPE_KEY: &str = "Subtype";
const ENCODING_KEY: &str = "Encoding";
const BASE_ENCODING_KEY: &str = "BaseEncoding";
const DIFFERENCES_KEY: &str = "Differences";
const TO_UNICODE_KEY: &str = "ToUnicode";
const DESCENDANT_FONTS_KEY: &str = "DescendantFonts";
const CID_SYSTEM_INFO_KEY: &str = "CIDSystemInfo";
const REGISTRY_KEY: &str = "Registry";
const ORDERING_KEY: &str = "Ordering";
const COMPOSITE_SUBTYPE: &str = "Type0";
const ADOBE_REGISTRY: &[u8] = b"Adobe";
const JAPAN1_ORDERING: &[u8] = b"Japan1";
const IDENTITY_ENCODINGS: [&str; 2] = ["Identity-H", "Identity-V"];
const CID_BYTES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Collection {
    Japan1,
}

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    Simple(encoding::Codes),
    Composite,
    Japan1,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    to_unicode: Option<ToUnicode>,
    kind: Kind,
}

impl Font {
    pub fn load(data: &[u8], table: &XrefTable, dictionary: &Object) -> Self {
        let to_unicode = dictionary
            .get(TO_UNICODE_KEY)
            .and_then(|object| to_unicode(data, table, object));
        let kind = match (
            dictionary.get(SUBTYPE_KEY).and_then(Object::as_name),
            dictionary.get(ENCODING_KEY).and_then(Object::as_name),
            &to_unicode,
        ) {
            (Some(COMPOSITE_SUBTYPE), Some(encoding), None)
                if IDENTITY_ENCODINGS.contains(&encoding)
                    && collection(data, table, dictionary) == Some(Collection::Japan1) =>
            {
                Kind::Japan1
            }
            (Some(COMPOSITE_SUBTYPE), _, _) => Kind::Composite,
            _ => Kind::Simple(simple_codes(data, table, dictionary)),
        };

        Self { to_unicode, kind }
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        match &self.kind {
            Kind::Simple(codes) => bytes
                .iter()
                .filter_map(|code| self.simple_text(codes, *code))
                .collect(),
            Kind::Composite => self
                .to_unicode
                .as_ref()
                .map(|map| map.decode(bytes))
                .unwrap_or_default(),
            Kind::Japan1 => {
                bytes
                    .as_chunks::<CID_BYTES>()
                    .0
                    .iter()
                    .fold(String::new(), |mut text, code| {
                        japan1::push(u32::from(u16::from_be_bytes(*code)), &mut text);
                        text
                    })
            }
        }
    }

    fn simple_text(&self, codes: &encoding::Codes, code: u8) -> Option<String> {
        if let Some(map) = &self.to_unicode
            && let Some(text) = map.lookup(u32::from(code))
        {
            return Some(text);
        }

        codes.char_of(code).map(String::from)
    }
}

impl Default for Font {
    fn default() -> Self {
        Self {
            to_unicode: None,
            kind: Kind::Simple(encoding::codes(None, [])),
        }
    }
}

fn collection(data: &[u8], table: &XrefTable, dictionary: &Object) -> Option<Collection> {
    let info = dictionary
        .get(DESCENDANT_FONTS_KEY)
        .and_then(|object| pdf::resolve(data, table, object).ok())
        .as_ref()
        .and_then(Object::as_array)
        .and_then(<[Object]>::first)
        .and_then(|first| pdf::resolve(data, table, first).ok())
        .as_ref()
        .and_then(|descendant| descendant.get(CID_SYSTEM_INFO_KEY))
        .and_then(|object| pdf::resolve(data, table, object).ok())?;

    match (info.get(REGISTRY_KEY), info.get(ORDERING_KEY)) {
        (Some(Object::String(registry)), Some(Object::String(ordering)))
            if registry == ADOBE_REGISTRY && ordering == JAPAN1_ORDERING =>
        {
            Some(Collection::Japan1)
        }
        _ => None,
    }
}

fn to_unicode(data: &[u8], table: &XrefTable, object: &Object) -> Option<ToUnicode> {
    let stream = pdf::resolve(data, table, object).ok()?;
    if !matches!(stream, Object::Stream { .. }) {
        return None;
    }

    ToUnicode::parse(&pdf::decode_stream(&stream).ok()?).ok()
}

fn simple_codes(data: &[u8], table: &XrefTable, dictionary: &Object) -> encoding::Codes {
    let encoding = dictionary
        .get(ENCODING_KEY)
        .and_then(|object| pdf::resolve(data, table, object).ok());
    let base = match &encoding {
        Some(Object::Name(name)) => Some(name.as_str()),
        Some(object) => object.get(BASE_ENCODING_KEY).and_then(Object::as_name),
        None => None,
    };
    let differences = encoding
        .as_ref()
        .and_then(|object| object.get(DIFFERENCES_KEY))
        .and_then(|object| pdf::resolve(data, table, object).ok());

    encoding::codes(
        base,
        differences
            .as_ref()
            .and_then(Object::as_array)
            .map_or_else(Vec::new, |items| {
                items
                    .iter()
                    .scan(0usize, |next, item| {
                        Some(match item {
                            Object::Name(name) => {
                                let code = *next;
                                *next = next.saturating_add(1);
                                Some((code, name.as_str()))
                            }
                            object => {
                                *next = object
                                    .as_i64()
                                    .and_then(|code| usize::try_from(code).ok())
                                    .unwrap_or(*next);
                                None
                            }
                        })
                    })
                    .flatten()
                    .collect()
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY_CMAP: &str = concat!(
        "begincmap\n",
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        "2 beginbfchar\n<0024> <0041>\n<0025> <65E5>\nendbfchar\n",
        "endcmap\n",
    );
    const SIMPLE_CMAP: &str = concat!(
        "begincmap\n",
        "1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
        "1 beginbfchar\n<41> <2460>\nendbfchar\n",
        "endcmap\n",
    );

    fn empty_table() -> XrefTable {
        XrefTable {
            entries: Vec::new(),
            trailer_offset: 0,
        }
    }

    fn stream(payload: &str) -> Object {
        Object::Stream {
            dictionary: Vec::new(),
            data: payload.as_bytes().to_vec(),
        }
    }

    fn font(entries: Vec<(&str, Object)>) -> Object {
        Object::Dictionary(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    fn loaded(entries: Vec<(&str, Object)>) -> Font {
        Font::load(b"", &empty_table(), &font(entries))
    }

    fn cid_font(ordering: &str) -> Object {
        font(vec![
            ("Subtype", Object::Name("CIDFontType0".to_string())),
            (
                "CIDSystemInfo",
                font(vec![
                    ("Registry", Object::String(b"Adobe".to_vec())),
                    ("Ordering", Object::String(ordering.as_bytes().to_vec())),
                ]),
            ),
        ])
    }

    fn composite(encoding: &str, ordering: &str) -> Vec<(&'static str, Object)> {
        vec![
            ("Subtype", Object::Name("Type0".to_string())),
            ("Encoding", Object::Name(encoding.to_string())),
            ("DescendantFonts", Object::Array(vec![cid_font(ordering)])),
        ]
    }

    mod load {
        use super::*;

        #[test]
        fn when_load_with_no_encoding_then_decodes_as_standard() {
            let font = loaded(vec![("Subtype", Object::Name("Type1".to_string()))]);

            assert_eq!(font.decode(b"'"), "\u{2019}".to_string());
        }

        #[test]
        fn when_load_with_win_ansi_encoding_then_decodes_as_win_ansi() {
            let font = loaded(vec![(
                "Encoding",
                Object::Name("WinAnsiEncoding".to_string()),
            )]);

            assert_eq!(font.decode(&[0x92]), "\u{2019}".to_string());
        }

        #[test]
        fn when_load_with_encoding_dictionary_then_decodes_as_its_base_encoding() {
            let encoding = font(vec![(
                "BaseEncoding",
                Object::Name("WinAnsiEncoding".to_string()),
            )]);
            let font = loaded(vec![("Encoding", encoding)]);

            assert_eq!(font.decode(&[0x80]), "€".to_string());
        }

        #[test]
        fn when_load_with_differences_then_overrides_those_codes() {
            let encoding = font(vec![
                ("BaseEncoding", Object::Name("WinAnsiEncoding".to_string())),
                (
                    "Differences",
                    Object::Array(vec![
                        Object::Integer(0x41),
                        Object::Name("bullet".to_string()),
                        Object::Name("uni65E5".to_string()),
                    ]),
                ),
            ]);
            let font = loaded(vec![("Encoding", encoding)]);

            assert_eq!(font.decode(b"ABC"), "•日C".to_string());
        }

        #[test]
        fn when_load_with_unknown_glyph_in_differences_then_drops_that_code() {
            let encoding = font(vec![(
                "Differences",
                Object::Array(vec![
                    Object::Integer(0x41),
                    Object::Name("nosuchglyph".to_string()),
                ]),
            )]);
            let font = loaded(vec![("Encoding", encoding)]);

            assert_eq!(font.decode(b"AB"), "B".to_string());
        }

        #[test]
        fn when_load_with_out_of_range_differences_then_keeps_the_other_codes() {
            let encoding = font(vec![(
                "Differences",
                Object::Array(vec![
                    Object::Integer(512),
                    Object::Name("bullet".to_string()),
                ]),
            )]);
            let font = loaded(vec![("Encoding", encoding)]);

            assert_eq!(font.decode(b"A"), "A".to_string());
        }

        #[test]
        fn when_load_with_to_unicode_then_it_wins_over_the_encoding() {
            let font = loaded(vec![
                ("Encoding", Object::Name("WinAnsiEncoding".to_string())),
                ("ToUnicode", stream(SIMPLE_CMAP)),
            ]);

            assert_eq!(font.decode(b"AB"), "①B".to_string());
        }

        #[test]
        fn when_load_with_composite_subtype_and_to_unicode_then_decodes_two_byte_codes() {
            let font = loaded(vec![
                ("Subtype", Object::Name("Type0".to_string())),
                ("Encoding", Object::Name("Identity-H".to_string())),
                ("ToUnicode", stream(IDENTITY_CMAP)),
            ]);

            assert_eq!(font.decode(&[0x00, 0x24, 0x00, 0x25]), "A日".to_string());
        }

        #[test]
        fn when_load_with_composite_subtype_and_no_to_unicode_then_decodes_nothing() {
            let font = loaded(vec![
                ("Subtype", Object::Name("Type0".to_string())),
                ("Encoding", Object::Name("Identity-H".to_string())),
            ]);

            assert_eq!(font.decode(&[0x00, 0x24]), String::new());
        }

        #[test]
        fn when_load_with_to_unicode_that_is_not_a_stream_then_ignores_it() {
            let font = loaded(vec![
                ("Encoding", Object::Name("WinAnsiEncoding".to_string())),
                ("ToUnicode", Object::Null),
            ]);

            assert_eq!(font.decode(b"A"), "A".to_string());
        }

        #[test]
        fn when_load_with_unsupported_to_unicode_filter_then_falls_back_to_the_encoding() {
            let to_unicode = Object::Stream {
                dictionary: vec![("Filter".to_string(), Object::Name("LZWDecode".to_string()))],
                data: Vec::new(),
            };
            let font = loaded(vec![
                ("Encoding", Object::Name("WinAnsiEncoding".to_string())),
                ("ToUnicode", to_unicode),
            ]);

            assert_eq!(font.decode(&[0x92]), "\u{2019}".to_string());
        }

        #[test]
        fn when_load_with_indirect_encoding_then_resolves_it() {
            let body = "%PDF-1.7\n1 0 obj\n/WinAnsiEncoding\nendobj\n";
            let data = format!(
                "{body}xref\n0 2\n{}{}trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                format_args!("{:010} {:05} f \n", 0, 65535),
                format_args!("{:010} {:05} n \n", 9, 0),
                body.len()
            )
            .into_bytes();
            let table = pdf::read_xref_table(&data).unwrap();
            let dictionary = font(vec![(
                "Encoding",
                Object::Reference {
                    object_number: 1,
                    generation: 0,
                },
            )]);
            let font = Font::load(&data, &table, &dictionary);

            assert_eq!(font.decode(&[0x92]), "\u{2019}".to_string());
        }
        #[test]
        fn when_load_with_identity_japan1_font_then_decodes_cids_as_japanese() {
            let font = loaded(composite("Identity-H", "Japan1"));

            assert_eq!(font.decode(&[0x0E, 0x8A, 0x09, 0x7B]), "本書".to_string());
        }

        #[test]
        fn when_load_with_identity_japan1_font_then_ignores_a_trailing_half_code() {
            let font = loaded(composite("Identity-H", "Japan1"));

            assert_eq!(font.decode(&[0x0E, 0x8A, 0x09]), "本".to_string());
        }

        #[test]
        fn when_load_with_vertical_identity_japan1_font_then_decodes_cids() {
            let font = loaded(composite("Identity-V", "Japan1"));

            assert_eq!(font.decode(&[0x03, 0x4B]), "あ".to_string());
        }

        #[test]
        fn when_load_with_another_ordering_then_decodes_nothing() {
            let font = loaded(composite("Identity-H", "Identity"));

            assert_eq!(font.decode(&[0x0E, 0x8A]), String::new());
        }

        #[test]
        fn when_load_with_non_identity_encoding_then_decodes_nothing() {
            let font = loaded(composite("UniJIS-UCS2-H", "Japan1"));

            assert_eq!(font.decode(&[0x0E, 0x8A]), String::new());
        }

        #[test]
        fn when_load_with_japan1_font_and_to_unicode_then_to_unicode_wins() {
            let mut entries = composite("Identity-H", "Japan1");
            entries.push(("ToUnicode", stream(IDENTITY_CMAP)));
            let font = loaded(entries);

            assert_eq!(font.decode(&[0x00, 0x24]), "A".to_string());
        }
    }

    mod default {
        use super::*;

        #[test]
        fn when_default_then_decodes_ascii_text() {
            assert_eq!(Font::default().decode(b"Hello"), "Hello".to_string());
        }
    }
}
