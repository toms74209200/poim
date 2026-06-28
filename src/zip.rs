const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
const CENTRAL_DIR_SIGNATURE: u32 = 0x02014b50;
const EOCD_SIGNATURE: u32 = 0x06054b50;
const EOCD_MIN_SIZE: usize = 22;

#[derive(Debug, Clone, PartialEq)]
pub struct LocalFileHeader {
    pub compression_method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub file_name: Vec<u8>,
    pub data_offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CentralDirectoryEntry {
    pub compression_method: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub file_name: Vec<u8>,
    pub local_header_offset: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EndOfCentralDirectory {
    pub total_entries: u16,
    pub central_dir_size: u32,
    pub central_dir_offset: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ZipError {
    UnexpectedEof,
    InvalidSignature,
    EocdNotFound,
    UnsupportedCompression(u16),
    InflateError(crate::inflate::InflateError),
    EntryNotFound,
}

fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, ZipError> {
    if offset + 2 > data.len() {
        return Err(ZipError::UnexpectedEof);
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, ZipError> {
    if offset + 4 > data.len() {
        return Err(ZipError::UnexpectedEof);
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

pub fn read_local_file_header(data: &[u8], offset: usize) -> Result<LocalFileHeader, ZipError> {
    let signature = read_u32_le(data, offset)?;
    if signature != LOCAL_FILE_HEADER_SIGNATURE {
        return Err(ZipError::InvalidSignature);
    }

    let compression_method = read_u16_le(data, offset + 8)?;
    let crc32 = read_u32_le(data, offset + 14)?;
    let compressed_size = read_u32_le(data, offset + 18)?;
    let uncompressed_size = read_u32_le(data, offset + 22)?;
    let file_name_length = read_u16_le(data, offset + 26)? as usize;
    let extra_field_length = read_u16_le(data, offset + 28)? as usize;

    let name_start = offset + 30;
    let name_end = name_start + file_name_length;
    if name_end > data.len() {
        return Err(ZipError::UnexpectedEof);
    }
    let file_name = data[name_start..name_end].to_vec();

    let data_offset = name_end + extra_field_length;
    if data_offset > data.len() {
        return Err(ZipError::UnexpectedEof);
    }

    Ok(LocalFileHeader {
        compression_method,
        crc32,
        compressed_size,
        uncompressed_size,
        file_name,
        data_offset,
    })
}

pub fn find_eocd(data: &[u8]) -> Result<EndOfCentralDirectory, ZipError> {
    if data.len() < EOCD_MIN_SIZE {
        return Err(ZipError::EocdNotFound);
    }
    let search_start = data.len().saturating_sub(EOCD_MIN_SIZE + 65535);
    for i in (search_start..=data.len() - EOCD_MIN_SIZE).rev() {
        if read_u32_le(data, i)? == EOCD_SIGNATURE {
            let total_entries = read_u16_le(data, i + 8)?;
            let central_dir_size = read_u32_le(data, i + 12)?;
            let central_dir_offset = read_u32_le(data, i + 16)?;
            return Ok(EndOfCentralDirectory {
                total_entries,
                central_dir_size,
                central_dir_offset,
            });
        }
    }
    Err(ZipError::EocdNotFound)
}

pub fn read_central_directory_entry(
    data: &[u8],
    offset: usize,
) -> Result<(CentralDirectoryEntry, usize), ZipError> {
    let signature = read_u32_le(data, offset)?;
    if signature != CENTRAL_DIR_SIGNATURE {
        return Err(ZipError::InvalidSignature);
    }

    let compression_method = read_u16_le(data, offset + 10)?;
    let crc32 = read_u32_le(data, offset + 16)?;
    let compressed_size = read_u32_le(data, offset + 20)?;
    let uncompressed_size = read_u32_le(data, offset + 24)?;
    let file_name_length = read_u16_le(data, offset + 28)? as usize;
    let extra_field_length = read_u16_le(data, offset + 30)? as usize;
    let comment_length = read_u16_le(data, offset + 32)? as usize;
    let local_header_offset = read_u32_le(data, offset + 42)?;

    let name_start = offset + 46;
    let name_end = name_start + file_name_length;
    if name_end > data.len() {
        return Err(ZipError::UnexpectedEof);
    }
    let file_name = data[name_start..name_end].to_vec();

    let next_offset = name_end + extra_field_length + comment_length;

    Ok((
        CentralDirectoryEntry {
            compression_method,
            crc32,
            compressed_size,
            uncompressed_size,
            file_name,
            local_header_offset,
        },
        next_offset,
    ))
}

pub fn read_central_directory(data: &[u8]) -> Result<Vec<CentralDirectoryEntry>, ZipError> {
    let eocd = find_eocd(data)?;
    let mut entries = Vec::new();
    let mut offset = eocd.central_dir_offset as usize;

    for _ in 0..eocd.total_entries {
        let (entry, next_offset) = read_central_directory_entry(data, offset)?;
        entries.push(entry);
        offset = next_offset;
    }

    Ok(entries)
}

pub fn extract_entry(data: &[u8], entry: &CentralDirectoryEntry) -> Result<Vec<u8>, ZipError> {
    let local = read_local_file_header(data, entry.local_header_offset as usize)?;
    let compressed_size = entry.compressed_size as usize;
    let end = local.data_offset + compressed_size;
    if end > data.len() {
        return Err(ZipError::UnexpectedEof);
    }
    let raw = &data[local.data_offset..end];

    match entry.compression_method {
        0 => Ok(raw.to_vec()),
        8 => crate::inflate::inflate(raw).map_err(ZipError::InflateError),
        other => Err(ZipError::UnsupportedCompression(other)),
    }
}

pub fn extract_by_name(data: &[u8], name: &[u8]) -> Result<Vec<u8>, ZipError> {
    let entries = read_central_directory(data)?;
    let entry = entries
        .iter()
        .find(|e| e.file_name == name)
        .ok_or(ZipError::EntryNotFound)?;
    extract_entry(data, entry)
}

pub fn read_all_local_file_headers(data: &[u8]) -> Result<Vec<LocalFileHeader>, ZipError> {
    let mut headers = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        if offset + 4 > data.len() {
            break;
        }
        let signature = read_u32_le(data, offset)?;
        if signature != LOCAL_FILE_HEADER_SIGNATURE {
            break;
        }
        let header = read_local_file_header(data, offset)?;
        let next_offset = header.data_offset + header.compressed_size as usize;
        headers.push(header);
        offset = next_offset;
    }

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_local_file_header(
        file_name: &[u8],
        compression_method: u16,
        data: &[u8],
        extra: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&LOCAL_FILE_HEADER_SIGNATURE.to_le_bytes());
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&compression_method.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&0u32.to_le_bytes()); // crc32
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(file_name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        buf.extend_from_slice(file_name);
        buf.extend_from_slice(extra);
        buf.extend_from_slice(data);
        buf
    }

    fn build_central_dir_entry(
        file_name: &[u8],
        compression_method: u16,
        compressed_size: u32,
        uncompressed_size: u32,
        local_header_offset: u32,
        extra: &[u8],
        comment: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&CENTRAL_DIR_SIGNATURE.to_le_bytes());
        buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&compression_method.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&0u32.to_le_bytes()); // crc32
        buf.extend_from_slice(&compressed_size.to_le_bytes());
        buf.extend_from_slice(&uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&(file_name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(extra.len() as u16).to_le_bytes());
        buf.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk number start
        buf.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        buf.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        buf.extend_from_slice(&local_header_offset.to_le_bytes());
        buf.extend_from_slice(file_name);
        buf.extend_from_slice(extra);
        buf.extend_from_slice(comment);
        buf
    }

    fn build_eocd(total_entries: u16, cd_size: u32, cd_offset: u32, comment: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&EOCD_SIGNATURE.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
        buf.extend_from_slice(&0u16.to_le_bytes()); // disk with CD
        buf.extend_from_slice(&total_entries.to_le_bytes()); // entries on this disk
        buf.extend_from_slice(&total_entries.to_le_bytes()); // total entries
        buf.extend_from_slice(&cd_size.to_le_bytes());
        buf.extend_from_slice(&cd_offset.to_le_bytes());
        buf.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        buf.extend_from_slice(comment);
        buf
    }

    struct ZipBuilder {
        local_headers: Vec<u8>,
        entries: Vec<(Vec<u8>, u16, u32, u32, u32)>,
    }

    impl ZipBuilder {
        fn new() -> Self {
            Self {
                local_headers: Vec::new(),
                entries: Vec::new(),
            }
        }

        fn add_stored(mut self, name: &[u8], data: &[u8]) -> Self {
            let offset = self.local_headers.len() as u32;
            self.local_headers
                .extend_from_slice(&build_local_file_header(name, 0, data, &[]));
            self.entries.push((
                name.to_vec(),
                0,
                data.len() as u32,
                data.len() as u32,
                offset,
            ));
            self
        }

        fn add_deflated(mut self, name: &[u8], compressed: &[u8], original_size: u32) -> Self {
            let offset = self.local_headers.len() as u32;
            self.local_headers
                .extend_from_slice(&build_local_file_header(name, 8, compressed, &[]));
            self.entries.push((
                name.to_vec(),
                8,
                compressed.len() as u32,
                original_size,
                offset,
            ));
            self
        }

        fn build(self) -> Vec<u8> {
            let cd_offset = self.local_headers.len() as u32;
            let mut cd = Vec::new();
            for (name, method, comp_size, uncomp_size, offset) in &self.entries {
                cd.extend_from_slice(&build_central_dir_entry(
                    name,
                    *method,
                    *comp_size,
                    *uncomp_size,
                    *offset,
                    &[],
                    &[],
                ));
            }
            let cd_size = cd.len() as u32;
            let eocd = build_eocd(self.entries.len() as u16, cd_size, cd_offset, &[]);

            let mut result = self.local_headers;
            result.extend_from_slice(&cd);
            result.extend_from_slice(&eocd);
            result
        }
    }

    mod read_local_file_header {
        use super::*;

        #[test]
        fn when_read_with_stored_entry_then_returns_header() {
            let content = b"Hello";
            let input = build_local_file_header(b"hello.txt", 0, content, &[]);

            let header = read_local_file_header(&input, 0).unwrap();

            assert_eq!(header.file_name, b"hello.txt");
            assert_eq!(header.compression_method, 0);
            assert_eq!(header.compressed_size, 5);
            assert_eq!(header.uncompressed_size, 5);
            assert_eq!(header.data_offset, 30 + 9);
            assert_eq!(&input[header.data_offset..header.data_offset + 5], content);
        }

        #[test]
        fn when_read_with_deflated_entry_then_returns_method_8() {
            let compressed = &[0x03, 0x00]; // empty deflate stream
            let input = build_local_file_header(b"empty.bin", 8, compressed, &[]);

            let header = read_local_file_header(&input, 0).unwrap();

            assert_eq!(header.compression_method, 8);
            assert_eq!(header.compressed_size, 2);
            assert_eq!(header.file_name, b"empty.bin");
        }

        #[test]
        fn when_read_with_extra_field_then_skips_extra() {
            let extra = [0xFF; 12];
            let content = b"data";
            let input = build_local_file_header(b"f.dat", 0, content, &extra);

            let header = read_local_file_header(&input, 0).unwrap();

            assert_eq!(header.data_offset, 30 + 5 + 12);
            assert_eq!(&input[header.data_offset..header.data_offset + 4], content);
        }

        #[test]
        fn when_read_with_nonzero_offset_then_reads_at_position() {
            let padding = [0u8; 10];
            let entry = build_local_file_header(b"a.txt", 0, b"AB", &[]);
            let mut input = padding.to_vec();
            input.extend_from_slice(&entry);

            let header = read_local_file_header(&input, 10).unwrap();

            assert_eq!(header.file_name, b"a.txt");
            assert_eq!(header.compressed_size, 2);
        }

        #[test]
        fn when_read_with_invalid_signature_then_returns_error() {
            let input = [0x00; 30];
            assert_eq!(
                read_local_file_header(&input, 0),
                Err(ZipError::InvalidSignature)
            );
        }

        #[test]
        fn when_read_with_truncated_header_then_returns_eof_error() {
            let input = [0x50, 0x4B, 0x03, 0x04]; // signature only
            assert_eq!(
                read_local_file_header(&input, 0),
                Err(ZipError::UnexpectedEof)
            );
        }

        #[test]
        fn when_read_with_empty_file_name_then_returns_empty_name() {
            let input = build_local_file_header(b"", 0, b"x", &[]);

            let header = read_local_file_header(&input, 0).unwrap();

            assert_eq!(header.file_name, b"");
            assert_eq!(header.data_offset, 30);
        }

        #[test]
        fn when_read_with_directory_entry_then_returns_zero_sizes() {
            let input = build_local_file_header(b"dir/", 0, &[], &[]);

            let header = read_local_file_header(&input, 0).unwrap();

            assert_eq!(header.file_name, b"dir/");
            assert_eq!(header.compressed_size, 0);
            assert_eq!(header.uncompressed_size, 0);
        }
    }

    mod read_all_local_file_headers {
        use super::*;

        #[test]
        fn when_read_all_with_multiple_entries_then_returns_all() {
            let mut input = build_local_file_header(b"a.txt", 0, b"AAA", &[]);
            input.extend_from_slice(&build_local_file_header(b"b.txt", 8, b"BB", &[]));

            let headers = read_all_local_file_headers(&input).unwrap();

            assert_eq!(headers.len(), 2);
            assert_eq!(headers[0].file_name, b"a.txt");
            assert_eq!(headers[0].compressed_size, 3);
            assert_eq!(headers[1].file_name, b"b.txt");
            assert_eq!(headers[1].compressed_size, 2);
        }

        #[test]
        fn when_read_all_with_single_entry_then_returns_one() {
            let input = build_local_file_header(b"only.txt", 0, b"data", &[]);

            let headers = read_all_local_file_headers(&input).unwrap();

            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].file_name, b"only.txt");
        }

        #[test]
        fn when_read_all_with_trailing_central_directory_then_stops() {
            let mut input = build_local_file_header(b"x.txt", 0, b"X", &[]);
            // central directory signature: 0x02014b50
            input.extend_from_slice(&0x02014b50u32.to_le_bytes());
            input.extend_from_slice(&[0u8; 42]);

            let headers = read_all_local_file_headers(&input).unwrap();

            assert_eq!(headers.len(), 1);
        }

        #[test]
        fn when_read_all_with_empty_input_then_returns_empty() {
            let headers = read_all_local_file_headers(&[]).unwrap();
            assert_eq!(headers.len(), 0);
        }
    }

    mod find_eocd {
        use super::*;

        #[test]
        fn when_find_with_minimal_zip_then_returns_eocd() {
            let zip = ZipBuilder::new().add_stored(b"a.txt", b"hi").build();

            let eocd = find_eocd(&zip).unwrap();

            assert_eq!(eocd.total_entries, 1);
            assert_eq!(eocd.central_dir_offset, 30 + 5 + 2);
        }

        #[test]
        fn when_find_with_comment_then_returns_eocd() {
            let local = build_local_file_header(b"x.txt", 0, b"X", &[]);
            let cd_offset = local.len() as u32;
            let cd = build_central_dir_entry(b"x.txt", 0, 1, 1, 0, &[], &[]);
            let cd_size = cd.len() as u32;
            let eocd = build_eocd(1, cd_size, cd_offset, b"ZIP comment here");

            let mut zip = local;
            zip.extend_from_slice(&cd);
            zip.extend_from_slice(&eocd);

            let result = find_eocd(&zip).unwrap();

            assert_eq!(result.total_entries, 1);
            assert_eq!(result.central_dir_offset, cd_offset);
        }

        #[test]
        fn when_find_with_too_small_input_then_returns_error() {
            assert_eq!(find_eocd(&[0u8; 10]), Err(ZipError::EocdNotFound));
        }

        #[test]
        fn when_find_with_no_signature_then_returns_error() {
            assert_eq!(find_eocd(&[0u8; 22]), Err(ZipError::EocdNotFound));
        }
    }

    mod read_central_directory_entry {
        use super::*;

        #[test]
        fn when_read_entry_with_stored_file_then_returns_metadata() {
            let cd = build_central_dir_entry(b"hello.txt", 0, 5, 5, 0, &[], &[]);

            let (entry, next) = read_central_directory_entry(&cd, 0).unwrap();

            assert_eq!(entry.file_name, b"hello.txt");
            assert_eq!(entry.compression_method, 0);
            assert_eq!(entry.compressed_size, 5);
            assert_eq!(entry.uncompressed_size, 5);
            assert_eq!(entry.local_header_offset, 0);
            assert_eq!(next, cd.len());
        }

        #[test]
        fn when_read_entry_with_deflated_file_then_returns_method_8() {
            let cd = build_central_dir_entry(b"data.bin", 8, 100, 200, 42, &[], &[]);

            let (entry, _) = read_central_directory_entry(&cd, 0).unwrap();

            assert_eq!(entry.compression_method, 8);
            assert_eq!(entry.compressed_size, 100);
            assert_eq!(entry.uncompressed_size, 200);
            assert_eq!(entry.local_header_offset, 42);
        }

        #[test]
        fn when_read_entry_with_extra_and_comment_then_skips_both() {
            let extra = [0xAA; 8];
            let comment = b"a comment";
            let cd = build_central_dir_entry(b"f.txt", 0, 3, 3, 0, &extra, comment);

            let (entry, next) = read_central_directory_entry(&cd, 0).unwrap();

            assert_eq!(entry.file_name, b"f.txt");
            assert_eq!(next, 46 + 5 + 8 + 9);
        }

        #[test]
        fn when_read_entry_with_invalid_signature_then_returns_error() {
            let data = [0u8; 46];
            assert_eq!(
                read_central_directory_entry(&data, 0),
                Err(ZipError::InvalidSignature)
            );
        }

        #[test]
        fn when_read_entry_with_truncated_data_then_returns_eof_error() {
            let data = CENTRAL_DIR_SIGNATURE.to_le_bytes();
            assert_eq!(
                read_central_directory_entry(&data, 0),
                Err(ZipError::UnexpectedEof)
            );
        }
    }

    mod read_central_directory {
        use super::*;

        #[test]
        fn when_read_cd_with_multiple_entries_then_returns_all() {
            let zip = ZipBuilder::new()
                .add_stored(b"a.txt", b"AAA")
                .add_stored(b"b.txt", b"BB")
                .build();

            let entries = read_central_directory(&zip).unwrap();

            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].file_name, b"a.txt");
            assert_eq!(entries[0].compressed_size, 3);
            assert_eq!(entries[1].file_name, b"b.txt");
            assert_eq!(entries[1].compressed_size, 2);
        }

        #[test]
        fn when_read_cd_with_deflated_entry_then_returns_correct_sizes() {
            let compressed = &[0x03, 0x00];
            let zip = ZipBuilder::new()
                .add_deflated(b"empty.deflate", compressed, 0)
                .build();

            let entries = read_central_directory(&zip).unwrap();

            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].compression_method, 8);
            assert_eq!(entries[0].compressed_size, 2);
            assert_eq!(entries[0].uncompressed_size, 0);
        }

        #[test]
        fn when_read_cd_with_mixed_methods_then_preserves_each() {
            let compressed = &[0xF3, 0x48, 0xCD, 0xC9, 0xC9, 0x07, 0x00];
            let zip = ZipBuilder::new()
                .add_stored(b"raw.txt", b"Hello")
                .add_deflated(b"comp.txt", compressed, 5)
                .build();

            let entries = read_central_directory(&zip).unwrap();

            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0].compression_method, 0);
            assert_eq!(entries[1].compression_method, 8);
        }

        #[test]
        fn when_read_cd_with_local_header_offsets_then_matches_positions() {
            let zip = ZipBuilder::new()
                .add_stored(b"first.txt", b"111")
                .add_stored(b"second.txt", b"22")
                .build();

            let entries = read_central_directory(&zip).unwrap();

            assert_eq!(entries[0].local_header_offset, 0);
            let expected_second_offset = 30 + 9 + 3; // header + name + data
            assert_eq!(
                entries[1].local_header_offset,
                expected_second_offset as u32
            );
        }

        #[test]
        fn when_read_cd_with_no_eocd_then_returns_error() {
            let data = build_local_file_header(b"a.txt", 0, b"x", &[]);
            assert_eq!(read_central_directory(&data), Err(ZipError::EocdNotFound));
        }
    }

    mod extract_entry {
        use super::*;

        #[test]
        fn when_extract_with_stored_entry_then_returns_raw_data() {
            let content = b"stored content here";
            let zip = ZipBuilder::new().add_stored(b"file.txt", content).build();
            let entries = read_central_directory(&zip).unwrap();

            let result = extract_entry(&zip, &entries[0]).unwrap();

            assert_eq!(result, content);
        }

        #[test]
        fn when_extract_with_deflated_entry_then_returns_decompressed() {
            let original = b"Hello, ZIP extraction!";
            let compressed: &[u8] = &[
                0xf3, 0x48, 0xcd, 0xc9, 0xc9, 0xd7, 0x51, 0x88, 0xf2, 0x0c, 0x50, 0x48, 0xad, 0x28,
                0x29, 0x4a, 0x4c, 0x2e, 0xc9, 0xcc, 0xcf, 0x53, 0x04, 0x00,
            ];
            let zip = ZipBuilder::new()
                .add_deflated(b"msg.txt", compressed, original.len() as u32)
                .build();
            let entries = read_central_directory(&zip).unwrap();

            let result = extract_entry(&zip, &entries[0]).unwrap();

            assert_eq!(result, original);
        }

        #[test]
        fn when_extract_with_second_entry_then_returns_correct_data() {
            let first = b"FIRST";
            let second = b"SECOND";
            let zip = ZipBuilder::new()
                .add_stored(b"a.txt", first)
                .add_stored(b"b.txt", second)
                .build();
            let entries = read_central_directory(&zip).unwrap();

            let result = extract_entry(&zip, &entries[1]).unwrap();

            assert_eq!(result, second);
        }

        #[test]
        fn when_extract_with_empty_stored_then_returns_empty() {
            let zip = ZipBuilder::new().add_stored(b"empty", &[]).build();
            let entries = read_central_directory(&zip).unwrap();

            let result = extract_entry(&zip, &entries[0]).unwrap();

            assert!(result.is_empty());
        }

        #[test]
        fn when_extract_with_unsupported_method_then_returns_error() {
            let zip = ZipBuilder::new().add_stored(b"x", b"x").build();
            let mut entries = read_central_directory(&zip).unwrap();
            entries[0].compression_method = 99;

            assert_eq!(
                extract_entry(&zip, &entries[0]),
                Err(ZipError::UnsupportedCompression(99))
            );
        }
    }

    mod extract_by_name {
        use super::*;

        #[test]
        fn when_extract_by_name_with_existing_file_then_returns_content() {
            let zip = ZipBuilder::new()
                .add_stored(b"alpha.txt", b"AAA")
                .add_stored(b"beta.txt", b"BBB")
                .build();

            let result = extract_by_name(&zip, b"beta.txt").unwrap();

            assert_eq!(result, b"BBB");
        }

        #[test]
        fn when_extract_by_name_with_missing_file_then_returns_error() {
            let zip = ZipBuilder::new().add_stored(b"exists.txt", b"data").build();

            assert_eq!(
                extract_by_name(&zip, b"missing.txt"),
                Err(ZipError::EntryNotFound)
            );
        }

        #[test]
        fn when_extract_by_name_with_deflated_file_then_decompresses() {
            let original = b"Hello, ZIP extraction!";
            let compressed: &[u8] = &[
                0xf3, 0x48, 0xcd, 0xc9, 0xc9, 0xd7, 0x51, 0x88, 0xf2, 0x0c, 0x50, 0x48, 0xad, 0x28,
                0x29, 0x4a, 0x4c, 0x2e, 0xc9, 0xcc, 0xcf, 0x53, 0x04, 0x00,
            ];
            let zip = ZipBuilder::new()
                .add_stored(b"other.txt", b"xxx")
                .add_deflated(b"compressed.txt", compressed, original.len() as u32)
                .build();

            let result = extract_by_name(&zip, b"compressed.txt").unwrap();

            assert_eq!(result, original);
        }

        #[test]
        fn when_extract_by_name_with_path_then_matches_full_path() {
            let zip = ZipBuilder::new()
                .add_stored(b"dir/sub/file.xml", b"<xml/>")
                .build();

            let result = extract_by_name(&zip, b"dir/sub/file.xml").unwrap();

            assert_eq!(result, b"<xml/>");
        }
    }
}
