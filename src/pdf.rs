const HEADER_PREFIX: &[u8] = b"%PDF-";
const HEADER_SEARCH_LIMIT: usize = 1024;
const STARTXREF_KEYWORD: &[u8] = b"startxref";
const XREF_KEYWORD: &[u8] = b"xref";
const TRAILER_KEYWORD: &[u8] = b"trailer";
const WHITESPACE: [u8; 6] = *b"\0\t\n\x0c\r ";
const OBJ_KEYWORD: &[u8] = b"obj";
const ENDOBJ_KEYWORD: &[u8] = b"endobj";
const STREAM_KEYWORD: &[u8] = b"stream";
const ENDSTREAM_KEYWORD: &[u8] = b"endstream";
const TRUE_KEYWORD: &[u8] = b"true";
const FALSE_KEYWORD: &[u8] = b"false";
const NULL_KEYWORD: &[u8] = b"null";
const DICTIONARY_OPEN: &[u8] = b"<<";
const DICTIONARY_CLOSE: &[u8] = b">>";
const DELIMITERS: [u8; 10] = *b"()<>[]{}/%";
const LENGTH_KEY: &str = "Length";
const MAX_REFERENCE_DEPTH: usize = 32;

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

pub type Dictionary = Vec<(String, Object)>;

#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Object>),
    Dictionary(Dictionary),
    Stream {
        dictionary: Dictionary,
        data: Vec<u8>,
    },
    Reference {
        object_number: u32,
        generation: u16,
    },
}

impl Object {
    pub fn get(&self, key: &str) -> Option<&Object> {
        let entries = match self {
            Object::Dictionary(entries) => entries,
            Object::Stream { dictionary, .. } => dictionary,
            _ => return None,
        };
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Object::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Object::Integer(value) => Some(*value as f64),
            Object::Real(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&str> {
        match self {
            Object::Name(name) => Some(name),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Object]> {
        match self {
            Object::Array(items) => Some(items),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndirectObject {
    pub object_number: u32,
    pub generation: u16,
    pub object: Object,
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
    MalformedObject,
    ObjectNotFound,
    ObjectNumberMismatch,
    CircularReference,
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
            PdfError::MalformedObject => write!(f, "malformed object"),
            PdfError::ObjectNotFound => write!(f, "object not found in the xref table"),
            PdfError::ObjectNumberMismatch => {
                write!(f, "object number does not match the xref table")
            }
            PdfError::CircularReference => write!(f, "indirect references form a cycle"),
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

pub fn parse_object(data: &[u8], offset: usize) -> Result<(Object, usize), PdfError> {
    let position = skip_blanks(data, offset.min(data.len()));
    match *data.get(position).ok_or(PdfError::UnexpectedEof)? {
        b'/' => read_name(data, position),
        b'(' => read_literal_string(data, position),
        b'[' => read_array(data, position),
        b'<' if data[position..].starts_with(DICTIONARY_OPEN) => read_dictionary(data, position),
        b'<' => read_hex_string(data, position),
        b't' => read_keyword(data, position, TRUE_KEYWORD, Object::Boolean(true)),
        b'f' => read_keyword(data, position, FALSE_KEYWORD, Object::Boolean(false)),
        b'n' => read_keyword(data, position, NULL_KEYWORD, Object::Null),
        b'+' | b'-' | b'.' => read_numeric(data, position),
        byte if byte.is_ascii_digit() => read_numeric(data, position),
        _ => Err(PdfError::MalformedObject),
    }
}

pub fn parse_indirect_object(
    data: &[u8],
    offset: usize,
) -> Result<(IndirectObject, usize), PdfError> {
    let start = skip_blanks(data, offset.min(data.len()));
    let (object_number, after_number) =
        read_number(data, start).ok_or(PdfError::MalformedObject)?;
    let (generation, after_generation) =
        read_number(data, skip_blanks(data, after_number)).ok_or(PdfError::MalformedObject)?;

    let keyword = skip_blanks(data, after_generation);
    if !data[keyword..].starts_with(OBJ_KEYWORD) {
        return Err(PdfError::MalformedObject);
    }

    let (object, after_object) = parse_object(data, keyword + OBJ_KEYWORD.len())?;
    let end = skip_blanks(data, after_object);
    if !data[end..].starts_with(ENDOBJ_KEYWORD) {
        return Err(PdfError::MalformedObject);
    }

    let indirect = IndirectObject {
        object_number: u32::try_from(object_number).map_err(|_| PdfError::MalformedObject)?,
        generation: u16::try_from(generation).map_err(|_| PdfError::MalformedObject)?,
        object,
    };

    Ok((indirect, end + ENDOBJ_KEYWORD.len()))
}

pub fn get_object(data: &[u8], table: &XrefTable, object_number: u32) -> Result<Object, PdfError> {
    let offset = table
        .offset_of(object_number)
        .ok_or(PdfError::ObjectNotFound)?;
    let (indirect, _) = parse_indirect_object(data, offset)?;
    if indirect.object_number != object_number {
        return Err(PdfError::ObjectNumberMismatch);
    }

    Ok(indirect.object)
}

pub fn resolve(data: &[u8], table: &XrefTable, object: &Object) -> Result<Object, PdfError> {
    let mut current = object.clone();
    for _ in 0..MAX_REFERENCE_DEPTH {
        match current {
            Object::Reference { object_number, .. } => {
                current = get_object(data, table, object_number)?
            }
            resolved => return Ok(resolved),
        }
    }

    Err(PdfError::CircularReference)
}

fn read_keyword(
    data: &[u8],
    from: usize,
    keyword: &[u8],
    object: Object,
) -> Result<(Object, usize), PdfError> {
    if !data[from..].starts_with(keyword) {
        return Err(PdfError::MalformedObject);
    }

    Ok((object, from + keyword.len()))
}

fn read_numeric(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    let end = numeric_token_end(data, from);
    let token = core::str::from_utf8(&data[from..end]).map_err(|_| PdfError::MalformedObject)?;
    if token.contains('.') {
        let value = token
            .parse::<f64>()
            .map_err(|_| PdfError::MalformedObject)?;
        return Ok((Object::Real(value), end));
    }

    let value = token
        .parse::<i64>()
        .map_err(|_| PdfError::MalformedObject)?;
    match (read_reference_tail(data, end), u32::try_from(value)) {
        (Some((generation, after_reference)), Ok(object_number)) => Ok((
            Object::Reference {
                object_number,
                generation,
            },
            after_reference,
        )),
        _ => Ok((Object::Integer(value), end)),
    }
}

fn read_reference_tail(data: &[u8], from: usize) -> Option<(u16, usize)> {
    let generation_start = skip_blanks(data, from);
    if generation_start == from {
        return None;
    }

    let (generation, after_generation) = read_number(data, generation_start)?;
    let keyword = skip_blanks(data, after_generation);
    if data.get(keyword) != Some(&b'R') {
        return None;
    }
    if data.get(keyword + 1).is_some_and(|byte| is_regular(*byte)) {
        return None;
    }

    Some((u16::try_from(generation).ok()?, keyword + 1))
}

fn read_name(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    read_name_text(data, from).map(|(name, after)| (Object::Name(name), after))
}

fn read_name_text(data: &[u8], from: usize) -> Result<(String, usize), PdfError> {
    let mut position = from + 1;
    let mut bytes = Vec::new();
    while let Some(byte) = data.get(position).copied().filter(|byte| is_regular(*byte)) {
        if byte != b'#' {
            bytes.push(byte);
            position += 1;
            continue;
        }

        let high = hex_value(*data.get(position + 1).ok_or(PdfError::UnexpectedEof)?)
            .ok_or(PdfError::MalformedObject)?;
        let low = hex_value(*data.get(position + 2).ok_or(PdfError::UnexpectedEof)?)
            .ok_or(PdfError::MalformedObject)?;
        bytes.push(high * 16 + low);
        position += 3;
    }

    let name = String::from_utf8(bytes).map_err(|_| PdfError::MalformedObject)?;
    Ok((name, position))
}

fn read_literal_string(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    let mut position = from + 1;
    let mut depth = 1usize;
    let mut bytes = Vec::new();
    while let Some(byte) = data.get(position).copied() {
        position += 1;
        match byte {
            b'(' => {
                depth += 1;
                bytes.push(byte);
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((Object::String(bytes), position));
                }
                bytes.push(byte);
            }
            b'\\' => {
                let escaped = *data.get(position).ok_or(PdfError::UnexpectedEof)?;
                position = read_escape(data, position, &mut bytes, escaped);
            }
            b'\r' => {
                bytes.push(b'\n');
                if data.get(position) == Some(&b'\n') {
                    position += 1;
                }
            }
            _ => bytes.push(byte),
        }
    }

    Err(PdfError::UnexpectedEof)
}

fn read_escape(data: &[u8], from: usize, bytes: &mut Vec<u8>, escaped: u8) -> usize {
    let mut position = from + 1;
    match escaped {
        b'n' => bytes.push(b'\n'),
        b'r' => bytes.push(b'\r'),
        b't' => bytes.push(b'\t'),
        b'b' => bytes.push(0x08),
        b'f' => bytes.push(0x0c),
        b'\n' => {}
        b'\r' => {
            if data.get(position) == Some(&b'\n') {
                position += 1;
            }
        }
        b'0'..=b'7' => {
            let mut value = u32::from(escaped - b'0');
            for _ in 0..2 {
                match data
                    .get(position)
                    .copied()
                    .filter(|byte| (b'0'..=b'7').contains(byte))
                {
                    Some(digit) => {
                        value = value * 8 + u32::from(digit - b'0');
                        position += 1;
                    }
                    None => break,
                }
            }
            bytes.push((value & 0xff) as u8);
        }
        other => bytes.push(other),
    }

    position
}

fn read_hex_string(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    let mut position = from + 1;
    let mut bytes = Vec::new();
    let mut pending: Option<u8> = None;
    while let Some(byte) = data.get(position).copied() {
        position += 1;
        if byte == b'>' {
            if let Some(high) = pending {
                bytes.push(high * 16);
            }
            return Ok((Object::String(bytes), position));
        }
        if WHITESPACE.contains(&byte) {
            continue;
        }

        let value = hex_value(byte).ok_or(PdfError::MalformedObject)?;
        match pending.take() {
            Some(high) => bytes.push(high * 16 + value),
            None => pending = Some(value),
        }
    }

    Err(PdfError::UnexpectedEof)
}

fn read_array(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    let mut position = from + 1;
    let mut items = Vec::new();
    loop {
        position = skip_blanks(data, position);
        match data.get(position) {
            None => return Err(PdfError::UnexpectedEof),
            Some(b']') => return Ok((Object::Array(items), position + 1)),
            Some(_) => {
                let (object, after_object) = parse_object(data, position)?;
                items.push(object);
                position = after_object;
            }
        }
    }
}

fn read_dictionary(data: &[u8], from: usize) -> Result<(Object, usize), PdfError> {
    let mut position = from + DICTIONARY_OPEN.len();
    let mut entries: Dictionary = Vec::new();
    loop {
        position = skip_blanks(data, position);
        if position >= data.len() {
            return Err(PdfError::UnexpectedEof);
        }
        if data[position..].starts_with(DICTIONARY_CLOSE) {
            position += DICTIONARY_CLOSE.len();
            break;
        }
        if data[position] != b'/' {
            return Err(PdfError::MalformedObject);
        }

        let (key, after_key) = read_name_text(data, position)?;
        let (value, after_value) = parse_object(data, after_key)?;
        entries.push((key, value));
        position = after_value;
    }

    read_stream(data, entries, position)
}

fn read_stream(data: &[u8], entries: Dictionary, from: usize) -> Result<(Object, usize), PdfError> {
    let keyword = skip_blanks(data, from);
    if !data[keyword..].starts_with(STREAM_KEYWORD) {
        return Ok((Object::Dictionary(entries), from));
    }

    let after_keyword = keyword + STREAM_KEYWORD.len();
    let start = match data.get(after_keyword) {
        Some(b'\r') if data.get(after_keyword + 1) == Some(&b'\n') => after_keyword + 2,
        Some(b'\n') => after_keyword + 1,
        _ => return Err(PdfError::MalformedObject),
    };

    let (end, after_stream) = stream_bounds(data, &entries, start)?;
    let object = Object::Stream {
        dictionary: entries,
        data: data[start..end].to_vec(),
    };

    Ok((object, after_stream))
}

fn stream_bounds(
    data: &[u8],
    entries: &[(String, Object)],
    start: usize,
) -> Result<(usize, usize), PdfError> {
    let declared = entries
        .iter()
        .find(|(key, _)| key == LENGTH_KEY)
        .and_then(|(_, value)| value.as_i64())
        .and_then(|length| usize::try_from(length).ok())
        .and_then(|length| start.checked_add(length))
        .filter(|end| *end <= data.len());
    if let Some(end) = declared
        && data[skip_whitespace(data, end)..].starts_with(ENDSTREAM_KEYWORD)
    {
        let keyword = skip_whitespace(data, end);
        return Ok((end, keyword + ENDSTREAM_KEYWORD.len()));
    }

    let keyword = find(&data[start..], ENDSTREAM_KEYWORD)
        .map(|position| start + position)
        .ok_or(PdfError::UnexpectedEof)?;

    Ok((trim_eol(data, keyword), keyword + ENDSTREAM_KEYWORD.len()))
}

fn trim_eol(data: &[u8], end: usize) -> usize {
    let without_lf = if end > 0 && data[end - 1] == b'\n' {
        end - 1
    } else {
        end
    };

    if without_lf > 0 && data[without_lf - 1] == b'\r' {
        without_lf - 1
    } else {
        without_lf
    }
}

fn numeric_token_end(data: &[u8], from: usize) -> usize {
    let mut position = from;
    while data
        .get(position)
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
    {
        position += 1;
    }
    position
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_regular(byte: u8) -> bool {
    !WHITESPACE.contains(&byte) && !DELIMITERS.contains(&byte)
}

fn skip_blanks(data: &[u8], from: usize) -> usize {
    let mut position = skip_whitespace(data, from);
    while data.get(position) == Some(&b'%') {
        while data
            .get(position)
            .is_some_and(|byte| *byte != b'\n' && *byte != b'\r')
        {
            position += 1;
        }
        position = skip_whitespace(data, position);
    }

    position
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

    fn pdf_with_objects(objects: &[&str]) -> Vec<u8> {
        let mut body = String::from("%PDF-1.7\n");
        let mut entries = entry_line(0, 65535, 'f');
        for (index, object) in objects.iter().enumerate() {
            entries.push_str(&entry_line(body.len() as u64, 0, 'n'));
            body.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let table = xref_section(&format!("0 {}\n{entries}", objects.len() + 1));

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

    mod parse_object {
        use super::*;

        #[test]
        fn when_parse_with_null_then_returns_null() {
            let result = parse_object(b"null", 0);
            assert_eq!(result, Ok((Object::Null, 4)));
        }

        #[test]
        fn when_parse_with_true_then_returns_boolean() {
            let result = parse_object(b"true", 0);
            assert_eq!(result, Ok((Object::Boolean(true), 4)));
        }

        #[test]
        fn when_parse_with_false_then_returns_boolean() {
            let result = parse_object(b"false", 0);
            assert_eq!(result, Ok((Object::Boolean(false), 5)));
        }

        #[test]
        fn when_parse_with_truncated_keyword_then_returns_error() {
            let result = parse_object(b"tru", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_integer_then_returns_integer() {
            let result = parse_object(b"42", 0);
            assert_eq!(result, Ok((Object::Integer(42), 2)));
        }

        #[test]
        fn when_parse_with_signed_integer_then_returns_integer() {
            let result = parse_object(b"-17", 0);
            assert_eq!(result, Ok((Object::Integer(-17), 3)));
        }

        #[test]
        fn when_parse_with_real_then_returns_real() {
            let result = parse_object(b"34.5", 0);
            assert_eq!(result, Ok((Object::Real(34.5), 4)));
        }

        #[test]
        fn when_parse_with_real_without_integer_part_then_returns_real() {
            let result = parse_object(b"-.002", 0);
            assert_eq!(result, Ok((Object::Real(-0.002), 5)));
        }

        #[test]
        fn when_parse_with_real_without_fraction_part_then_returns_real() {
            let result = parse_object(b"4.", 0);
            assert_eq!(result, Ok((Object::Real(4.0), 2)));
        }

        #[test]
        fn when_parse_with_malformed_number_then_returns_error() {
            let result = parse_object(b"--5", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_malformed_real_then_returns_error() {
            let result = parse_object(b"-.", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_name_then_returns_name() {
            let result = parse_object(b"/Type", 0);
            assert_eq!(result, Ok((Object::Name("Type".to_string()), 5)));
        }

        #[test]
        fn when_parse_with_name_containing_hex_escape_then_returns_decoded_name() {
            let result = parse_object(b"/A#20B", 0);
            assert_eq!(result, Ok((Object::Name("A B".to_string()), 6)));
        }

        #[test]
        fn when_parse_with_empty_name_then_returns_empty_name() {
            let result = parse_object(b"/ ", 0);
            assert_eq!(result, Ok((Object::Name(String::new()), 1)));
        }

        #[test]
        fn when_parse_with_invalid_hex_escape_then_returns_error() {
            let result = parse_object(b"/A#ZZ", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_truncated_hex_escape_then_returns_error() {
            let result = parse_object(b"/A#2", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_literal_string_then_returns_string() {
            let result = parse_object(b"(Hello)", 0);
            assert_eq!(result, Ok((Object::String(b"Hello".to_vec()), 7)));
        }

        #[test]
        fn when_parse_with_nested_parentheses_then_returns_string() {
            let result = parse_object(b"(a(b)c)", 0);
            assert_eq!(result, Ok((Object::String(b"a(b)c".to_vec()), 7)));
        }

        #[test]
        fn when_parse_with_escaped_characters_then_returns_string() {
            let result = parse_object(br"(a\nb\tc\(d\\e)", 0);
            assert_eq!(result, Ok((Object::String(b"a\nb\tc(d\\e".to_vec()), 15)));
        }

        #[test]
        fn when_parse_with_octal_escape_then_returns_string() {
            let result = parse_object(br"(\101\7)", 0);
            assert_eq!(result, Ok((Object::String(vec![b'A', 7]), 8)));
        }

        #[test]
        fn when_parse_with_escaped_line_break_then_returns_joined_string() {
            let result = parse_object(b"(a\\\nb)", 0);
            assert_eq!(result, Ok((Object::String(b"ab".to_vec()), 6)));
        }

        #[test]
        fn when_parse_with_carriage_return_then_returns_normalized_string() {
            let result = parse_object(b"(a\r\nb)", 0);
            assert_eq!(result, Ok((Object::String(b"a\nb".to_vec()), 6)));
        }

        #[test]
        fn when_parse_with_unknown_escape_then_returns_escaped_character() {
            let result = parse_object(br"(\q)", 0);
            assert_eq!(result, Ok((Object::String(b"q".to_vec()), 4)));
        }

        #[test]
        fn when_parse_with_unterminated_literal_string_then_returns_error() {
            let result = parse_object(b"(Hello", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_hex_string_then_returns_string() {
            let result = parse_object(b"<48656C6C6F>", 0);
            assert_eq!(result, Ok((Object::String(b"Hello".to_vec()), 12)));
        }

        #[test]
        fn when_parse_with_odd_hex_digits_then_pads_last_digit() {
            let result = parse_object(b"<48F>", 0);
            assert_eq!(result, Ok((Object::String(vec![0x48, 0xf0]), 5)));
        }

        #[test]
        fn when_parse_with_hex_string_containing_whitespace_then_returns_string() {
            let result = parse_object(b"<48 65\n6C>", 0);
            assert_eq!(result, Ok((Object::String(b"Hel".to_vec()), 10)));
        }

        #[test]
        fn when_parse_with_invalid_hex_digit_then_returns_error() {
            let result = parse_object(b"<48Z>", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_unterminated_hex_string_then_returns_error() {
            let result = parse_object(b"<4865", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_array_then_returns_array() {
            let result = parse_object(b"[1 2 3]", 0);
            let items = vec![Object::Integer(1), Object::Integer(2), Object::Integer(3)];
            assert_eq!(result, Ok((Object::Array(items), 7)));
        }

        #[test]
        fn when_parse_with_empty_array_then_returns_empty_array() {
            let result = parse_object(b"[]", 0);
            assert_eq!(result, Ok((Object::Array(Vec::new()), 2)));
        }

        #[test]
        fn when_parse_with_nested_array_then_returns_nested_array() {
            let result = parse_object(b"[[1] /Name]", 0);
            let items = vec![
                Object::Array(vec![Object::Integer(1)]),
                Object::Name("Name".to_string()),
            ];
            assert_eq!(result, Ok((Object::Array(items), 11)));
        }

        #[test]
        fn when_parse_with_unterminated_array_then_returns_error() {
            let result = parse_object(b"[1 2", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_reference_then_returns_reference() {
            let result = parse_object(b"1 0 R", 0);
            let reference = Object::Reference {
                object_number: 1,
                generation: 0,
            };
            assert_eq!(result, Ok((reference, 5)));
        }

        #[test]
        fn when_parse_with_references_in_array_then_returns_references() {
            let result = parse_object(b"[1 0 R 2 5 R]", 0);
            let items = vec![
                Object::Reference {
                    object_number: 1,
                    generation: 0,
                },
                Object::Reference {
                    object_number: 2,
                    generation: 5,
                },
            ];
            assert_eq!(result, Ok((Object::Array(items), 13)));
        }

        #[test]
        fn when_parse_with_integer_pair_without_keyword_then_returns_integer() {
            let result = parse_object(b"1 0 X", 0);
            assert_eq!(result, Ok((Object::Integer(1), 1)));
        }

        #[test]
        fn when_parse_with_keyword_prefix_after_integers_then_returns_integer() {
            let result = parse_object(b"1 0 RG", 0);
            assert_eq!(result, Ok((Object::Integer(1), 1)));
        }

        #[test]
        fn when_parse_with_dictionary_then_returns_dictionary() {
            let result = parse_object(b"<< /Type /Catalog /Pages 2 0 R >>", 0);
            let entries = vec![
                ("Type".to_string(), Object::Name("Catalog".to_string())),
                (
                    "Pages".to_string(),
                    Object::Reference {
                        object_number: 2,
                        generation: 0,
                    },
                ),
            ];
            assert_eq!(result, Ok((Object::Dictionary(entries), 33)));
        }

        #[test]
        fn when_parse_with_empty_dictionary_then_returns_empty_dictionary() {
            let result = parse_object(b"<< >>", 0);
            assert_eq!(result, Ok((Object::Dictionary(Vec::new()), 5)));
        }

        #[test]
        fn when_parse_with_nested_dictionary_then_returns_nested_dictionary() {
            let result = parse_object(b"<< /Font << /F1 1 >> >>", 0);
            let inner = vec![("F1".to_string(), Object::Integer(1))];
            let entries = vec![("Font".to_string(), Object::Dictionary(inner))];
            assert_eq!(result, Ok((Object::Dictionary(entries), 23)));
        }

        #[test]
        fn when_parse_with_non_name_key_then_returns_error() {
            let result = parse_object(b"<< 1 2 >>", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_unterminated_dictionary_then_returns_error() {
            let result = parse_object(b"<< /Type /Catalog", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_comment_then_skips_comment() {
            let result = parse_object(b"% a comment\n42", 0);
            assert_eq!(result, Ok((Object::Integer(42), 14)));
        }

        #[test]
        fn when_parse_with_offset_then_reads_at_position() {
            let result = parse_object(b"garbage /Name", 8);
            assert_eq!(result, Ok((Object::Name("Name".to_string()), 13)));
        }

        #[test]
        fn when_parse_with_offset_beyond_input_then_returns_error() {
            let result = parse_object(b"42", 100);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_empty_input_then_returns_error() {
            let result = parse_object(b"", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_unexpected_byte_then_returns_error() {
            let result = parse_object(b"]", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_stream_then_returns_stream() {
            let result = parse_object(b"<< /Length 5 >>\nstream\nHello\nendstream", 0);
            let stream = Object::Stream {
                dictionary: vec![("Length".to_string(), Object::Integer(5))],
                data: b"Hello".to_vec(),
            };
            assert_eq!(result, Ok((stream, 38)));
        }

        #[test]
        fn when_parse_with_stream_using_crlf_then_returns_stream() {
            let result = parse_object(b"<< /Length 5 >>\r\nstream\r\nHello\r\nendstream", 0);
            let stream = Object::Stream {
                dictionary: vec![("Length".to_string(), Object::Integer(5))],
                data: b"Hello".to_vec(),
            };
            assert_eq!(result, Ok((stream, 41)));
        }

        #[test]
        fn when_parse_with_wrong_length_then_reads_until_endstream() {
            let result = parse_object(b"<< /Length 99 >>\nstream\nHello\nendstream", 0);
            let stream = Object::Stream {
                dictionary: vec![("Length".to_string(), Object::Integer(99))],
                data: b"Hello".to_vec(),
            };
            assert_eq!(result, Ok((stream, 39)));
        }

        #[test]
        fn when_parse_with_indirect_length_then_reads_until_endstream() {
            let result = parse_object(b"<< /Length 1 0 R >>\nstream\nHello\nendstream", 0);
            let stream = Object::Stream {
                dictionary: vec![(
                    "Length".to_string(),
                    Object::Reference {
                        object_number: 1,
                        generation: 0,
                    },
                )],
                data: b"Hello".to_vec(),
            };
            assert_eq!(result, Ok((stream, 42)));
        }

        #[test]
        fn when_parse_with_binary_stream_then_returns_raw_bytes() {
            let mut data = b"<< /Length 3 >>\nstream\n".to_vec();
            data.extend_from_slice(&[0x00, 0xff, 0x25]);
            data.extend_from_slice(b"\nendstream");
            let stream = Object::Stream {
                dictionary: vec![("Length".to_string(), Object::Integer(3))],
                data: vec![0x00, 0xff, 0x25],
            };
            assert_eq!(parse_object(&data, 0), Ok((stream, data.len())));
        }

        #[test]
        fn when_parse_with_missing_endstream_then_returns_error() {
            let result = parse_object(b"<< /Length 5 >>\nstream\nHello", 0);
            assert_eq!(result, Err(PdfError::UnexpectedEof));
        }

        #[test]
        fn when_parse_with_stream_keyword_without_eol_then_returns_error() {
            let result = parse_object(b"<< /Length 0 >>\nstreamendstream", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }
    }

    mod parse_indirect_object {
        use super::*;

        #[test]
        fn when_parse_with_indirect_object_then_returns_object() {
            let data = b"1 0 obj\n<< /Type /Catalog >>\nendobj\n";
            let (indirect, after) = parse_indirect_object(data, 0).unwrap();
            assert_eq!(indirect.object_number, 1);
            assert_eq!(indirect.generation, 0);
            assert_eq!(
                indirect.object,
                Object::Dictionary(vec![(
                    "Type".to_string(),
                    Object::Name("Catalog".to_string())
                )])
            );
            assert_eq!(after, 35);
        }

        #[test]
        fn when_parse_with_generation_then_returns_generation() {
            let data = b"7 3 obj\n42\nendobj\n";
            let (indirect, _) = parse_indirect_object(data, 0).unwrap();
            assert_eq!((indirect.object_number, indirect.generation), (7, 3));
        }

        #[test]
        fn when_parse_with_stream_object_then_returns_stream() {
            let data = b"1 0 obj\n<< /Length 5 >>\nstream\nHello\nendstream\nendobj\n";
            let (indirect, _) = parse_indirect_object(data, 0).unwrap();
            assert_eq!(
                indirect.object,
                Object::Stream {
                    dictionary: vec![("Length".to_string(), Object::Integer(5))],
                    data: b"Hello".to_vec(),
                }
            );
        }

        #[test]
        fn when_parse_with_offset_then_reads_at_position() {
            let data = minimal_pdf();
            let (indirect, _) = parse_indirect_object(&data, 9).unwrap();
            assert_eq!(indirect.object_number, 1);
        }

        #[test]
        fn when_parse_with_missing_obj_keyword_then_returns_error() {
            let result = parse_indirect_object(b"1 0\n<< >>\nendobj\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_missing_endobj_then_returns_error() {
            let result = parse_indirect_object(b"1 0 obj\n<< >>\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_missing_generation_then_returns_error() {
            let result = parse_indirect_object(b"1 obj\n<< >>\nendobj\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_non_numeric_object_number_then_returns_error() {
            let result = parse_indirect_object(b"one 0 obj\n<< >>\nendobj\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_object_number_beyond_range_then_returns_error() {
            let result = parse_indirect_object(b"99999999999 0 obj\n1\nendobj\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }

        #[test]
        fn when_parse_with_generation_beyond_range_then_returns_error() {
            let result = parse_indirect_object(b"1 99999 obj\n1\nendobj\n", 0);
            assert_eq!(result, Err(PdfError::MalformedObject));
        }
    }

    mod get_object {
        use super::*;

        #[test]
        fn when_get_with_existing_object_then_returns_object() {
            let data = minimal_pdf();
            let table = read_xref_table(&data).unwrap();
            assert_eq!(
                get_object(&data, &table, 1),
                Ok(Object::Dictionary(vec![(
                    "Type".to_string(),
                    Object::Name("Catalog".to_string())
                )]))
            );
        }

        #[test]
        fn when_get_with_free_object_then_returns_error() {
            let data = minimal_pdf();
            let table = read_xref_table(&data).unwrap();
            assert_eq!(get_object(&data, &table, 0), Err(PdfError::ObjectNotFound));
        }

        #[test]
        fn when_get_with_unknown_object_then_returns_error() {
            let data = minimal_pdf();
            let table = read_xref_table(&data).unwrap();
            assert_eq!(get_object(&data, &table, 7), Err(PdfError::ObjectNotFound));
        }

        #[test]
        fn when_get_with_offset_pointing_at_another_object_then_returns_error() {
            let data = minimal_pdf();
            let table = XrefTable {
                entries: vec![XrefEntry {
                    object_number: 2,
                    generation: 0,
                    kind: XrefEntryKind::InUse { offset: 9 },
                }],
                trailer_offset: 0,
            };
            assert_eq!(
                get_object(&data, &table, 2),
                Err(PdfError::ObjectNumberMismatch)
            );
        }
    }

    mod resolve {
        use super::*;

        #[test]
        fn when_resolve_with_direct_object_then_returns_same_object() {
            let data = minimal_pdf();
            let table = read_xref_table(&data).unwrap();
            let result = resolve(&data, &table, &Object::Integer(42));
            assert_eq!(result, Ok(Object::Integer(42)));
        }

        #[test]
        fn when_resolve_with_reference_then_returns_referenced_object() {
            let data = pdf_with_objects(&["42"]);
            let table = read_xref_table(&data).unwrap();
            let reference = Object::Reference {
                object_number: 1,
                generation: 0,
            };
            assert_eq!(resolve(&data, &table, &reference), Ok(Object::Integer(42)));
        }

        #[test]
        fn when_resolve_with_chained_references_then_returns_final_object() {
            let data = pdf_with_objects(&["2 0 R", "3 0 R", "42"]);
            let table = read_xref_table(&data).unwrap();
            let reference = Object::Reference {
                object_number: 1,
                generation: 0,
            };
            assert_eq!(resolve(&data, &table, &reference), Ok(Object::Integer(42)));
        }

        #[test]
        fn when_resolve_with_circular_references_then_returns_error() {
            let data = pdf_with_objects(&["2 0 R", "1 0 R"]);
            let table = read_xref_table(&data).unwrap();
            let reference = Object::Reference {
                object_number: 1,
                generation: 0,
            };
            assert_eq!(
                resolve(&data, &table, &reference),
                Err(PdfError::CircularReference)
            );
        }

        #[test]
        fn when_resolve_with_dangling_reference_then_returns_error() {
            let data = pdf_with_objects(&["9 0 R"]);
            let table = read_xref_table(&data).unwrap();
            let reference = Object::Reference {
                object_number: 1,
                generation: 0,
            };
            assert_eq!(
                resolve(&data, &table, &reference),
                Err(PdfError::ObjectNotFound)
            );
        }
    }

    mod get {
        use super::*;

        fn catalog() -> Object {
            Object::Dictionary(vec![
                ("Type".to_string(), Object::Name("Catalog".to_string())),
                ("Version".to_string(), Object::Integer(2)),
            ])
        }

        #[test]
        fn when_get_with_existing_key_then_returns_value() {
            assert_eq!(
                catalog().get("Type"),
                Some(&Object::Name("Catalog".to_string()))
            );
        }

        #[test]
        fn when_get_with_missing_key_then_returns_none() {
            assert_eq!(catalog().get("Pages"), None);
        }

        #[test]
        fn when_get_with_stream_dictionary_then_returns_value() {
            let stream = Object::Stream {
                dictionary: vec![("Length".to_string(), Object::Integer(5))],
                data: b"Hello".to_vec(),
            };
            assert_eq!(stream.get("Length"), Some(&Object::Integer(5)));
        }

        #[test]
        fn when_get_with_non_dictionary_then_returns_none() {
            assert_eq!(Object::Integer(1).get("Type"), None);
        }
    }

    mod as_i64 {
        use super::*;

        #[test]
        fn when_as_i64_with_integer_then_returns_value() {
            assert_eq!(Object::Integer(-3).as_i64(), Some(-3));
        }

        #[test]
        fn when_as_i64_with_real_then_returns_none() {
            assert_eq!(Object::Real(3.5).as_i64(), None);
        }
    }

    mod as_f64 {
        use super::*;

        #[test]
        fn when_as_f64_with_real_then_returns_value() {
            assert_eq!(Object::Real(3.5).as_f64(), Some(3.5));
        }

        #[test]
        fn when_as_f64_with_integer_then_returns_value() {
            assert_eq!(Object::Integer(3).as_f64(), Some(3.0));
        }

        #[test]
        fn when_as_f64_with_name_then_returns_none() {
            assert_eq!(Object::Name("Type".to_string()).as_f64(), None);
        }
    }

    mod as_name {
        use super::*;

        #[test]
        fn when_as_name_with_name_then_returns_text() {
            assert_eq!(Object::Name("Type".to_string()).as_name(), Some("Type"));
        }

        #[test]
        fn when_as_name_with_string_then_returns_none() {
            assert_eq!(Object::String(b"Type".to_vec()).as_name(), None);
        }
    }

    mod as_array {
        use super::*;

        #[test]
        fn when_as_array_with_array_then_returns_items() {
            let array = Object::Array(vec![Object::Integer(1)]);
            assert_eq!(array.as_array(), Some(&[Object::Integer(1)][..]));
        }

        #[test]
        fn when_as_array_with_null_then_returns_none() {
            assert_eq!(Object::Null.as_array(), None);
        }
    }
}
