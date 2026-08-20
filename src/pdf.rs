const HEADER_PREFIX: &[u8] = b"%PDF-";
const HEADER_SEARCH_LIMIT: usize = 1024;
const STARTXREF_KEYWORD: &[u8] = b"startxref";
const XREF_KEYWORD: &[u8] = b"xref";
const TRAILER_KEYWORD: &[u8] = b"trailer";
const WHITESPACE: [u8; 6] = *b"\0\t\n\x0c\r ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrefEntryKind {
    InUse { offset: usize },
    Free { next_free: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XrefEntry {
    pub object_number: u32,
    pub generation: u16,
    pub kind: XrefEntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrefTable {
    pub entries: Vec<XrefEntry>,
    pub trailer_offset: usize,
}

impl XrefTable {
    pub fn offset_of(&self, object_number: u32) -> Option<usize> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.object_number == object_number)
            .and_then(|entry| match entry.kind {
                XrefEntryKind::InUse { offset } => Some(offset),
                XrefEntryKind::Free { .. } => None,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    HeaderNotFound,
    StartxrefNotFound,
    InvalidStartxref,
    XrefNotFound,
    MalformedXrefEntry,
    TrailerNotFound,
    UnexpectedEof,
}

impl core::fmt::Display for PdfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PdfError::HeaderNotFound => write!(f, "pdf header not found"),
            PdfError::StartxrefNotFound => write!(f, "startxref not found"),
            PdfError::InvalidStartxref => write!(f, "startxref does not point at an offset"),
            PdfError::XrefNotFound => write!(f, "xref table not found at the startxref offset"),
            PdfError::MalformedXrefEntry => write!(f, "malformed xref entry"),
            PdfError::TrailerNotFound => write!(f, "trailer not found after the xref table"),
            PdfError::UnexpectedEof => write!(f, "unexpected end of file"),
        }
    }
}

pub fn parse_header(data: &[u8]) -> Result<Version, PdfError> {
    let limit = data.len().min(HEADER_SEARCH_LIMIT);
    let start = find(&data[..limit], HEADER_PREFIX).ok_or(PdfError::HeaderNotFound)?;

    let (major, after_major) =
        read_number(data, start + HEADER_PREFIX.len()).ok_or(PdfError::HeaderNotFound)?;
    if data.get(after_major) != Some(&b'.') {
        return Err(PdfError::HeaderNotFound);
    }
    let (minor, _) = read_number(data, after_major + 1).ok_or(PdfError::HeaderNotFound)?;

    Ok(Version {
        major: u8::try_from(major).map_err(|_| PdfError::HeaderNotFound)?,
        minor: u8::try_from(minor).map_err(|_| PdfError::HeaderNotFound)?,
    })
}

pub fn find_startxref(data: &[u8]) -> Result<usize, PdfError> {
    let keyword = rfind(data, STARTXREF_KEYWORD).ok_or(PdfError::StartxrefNotFound)?;
    let value_start = skip_whitespace(data, keyword + STARTXREF_KEYWORD.len());
    let (offset, _) = read_number(data, value_start).ok_or(PdfError::InvalidStartxref)?;

    usize::try_from(offset).map_err(|_| PdfError::InvalidStartxref)
}

pub fn parse_xref_table(data: &[u8], offset: usize) -> Result<XrefTable, PdfError> {
    let mut position = skip_whitespace(data, offset.min(data.len()));
    if !data[position..].starts_with(XREF_KEYWORD) {
        return Err(PdfError::XrefNotFound);
    }
    position += XREF_KEYWORD.len();

    let mut entries = Vec::new();
    loop {
        position = skip_whitespace(data, position);
        if position >= data.len() {
            return Err(PdfError::TrailerNotFound);
        }
        if data[position..].starts_with(TRAILER_KEYWORD) {
            return Ok(XrefTable {
                entries,
                trailer_offset: skip_whitespace(data, position + TRAILER_KEYWORD.len()),
            });
        }

        let (first_object_number, after_first) =
            read_number(data, position).ok_or(PdfError::MalformedXrefEntry)?;
        let (count, after_count) = read_number(data, skip_whitespace(data, after_first))
            .ok_or(PdfError::MalformedXrefEntry)?;
        if count > (data.len() - after_count) as u64 {
            return Err(PdfError::MalformedXrefEntry);
        }
        position = after_count;

        for index in 0..count {
            let (entry, after_entry) = read_entry(data, position, first_object_number + index)?;
            entries.push(entry);
            position = after_entry;
        }
    }
}

pub fn read_xref_table(data: &[u8]) -> Result<XrefTable, PdfError> {
    parse_xref_table(data, find_startxref(data)?)
}

fn read_entry(
    data: &[u8],
    from: usize,
    object_number: u64,
) -> Result<(XrefEntry, usize), PdfError> {
    let (location, after_location) =
        read_number(data, skip_whitespace(data, from)).ok_or(PdfError::MalformedXrefEntry)?;
    let (generation, after_generation) = read_number(data, skip_whitespace(data, after_location))
        .ok_or(PdfError::MalformedXrefEntry)?;

    let marker_position = skip_whitespace(data, after_generation);
    let marker = *data.get(marker_position).ok_or(PdfError::UnexpectedEof)?;
    let kind = match marker {
        b'n' => XrefEntryKind::InUse {
            offset: usize::try_from(location).map_err(|_| PdfError::MalformedXrefEntry)?,
        },
        b'f' => XrefEntryKind::Free {
            next_free: u32::try_from(location).map_err(|_| PdfError::MalformedXrefEntry)?,
        },
        _ => return Err(PdfError::MalformedXrefEntry),
    };

    let entry = XrefEntry {
        object_number: u32::try_from(object_number).map_err(|_| PdfError::MalformedXrefEntry)?,
        generation: u16::try_from(generation).map_err(|_| PdfError::MalformedXrefEntry)?,
        kind,
    };

    Ok((entry, marker_position + 1))
}

fn read_number(data: &[u8], from: usize) -> Option<(u64, usize)> {
    let mut position = from;
    let mut value: u64 = 0;
    while let Some(digit) = data.get(position).filter(|byte| byte.is_ascii_digit()) {
        value = value.checked_mul(10)?.checked_add((digit - b'0') as u64)?;
        position += 1;
    }

    (position > from).then_some((value, position))
}

fn skip_whitespace(data: &[u8], from: usize) -> usize {
    let mut position = from;
    while data
        .get(position)
        .is_some_and(|byte| WHITESPACE.contains(byte))
    {
        position += 1;
    }
    position
}

fn find(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).position(|w| w == needle)
}

fn rfind(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).rposition(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_line(location: u64, generation: u16, marker: char) -> String {
        format!("{location:010} {generation:05} {marker} \n")
    }

    fn xref_section(body: &str) -> String {
        format!("xref\n{body}trailer\n<< /Size 2 /Root 1 0 R >>\n")
    }

    fn minimal_pdf() -> Vec<u8> {
        let body = "%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n";
        let table = xref_section(&format!(
            "0 2\n{}{}",
            entry_line(0, 65535, 'f'),
            entry_line(9, 0, 'n')
        ));
        format!("{body}{table}startxref\n{}\n%%EOF\n", body.len()).into_bytes()
    }

    mod parse_header {
        use super::*;

        #[test]
        fn when_parse_with_version_1_7_then_returns_version() {
            let result = parse_header(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n");
            assert_eq!(result, Ok(Version { major: 1, minor: 7 }));
        }

        #[test]
        fn when_parse_with_version_2_0_then_returns_version() {
            let result = parse_header(b"%PDF-2.0\n");
            assert_eq!(result, Ok(Version { major: 2, minor: 0 }));
        }

        #[test]
        fn when_parse_with_leading_junk_then_returns_version() {
            let result = parse_header(b"junk bytes\n%PDF-1.4\n");
            assert_eq!(result, Ok(Version { major: 1, minor: 4 }));
        }

        #[test]
        fn when_parse_with_header_beyond_search_limit_then_returns_error() {
            let mut data = vec![b'x'; HEADER_SEARCH_LIMIT];
            data.extend_from_slice(b"%PDF-1.4\n");
            assert_eq!(parse_header(&data), Err(PdfError::HeaderNotFound));
        }

        #[test]
        fn when_parse_with_no_header_then_returns_error() {
            let result = parse_header(b"this is not a pdf at all");
            assert_eq!(result, Err(PdfError::HeaderNotFound));
        }

        #[test]
        fn when_parse_with_missing_minor_then_returns_error() {
            let result = parse_header(b"%PDF-1\n");
            assert_eq!(result, Err(PdfError::HeaderNotFound));
        }

        #[test]
        fn when_parse_with_non_numeric_version_then_returns_error() {
            let result = parse_header(b"%PDF-x.y\n");
            assert_eq!(result, Err(PdfError::HeaderNotFound));
        }
    }

    mod find_startxref {
        use super::*;

        #[test]
        fn when_find_with_trailing_startxref_then_returns_offset() {
            let result = find_startxref(b"%PDF-1.7\nstartxref\n123\n%%EOF\n");
            assert_eq!(result, Ok(123));
        }

        #[test]
        fn when_find_with_incremental_update_then_returns_last_offset() {
            let result = find_startxref(b"startxref\n123\n%%EOF\nstartxref\n456\n%%EOF\n");
            assert_eq!(result, Ok(456));
        }

        #[test]
        fn when_find_with_no_keyword_then_returns_error() {
            let result = find_startxref(b"%PDF-1.7\n%%EOF\n");
            assert_eq!(result, Err(PdfError::StartxrefNotFound));
        }

        #[test]
        fn when_find_with_non_numeric_value_then_returns_error() {
            let result = find_startxref(b"startxref\nnowhere\n%%EOF\n");
            assert_eq!(result, Err(PdfError::InvalidStartxref));
        }
    }

    mod parse_xref_table {
        use super::*;

        #[test]
        fn when_parse_with_single_subsection_then_returns_entries() {
            let data = xref_section(&format!(
                "0 2\n{}{}",
                entry_line(0, 65535, 'f'),
                entry_line(9, 0, 'n')
            ));
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            assert_eq!(
                table.entries,
                vec![
                    XrefEntry {
                        object_number: 0,
                        generation: 65535,
                        kind: XrefEntryKind::Free { next_free: 0 },
                    },
                    XrefEntry {
                        object_number: 1,
                        generation: 0,
                        kind: XrefEntryKind::InUse { offset: 9 },
                    },
                ]
            );
        }

        #[test]
        fn when_parse_with_subsection_starting_at_three_then_numbers_from_three() {
            let data = xref_section(&format!(
                "3 2\n{}{}",
                entry_line(17, 0, 'n'),
                entry_line(81, 2, 'n')
            ));
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            let numbers: Vec<u32> = table.entries.iter().map(|e| e.object_number).collect();
            assert_eq!(numbers, vec![3, 4]);
        }

        #[test]
        fn when_parse_with_multiple_subsections_then_returns_all_entries() {
            let data = xref_section(&format!(
                "0 1\n{}5 1\n{}",
                entry_line(0, 65535, 'f'),
                entry_line(42, 0, 'n')
            ));
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            assert_eq!(table.entries.len(), 2);
            assert_eq!(table.offset_of(5), Some(42));
        }

        #[test]
        fn when_parse_with_single_eol_entries_then_returns_entries() {
            let data = xref_section("0 2\n0000000000 65535 f\n0000000009 00000 n\n");
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            assert_eq!(table.offset_of(1), Some(9));
        }

        #[test]
        fn when_parse_with_nonzero_offset_then_reads_at_position() {
            let data = format!(
                "%PDF-1.7\n{}",
                xref_section(&format!("0 1\n{}", entry_line(0, 65535, 'f')))
            );
            let table = parse_xref_table(data.as_bytes(), 9).unwrap();
            assert_eq!(table.entries.len(), 1);
        }

        #[test]
        fn when_parse_with_trailer_then_returns_offset_of_dictionary() {
            let data = xref_section(&format!("0 1\n{}", entry_line(0, 65535, 'f')));
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            assert_eq!(&data.as_bytes()[table.trailer_offset..][..2], b"<<");
        }

        #[test]
        fn when_parse_with_missing_xref_keyword_then_returns_error() {
            let result = parse_xref_table(b"trailer\n<< >>\n", 0);
            assert_eq!(result, Err(PdfError::XrefNotFound));
        }

        #[test]
        fn when_parse_with_offset_beyond_input_then_returns_error() {
            let data = xref_section(&format!("0 1\n{}", entry_line(0, 65535, 'f')));
            let result = parse_xref_table(data.as_bytes(), data.len() + 100);
            assert_eq!(result, Err(PdfError::XrefNotFound));
        }

        #[test]
        fn when_parse_with_missing_trailer_then_returns_error() {
            let data = format!("xref\n0 1\n{}", entry_line(0, 65535, 'f'));
            let result = parse_xref_table(data.as_bytes(), 0);
            assert_eq!(result, Err(PdfError::TrailerNotFound));
        }

        #[test]
        fn when_parse_with_invalid_marker_then_returns_error() {
            let data = xref_section("0 1\n0000000009 00000 x \n");
            let result = parse_xref_table(data.as_bytes(), 0);
            assert_eq!(result, Err(PdfError::MalformedXrefEntry));
        }

        #[test]
        fn when_parse_with_truncated_entry_then_returns_error() {
            let result = parse_xref_table(b"xref\n0 1\n0000000009 00000 ", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_missing_generation_then_returns_error() {
            let data = xref_section("0 1\n0000000009\n");
            let result = parse_xref_table(data.as_bytes(), 0);
            assert_eq!(result, Err(PdfError::MalformedXrefEntry));
        }

        #[test]
        fn when_parse_with_count_beyond_input_then_returns_error() {
            let data = xref_section(&format!("0 9999\n{}", entry_line(0, 65535, 'f')));
            let result = parse_xref_table(data.as_bytes(), 0);
            assert_eq!(result, Err(PdfError::MalformedXrefEntry));
        }

        #[test]
        fn when_parse_with_non_numeric_subsection_then_returns_error() {
            let data = xref_section("zero one\n");
            let result = parse_xref_table(data.as_bytes(), 0);
            assert_eq!(result, Err(PdfError::MalformedXrefEntry));
        }
    }

    mod read_xref_table {
        use super::*;

        #[test]
        fn when_read_with_minimal_pdf_then_returns_entries() {
            let table = read_xref_table(&minimal_pdf()).unwrap();
            assert_eq!(table.entries.len(), 2);
            assert_eq!(table.offset_of(1), Some(9));
        }

        #[test]
        fn when_read_with_no_startxref_then_returns_error() {
            let result = read_xref_table(b"%PDF-1.7\n%%EOF\n");
            assert_eq!(result, Err(PdfError::StartxrefNotFound));
        }

        #[test]
        fn when_read_with_startxref_pointing_at_body_then_returns_error() {
            let result = read_xref_table(b"%PDF-1.7\nstartxref\n0\n%%EOF\n");
            assert_eq!(result, Err(PdfError::XrefNotFound));
        }
    }

    mod offset_of {
        use super::*;

        #[test]
        fn when_offset_of_with_in_use_entry_then_returns_offset() {
            let table = read_xref_table(&minimal_pdf()).unwrap();
            assert_eq!(table.offset_of(1), Some(9));
        }

        #[test]
        fn when_offset_of_with_free_entry_then_returns_none() {
            let table = read_xref_table(&minimal_pdf()).unwrap();
            assert_eq!(table.offset_of(0), None);
        }

        #[test]
        fn when_offset_of_with_unknown_object_then_returns_none() {
            let table = read_xref_table(&minimal_pdf()).unwrap();
            assert_eq!(table.offset_of(7), None);
        }

        #[test]
        fn when_offset_of_with_object_freed_in_later_subsection_then_returns_none() {
            let data = xref_section(&format!(
                "1 1\n{}1 1\n{}",
                entry_line(42, 0, 'n'),
                entry_line(0, 1, 'f')
            ));
            let table = parse_xref_table(data.as_bytes(), 0).unwrap();
            assert_eq!(table.offset_of(1), None);
        }
    }
}
