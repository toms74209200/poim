use super::content;
use super::{Object, PdfError};

const END_CODESPACE_RANGE: &str = "endcodespacerange";
const END_BF_CHAR: &str = "endbfchar";
const END_BF_RANGE: &str = "endbfrange";
const CODESPACE_OPERANDS: usize = 2;
const BF_CHAR_OPERANDS: usize = 2;
const BF_RANGE_OPERANDS: usize = 3;
const MAX_CODE_BYTES: usize = 4;
const FALLBACK_CODE_BYTES: usize = 1;
const UTF16_BYTES: usize = 2;

#[derive(Debug, Clone, PartialEq)]
struct CodespaceRange {
    bytes: usize,
    low: u32,
    high: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct Mapping {
    low: u32,
    high: u32,
    target: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToUnicode {
    codespaces: Vec<CodespaceRange>,
    mappings: Vec<Mapping>,
}

impl ToUnicode {
    pub fn parse(data: &[u8]) -> Result<Self, PdfError> {
        let mut codespaces = Vec::new();
        let mut mappings = Vec::new();
        for operation in content::parse_content(data)? {
            let operands = operation.operands.as_slice();
            match operation.operator.as_str() {
                END_CODESPACE_RANGE => codespaces.extend(codespace_ranges(operands)),
                END_BF_CHAR => mappings.extend(bf_chars(operands)),
                END_BF_RANGE => mappings.extend(bf_ranges(operands)),
                _ => {}
            }
        }
        mappings.sort_by_key(|mapping| mapping.low);

        Ok(Self {
            codespaces,
            mappings,
        })
    }

    pub fn decode(&self, bytes: &[u8]) -> String {
        let mut text = String::new();
        let mut position = 0;
        while position < bytes.len() {
            let remaining = &bytes[position..];
            let length = self.code_length(remaining).min(remaining.len());
            if let Some(mapped) = self.lookup(code_of(&remaining[..length])) {
                text.push_str(&mapped);
            }
            position += length;
        }

        text
    }

    pub fn lookup(&self, code: u32) -> Option<String> {
        let index = self
            .mappings
            .partition_point(|mapping| mapping.low <= code)
            .checked_sub(1)?;
        let mapping = self.mappings.get(index)?;

        (code <= mapping.high).then(|| {
            let offset = (code - mapping.low) as u16;
            let units = mapping.target.iter().enumerate().map(|(at, unit)| {
                match at + 1 == mapping.target.len() {
                    true => unit.wrapping_add(offset),
                    false => *unit,
                }
            });

            char::decode_utf16(units).filter_map(Result::ok).collect()
        })
    }

    fn code_length(&self, bytes: &[u8]) -> usize {
        for length in 1..=MAX_CODE_BYTES.min(bytes.len()) {
            let code = code_of(&bytes[..length]);
            if self.codespaces.iter().any(|codespace| {
                codespace.bytes == length && (codespace.low..=codespace.high).contains(&code)
            }) {
                return length;
            }
        }

        self.codespaces
            .iter()
            .map(|codespace| codespace.bytes)
            .min()
            .unwrap_or(FALLBACK_CODE_BYTES)
    }
}

fn codespace_ranges(operands: &[Object]) -> Vec<CodespaceRange> {
    operands
        .as_chunks::<CODESPACE_OPERANDS>()
        .0
        .iter()
        .filter_map(|pair| {
            let (Object::String(low), Object::String(high)) = (&pair[0], &pair[1]) else {
                return None;
            };
            let bytes = low.len();
            (bytes == high.len() && (1..=MAX_CODE_BYTES).contains(&bytes)).then(|| CodespaceRange {
                bytes,
                low: code_of(low),
                high: code_of(high),
            })
        })
        .collect()
}

fn bf_chars(operands: &[Object]) -> Vec<Mapping> {
    operands
        .as_chunks::<BF_CHAR_OPERANDS>()
        .0
        .iter()
        .filter_map(|pair| {
            let Object::String(code) = &pair[0] else {
                return None;
            };
            let code = code_of(code);
            Some(Mapping {
                low: code,
                high: code,
                target: utf16_units(&pair[1])?,
            })
        })
        .collect()
}

fn bf_ranges(operands: &[Object]) -> Vec<Mapping> {
    let mut mappings = Vec::new();
    for triple in operands.as_chunks::<BF_RANGE_OPERANDS>().0 {
        let (Object::String(low), Object::String(high)) = (&triple[0], &triple[1]) else {
            continue;
        };
        let (low, high) = (code_of(low), code_of(high));
        if high < low {
            continue;
        }

        match &triple[2] {
            Object::Array(targets) => {
                for (offset, target) in targets.iter().enumerate() {
                    let code = low.saturating_add(offset as u32);
                    if code > high {
                        break;
                    }
                    let Some(target) = utf16_units(target) else {
                        continue;
                    };
                    mappings.push(Mapping {
                        low: code,
                        high: code,
                        target,
                    });
                }
            }
            target => {
                if let Some(target) = utf16_units(target) {
                    mappings.push(Mapping { low, high, target });
                }
            }
        }
    }

    mappings
}

fn utf16_units(object: &Object) -> Option<Vec<u16>> {
    let Object::String(bytes) = object else {
        return None;
    };
    if bytes.is_empty() {
        return None;
    }

    Some(
        bytes
            .chunks(UTF16_BYTES)
            .map(|chunk| match chunk {
                [high, low] => u16::from_be_bytes([*high, *low]),
                [single] => u16::from(*single),
                _ => 0,
            })
            .collect(),
    )
}

fn code_of(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .take(MAX_CODE_BYTES)
        .fold(0, |code, byte| (code << 8) | u32::from(*byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY_PREAMBLE: &str = concat!(
        "/CIDInit /ProcSet findresource begin\n",
        "12 dict begin\n",
        "begincmap\n",
        "/CMapName /Adobe-Identity-UCS def\n",
        "/CMapType 2 def\n",
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    const EPILOGUE: &str = "endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";

    fn cmap(body: &str) -> Vec<u8> {
        format!("{IDENTITY_PREAMBLE}{body}{EPILOGUE}").into_bytes()
    }

    fn parsed(body: &str) -> ToUnicode {
        ToUnicode::parse(&cmap(body)).unwrap()
    }

    mod parse {
        use super::*;

        #[test]
        fn when_parse_with_empty_cmap_then_maps_nothing() {
            let map = parsed("");

            assert_eq!(map.lookup(0x0003), None);
        }

        #[test]
        fn when_parse_with_bf_char_then_maps_that_code() {
            let map = parsed("1 beginbfchar\n<0024> <0041>\nendbfchar\n");

            assert_eq!(map.lookup(0x0024), Some("A".to_string()));
        }

        #[test]
        fn when_parse_with_bf_range_then_maps_every_code_in_it() {
            let map = parsed("1 beginbfrange\n<0025> <0027> <0042>\nendbfrange\n");

            assert_eq!(map.lookup(0x0025), Some("B".to_string()));
            assert_eq!(map.lookup(0x0026), Some("C".to_string()));
            assert_eq!(map.lookup(0x0027), Some("D".to_string()));
        }

        #[test]
        fn when_parse_with_bf_range_then_maps_nothing_outside_it() {
            let map = parsed("1 beginbfrange\n<0025> <0027> <0042>\nendbfrange\n");

            assert_eq!(map.lookup(0x0024), None);
            assert_eq!(map.lookup(0x0028), None);
        }

        #[test]
        fn when_parse_with_bf_range_array_then_maps_each_code_to_its_element() {
            let map = parsed("1 beginbfrange\n<0030> <0032> [<0058> <0059> <005A>]\nendbfrange\n");

            assert_eq!(map.lookup(0x0030), Some("X".to_string()));
            assert_eq!(map.lookup(0x0031), Some("Y".to_string()));
            assert_eq!(map.lookup(0x0032), Some("Z".to_string()));
        }

        #[test]
        fn when_parse_with_bf_range_array_longer_than_range_then_ignores_the_surplus() {
            let map = parsed("1 beginbfrange\n<0030> <0031> [<0058> <0059> <005A>]\nendbfrange\n");

            assert_eq!(map.lookup(0x0032), None);
        }

        #[test]
        fn when_parse_with_descending_bf_range_then_maps_nothing() {
            let map = parsed("1 beginbfrange\n<0031> <0030> <0041>\nendbfrange\n");

            assert_eq!(map.lookup(0x0030), None);
            assert_eq!(map.lookup(0x0031), None);
        }

        #[test]
        fn when_parse_with_multi_unit_target_then_maps_to_the_whole_text() {
            let map = parsed("1 beginbfchar\n<0001> <00660069>\nendbfchar\n");

            assert_eq!(map.lookup(0x0001), Some("fi".to_string()));
        }

        #[test]
        fn when_parse_with_surrogate_pair_target_then_maps_to_that_char() {
            let map = parsed("1 beginbfchar\n<0002> <D83DDE00>\nendbfchar\n");

            assert_eq!(map.lookup(0x0002), Some("\u{1F600}".to_string()));
        }

        #[test]
        fn when_parse_with_multibyte_target_then_maps_to_that_char() {
            let map = parsed("1 beginbfchar\n<0005> <65E5>\nendbfchar\n");

            assert_eq!(map.lookup(0x0005), Some("日".to_string()));
        }

        #[test]
        fn when_parse_with_several_sections_then_maps_all_of_them() {
            let map = parsed(concat!(
                "1 beginbfchar\n<0003> <0020>\nendbfchar\n",
                "1 beginbfrange\n<0025> <0027> <0042>\nendbfrange\n",
            ));

            assert_eq!(map.lookup(0x0003), Some(" ".to_string()));
            assert_eq!(map.lookup(0x0026), Some("C".to_string()));
        }

        #[test]
        fn when_parse_with_malformed_content_then_returns_error() {
            assert_eq!(
                ToUnicode::parse(b"1 beginbfchar\n<0024"),
                Err(PdfError::UnexpectedEof)
            );
        }
    }

    mod decode {
        use super::*;

        #[test]
        fn when_decode_with_two_byte_codes_then_returns_the_mapped_text() {
            let map = parsed("2 beginbfchar\n<0024> <0041>\n<0025> <0042>\nendbfchar\n");

            assert_eq!(map.decode(&[0x00, 0x24, 0x00, 0x25]), "AB".to_string());
        }

        #[test]
        fn when_decode_with_unmapped_code_then_skips_it() {
            let map = parsed("1 beginbfchar\n<0024> <0041>\nendbfchar\n");

            assert_eq!(map.decode(&[0x00, 0x24, 0x00, 0xFF]), "A".to_string());
        }

        #[test]
        fn when_decode_with_truncated_code_then_returns_the_text_so_far() {
            let map = parsed("1 beginbfchar\n<0024> <0041>\nendbfchar\n");

            assert_eq!(map.decode(&[0x00, 0x24, 0x00]), "A".to_string());
        }

        #[test]
        fn when_decode_with_one_byte_codespace_then_reads_one_byte_at_a_time() {
            let source = concat!(
                "begincmap\n",
                "1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
                "2 beginbfchar\n<41> <0041>\n<42> <0042>\nendbfchar\n",
                "endcmap\n",
            );
            let map = ToUnicode::parse(source.as_bytes()).unwrap();

            assert_eq!(map.decode(b"AB"), "AB".to_string());
        }

        #[test]
        fn when_decode_with_mixed_codespaces_then_reads_each_code_at_its_own_width() {
            let source = concat!(
                "begincmap\n",
                "2 begincodespacerange\n<20> <7E>\n<8140> <9FFC>\nendcodespacerange\n",
                "2 beginbfchar\n<41> <0041>\n<8140> <3042>\nendbfchar\n",
                "endcmap\n",
            );
            let map = ToUnicode::parse(source.as_bytes()).unwrap();

            assert_eq!(map.decode(&[0x41, 0x81, 0x40]), "Aあ".to_string());
        }

        #[test]
        fn when_decode_without_codespace_then_reads_one_byte_at_a_time() {
            let source = "begincmap\n1 beginbfchar\n<41> <0041>\nendbfchar\nendcmap\n";
            let map = ToUnicode::parse(source.as_bytes()).unwrap();

            assert_eq!(map.decode(b"A"), "A".to_string());
        }

        #[test]
        fn when_decode_with_no_bytes_then_returns_empty_text() {
            let map = parsed("1 beginbfchar\n<0024> <0041>\nendbfchar\n");

            assert_eq!(map.decode(&[]), String::new());
        }
    }
}
