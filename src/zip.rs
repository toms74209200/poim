const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;

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
pub enum ZipError {
    UnexpectedEof,
    InvalidSignature,
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
}
