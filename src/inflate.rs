#[derive(Debug, PartialEq)]
pub enum InflateError {
    UnexpectedEof,
    InvalidBlockType,
    InvalidStoredLen,
    InvalidHuffmanCode,
    InvalidLengthCode,
    InvalidDistanceCode,
    DistanceTooFar,
    InvalidCodeLengths,
}

impl core::fmt::Display for InflateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::InvalidBlockType => write!(f, "invalid block type"),
            Self::InvalidStoredLen => write!(f, "invalid stored block length"),
            Self::InvalidHuffmanCode => write!(f, "invalid huffman code"),
            Self::InvalidLengthCode => write!(f, "invalid length code"),
            Self::InvalidDistanceCode => write!(f, "invalid distance code"),
            Self::DistanceTooFar => write!(f, "distance exceeds output"),
            Self::InvalidCodeLengths => write!(f, "invalid code length sequence"),
        }
    }
}

pub fn inflate(input: &[u8]) -> Result<Vec<u8>, InflateError> {
    let mut reader = BitReader::new(input);
    let mut output = Vec::new();
    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;
        match btype {
            0 => decode_stored(&mut reader, &mut output)?,
            1 => {
                let (lit, dist) = fixed_trees();
                decode_compressed(&mut reader, &lit, &dist, &mut output)?;
            }
            2 => {
                let (lit, dist) = decode_dynamic_trees(&mut reader)?;
                decode_compressed(&mut reader, &lit, &dist, &mut output)?;
            }
            _ => return Err(InflateError::InvalidBlockType),
        }
        if bfinal != 0 {
            break;
        }
    }
    Ok(output)
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
const CL_ORDER: [u8; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit: 0,
        }
    }

    fn read_bits(&mut self, n: u8) -> Result<u32, InflateError> {
        let mut val: u32 = 0;
        for i in 0..n {
            if self.pos >= self.data.len() {
                return Err(InflateError::UnexpectedEof);
            }
            val |= (((self.data[self.pos] >> self.bit) & 1) as u32) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Ok(val)
    }

    fn read_bit(&mut self) -> Result<u8, InflateError> {
        if self.pos >= self.data.len() {
            return Err(InflateError::UnexpectedEof);
        }
        let b = (self.data[self.pos] >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Ok(b)
    }

    fn align(&mut self) {
        if self.bit > 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
}

const UNINIT: i32 = i32::MAX;

struct HuffmanTree {
    nodes: Vec<[i32; 2]>,
}

impl HuffmanTree {
    fn from_lengths(lengths: &[u8]) -> Result<Self, InflateError> {
        let max_bits = *lengths.iter().max().unwrap_or(&0) as usize;
        if max_bits == 0 {
            return Ok(Self {
                nodes: vec![[UNINIT; 2]],
            });
        }

        let mut bl_count = vec![0u32; max_bits + 1];
        for &len in lengths {
            if len > 0 {
                bl_count[len as usize] += 1;
            }
        }

        let mut next_code = vec![0u32; max_bits + 1];
        let mut code: u32 = 0;
        for bits in 1..=max_bits {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        let mut nodes = vec![[UNINIT; 2]];

        for (sym, &len) in lengths.iter().enumerate() {
            if len == 0 {
                continue;
            }
            let c = next_code[len as usize];
            next_code[len as usize] += 1;

            let mut node = 0usize;
            for bit_pos in (0..len).rev() {
                let bit = ((c >> bit_pos) & 1) as usize;
                if bit_pos == 0 {
                    nodes[node][bit] = -(sym as i32) - 1;
                } else {
                    let child = nodes[node][bit];
                    if child == UNINIT {
                        nodes.push([UNINIT; 2]);
                        let idx = (nodes.len() - 1) as i32;
                        nodes[node][bit] = idx;
                        node = idx as usize;
                    } else if child < 0 {
                        return Err(InflateError::InvalidCodeLengths);
                    } else {
                        node = child as usize;
                    }
                }
            }
        }

        Ok(Self { nodes })
    }

    fn decode(&self, reader: &mut BitReader) -> Result<u16, InflateError> {
        let mut node = 0;
        let len = self.nodes.len() as i32;
        loop {
            let bit = reader.read_bit()? as usize;
            let val = self.nodes[node][bit];
            if val < 0 {
                return Ok((-val - 1) as u16);
            }
            if val >= len || val == UNINIT {
                return Err(InflateError::InvalidHuffmanCode);
            }
            node = val as usize;
        }
    }
}

fn fixed_trees() -> (HuffmanTree, HuffmanTree) {
    let mut lit_len = [0u8; 288];
    for l in &mut lit_len[0..=143] {
        *l = 8;
    }
    for l in &mut lit_len[144..=255] {
        *l = 9;
    }
    for l in &mut lit_len[256..=279] {
        *l = 7;
    }
    for l in &mut lit_len[280..=287] {
        *l = 8;
    }
    let dist_len = [5u8; 32];
    (
        HuffmanTree::from_lengths(&lit_len).unwrap(),
        HuffmanTree::from_lengths(&dist_len).unwrap(),
    )
}

fn decode_stored(reader: &mut BitReader, output: &mut Vec<u8>) -> Result<(), InflateError> {
    reader.align();
    if reader.pos + 4 > reader.data.len() {
        return Err(InflateError::UnexpectedEof);
    }
    let len = u16::from_le_bytes([reader.data[reader.pos], reader.data[reader.pos + 1]]);
    let nlen = u16::from_le_bytes([reader.data[reader.pos + 2], reader.data[reader.pos + 3]]);
    reader.pos += 4;
    if len != !nlen {
        return Err(InflateError::InvalidStoredLen);
    }
    let end = reader.pos + len as usize;
    if end > reader.data.len() {
        return Err(InflateError::UnexpectedEof);
    }
    output.extend_from_slice(&reader.data[reader.pos..end]);
    reader.pos = end;
    Ok(())
}

fn decode_dynamic_trees(
    reader: &mut BitReader,
) -> Result<(HuffmanTree, HuffmanTree), InflateError> {
    let hlit = reader.read_bits(5)? as usize + 257;
    let hdist = reader.read_bits(5)? as usize + 1;
    let hclen = reader.read_bits(4)? as usize + 4;

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CL_ORDER[i] as usize] = reader.read_bits(3)? as u8;
    }
    let cl_tree = HuffmanTree::from_lengths(&cl_lengths)?;

    let total = hlit + hdist;
    let mut lengths = Vec::with_capacity(total);

    while lengths.len() < total {
        let sym = cl_tree.decode(reader)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last().ok_or(InflateError::InvalidCodeLengths)?;
                let n = reader.read_bits(2)? as usize + 3;
                lengths.resize(lengths.len() + n, prev);
            }
            17 => {
                let n = reader.read_bits(3)? as usize + 3;
                lengths.resize(lengths.len() + n, 0);
            }
            18 => {
                let n = reader.read_bits(7)? as usize + 11;
                lengths.resize(lengths.len() + n, 0);
            }
            _ => return Err(InflateError::InvalidCodeLengths),
        }
    }

    let lit = HuffmanTree::from_lengths(&lengths[..hlit])?;
    let dist = HuffmanTree::from_lengths(&lengths[hlit..])?;
    Ok((lit, dist))
}

fn decode_compressed(
    reader: &mut BitReader,
    lit_tree: &HuffmanTree,
    dist_tree: &HuffmanTree,
    output: &mut Vec<u8>,
) -> Result<(), InflateError> {
    loop {
        let sym = lit_tree.decode(reader)?;
        if sym < 256 {
            output.push(sym as u8);
        } else if sym == 256 {
            return Ok(());
        } else {
            let li = (sym - 257) as usize;
            if li >= LENGTH_BASE.len() {
                return Err(InflateError::InvalidLengthCode);
            }
            let length = LENGTH_BASE[li] as usize + reader.read_bits(LENGTH_EXTRA[li])? as usize;

            let di = dist_tree.decode(reader)? as usize;
            if di >= DIST_BASE.len() {
                return Err(InflateError::InvalidDistanceCode);
            }
            let distance = DIST_BASE[di] as usize + reader.read_bits(DIST_EXTRA[di])? as usize;

            if distance > output.len() {
                return Err(InflateError::DistanceTooFar);
            }
            let start = output.len() - distance;
            for i in 0..length {
                output.push(output[start + i]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod bit_reader {
        use super::*;

        #[test]
        fn when_read_bit_with_byte_then_returns_bits_in_lsb_order() {
            let mut r = BitReader::new(&[0b10110100]);
            assert_eq!(r.read_bit().unwrap(), 0);
            assert_eq!(r.read_bit().unwrap(), 0);
            assert_eq!(r.read_bit().unwrap(), 1);
            assert_eq!(r.read_bit().unwrap(), 0);
            assert_eq!(r.read_bit().unwrap(), 1);
            assert_eq!(r.read_bit().unwrap(), 1);
            assert_eq!(r.read_bit().unwrap(), 0);
            assert_eq!(r.read_bit().unwrap(), 1);
        }

        #[test]
        fn when_read_bits_with_3_and_5_then_returns_split_values() {
            let mut r = BitReader::new(&[0b11010010]);
            assert_eq!(r.read_bits(3).unwrap(), 0b010);
            assert_eq!(r.read_bits(5).unwrap(), 0b11010);
        }

        #[test]
        fn when_read_bits_with_full_byte_then_reads_next_byte() {
            let mut r = BitReader::new(&[0xFF, 0x01]);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
            assert_eq!(r.read_bits(2).unwrap(), 0b01);
        }

        #[test]
        fn when_read_bits_with_cross_byte_span_then_returns_merged_value() {
            let mut r = BitReader::new(&[0b11110000, 0b00001111]);
            assert_eq!(r.read_bits(4).unwrap(), 0b0000);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
        }

        #[test]
        fn when_read_bits_with_zero_count_then_returns_zero() {
            let mut r = BitReader::new(&[0xFF]);
            assert_eq!(r.read_bits(0).unwrap(), 0);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
        }

        #[test]
        fn when_read_bit_with_exhausted_input_then_returns_error() {
            let mut r = BitReader::new(&[0x00]);
            r.read_bits(8).unwrap();
            assert_eq!(r.read_bit(), Err(InflateError::UnexpectedEof));
        }

        #[test]
        fn when_align_with_partial_byte_then_advances_to_next_byte() {
            let mut r = BitReader::new(&[0xFF, 0x42]);
            r.read_bits(3).unwrap();
            r.align();
            assert_eq!(r.pos, 1);
            assert_eq!(r.bit, 0);
            assert_eq!(r.read_bits(8).unwrap(), 0x42);
        }

        #[test]
        fn when_align_with_byte_boundary_then_position_unchanged() {
            let mut r = BitReader::new(&[0xFF, 0x42]);
            r.read_bits(8).unwrap();
            r.align();
            assert_eq!(r.pos, 1);
            assert_eq!(r.read_bits(8).unwrap(), 0x42);
        }
    }

    mod huffman_tree {
        use super::*;

        #[test]
        fn when_decode_with_single_code_then_returns_symbol() {
            let tree = HuffmanTree::from_lengths(&[0, 1]).unwrap();
            let mut r = BitReader::new(&[0b0]);
            assert_eq!(tree.decode(&mut r).unwrap(), 1);
        }

        #[test]
        fn when_decode_with_two_equal_length_codes_then_returns_correct_symbols() {
            let tree = HuffmanTree::from_lengths(&[1, 1]).unwrap();
            let mut r = BitReader::new(&[0b10]);
            assert_eq!(tree.decode(&mut r).unwrap(), 0);
            assert_eq!(tree.decode(&mut r).unwrap(), 1);
        }

        #[test]
        fn when_decode_with_variable_length_codes_then_returns_correct_symbols() {
            // A=1, B=2, C=2 → codes: A=0, B=10, C=11
            // Stream bits: 0, 1,0, 1,1 → packed LSB first = 0b00011010
            let tree = HuffmanTree::from_lengths(&[1, 2, 2]).unwrap();
            let mut r = BitReader::new(&[0b00011010]);
            assert_eq!(tree.decode(&mut r).unwrap(), 0);
            assert_eq!(tree.decode(&mut r).unwrap(), 1);
            assert_eq!(tree.decode(&mut r).unwrap(), 2);
        }

        #[test]
        fn when_decode_with_all_zero_lengths_then_returns_error() {
            let tree = HuffmanTree::from_lengths(&[0, 0, 0]).unwrap();
            let mut r = BitReader::new(&[0xFF]);
            assert_eq!(tree.decode(&mut r), Err(InflateError::InvalidHuffmanCode));
        }
    }

    mod stored_block {
        use super::*;

        #[test]
        fn when_inflate_with_stored_block_then_returns_original_bytes() {
            let input = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e', b'l', b'l', b'o'];
            assert_eq!(inflate(&input).unwrap(), b"Hello");
        }

        #[test]
        fn when_inflate_with_empty_stored_block_then_returns_empty() {
            let input = [0x01, 0x00, 0x00, 0xFF, 0xFF];
            assert_eq!(inflate(&input).unwrap(), b"");
        }

        #[test]
        fn when_inflate_with_invalid_nlen_then_returns_error() {
            let input = [0x01, 0x05, 0x00, 0x00, 0x00, b'H', b'e', b'l', b'l', b'o'];
            assert_eq!(inflate(&input), Err(InflateError::InvalidStoredLen));
        }

        #[test]
        fn when_inflate_with_consecutive_stored_blocks_then_concatenates() {
            let input = [
                0x00, 0x03, 0x00, 0xFC, 0xFF, b'H', b'e', b'l', 0x01, 0x02, 0x00, 0xFD, 0xFF, b'l',
                b'o',
            ];
            assert_eq!(inflate(&input).unwrap(), b"Hello");
        }
    }

    mod fixed_huffman {
        use super::*;

        #[test]
        fn when_inflate_with_end_of_block_only_then_returns_empty() {
            assert_eq!(inflate(&[0x03, 0x00]).unwrap(), b"");
        }

        #[test]
        fn when_inflate_with_single_literal_then_returns_byte() {
            assert_eq!(inflate(&[0x73, 0x04, 0x00]).unwrap(), b"A");
        }

        #[test]
        fn when_inflate_with_ascii_text_then_returns_decoded_bytes() {
            assert_eq!(
                inflate(&[0xF3, 0x48, 0xCD, 0xC9, 0xC9, 0x07, 0x00]).unwrap(),
                b"Hello"
            );
        }

        #[test]
        fn when_inflate_with_repeated_byte_then_expands_backreference() {
            assert_eq!(
                inflate(&[0x73, 0x74, 0xC4, 0x0E, 0x00]).unwrap(),
                b"AAAAAAAAAAAAAAAAAAAAAAAA"
            );
        }

        #[test]
        fn when_inflate_with_sequential_literals_then_returns_alphabet() {
            let compressed: &[u8] = &[
                0x4B, 0x4C, 0x4A, 0x4E, 0x49, 0x4D, 0x4B, 0xCF, 0xC8, 0xCC, 0xCA, 0xCE, 0xC9, 0xCD,
                0xCB, 0x2F, 0x28, 0x2C, 0x2A, 0x2E, 0x29, 0x2D, 0x2B, 0xAF, 0xA8, 0xAC, 0x02, 0x00,
            ];
            assert_eq!(inflate(compressed).unwrap(), b"abcdefghijklmnopqrstuvwxyz");
        }

        #[test]
        fn when_inflate_with_pattern_repetition_then_expands_correctly() {
            assert_eq!(
                inflate(&[0x4B, 0x4C, 0x4A, 0x4E, 0x84, 0x21, 0x00]).unwrap(),
                b"abcabcabcabc"
            );
        }

        #[test]
        fn when_inflate_with_long_text_and_backrefs_then_returns_complete_output() {
            let compressed: &[u8] = &[
                0x0B, 0xC9, 0x48, 0x55, 0x28, 0x2C, 0xCD, 0x4C, 0xCE, 0x56, 0x48, 0x2A, 0xCA, 0x2F,
                0xCF, 0x53, 0x48, 0xCB, 0xAF, 0x50, 0xC8, 0x2A, 0xCD, 0x2D, 0x28, 0x56, 0xC8, 0x2F,
                0x4B, 0x2D, 0x52, 0x28, 0x01, 0x4A, 0xE7, 0x24, 0x56, 0x55, 0x2A, 0xA4, 0xE4, 0xA7,
                0xEB, 0x29, 0x84, 0x90, 0xA0, 0x18, 0x00,
            ];
            assert_eq!(
                inflate(compressed).unwrap(),
                b"The quick brown fox jumps over the lazy dog. \
                  The quick brown fox jumps over the lazy dog."
            );
        }
    }

    mod regression {
        use super::*;

        #[test]
        fn when_inflate_with_dynamic_huffman_50k_then_decompresses() {
            let compressed: &[u8] = &[
                236, 193, 1, 1, 0, 0, 0, 128, 144, 219, 205, 239, 8, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 219, 131, 3,
                18, 0, 0, 0, 0, 65, 255, 95, 247, 35, 84, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 128, 147, 0,
            ];
            assert_eq!(inflate(compressed).unwrap(), vec![b'x'; 50000]);
        }

        #[test]
        fn when_inflate_with_all_256_byte_values_then_returns_complete_range() {
            let compressed: &[u8] = &[
                0x63, 0x60, 0x64, 0x62, 0x66, 0x61, 0x65, 0x63, 0xE7, 0xE0, 0xE4, 0xE2, 0xE6, 0xE1,
                0xE5, 0xE3, 0x17, 0x10, 0x14, 0x12, 0x16, 0x11, 0x15, 0x13, 0x97, 0x90, 0x94, 0x92,
                0x96, 0x91, 0x95, 0x93, 0x57, 0x50, 0x54, 0x52, 0x56, 0x51, 0x55, 0x53, 0xD7, 0xD0,
                0xD4, 0xD2, 0xD6, 0xD1, 0xD5, 0xD3, 0x37, 0x30, 0x34, 0x32, 0x36, 0x31, 0x35, 0x33,
                0xB7, 0xB0, 0xB4, 0xB2, 0xB6, 0xB1, 0xB5, 0xB3, 0x77, 0x70, 0x74, 0x72, 0x76, 0x71,
                0x75, 0x73, 0xF7, 0xF0, 0xF4, 0xF2, 0xF6, 0xF1, 0xF5, 0xF3, 0x0F, 0x08, 0x0C, 0x0A,
                0x0E, 0x09, 0x0D, 0x0B, 0x8F, 0x88, 0x8C, 0x8A, 0x8E, 0x89, 0x8D, 0x8B, 0x4F, 0x48,
                0x4C, 0x4A, 0x4E, 0x49, 0x4D, 0x4B, 0xCF, 0xC8, 0xCC, 0xCA, 0xCE, 0xC9, 0xCD, 0xCB,
                0x2F, 0x28, 0x2C, 0x2A, 0x2E, 0x29, 0x2D, 0x2B, 0xAF, 0xA8, 0xAC, 0xAA, 0xAE, 0xA9,
                0xAD, 0xAB, 0x6F, 0x68, 0x6C, 0x6A, 0x6E, 0x69, 0x6D, 0x6B, 0xEF, 0xE8, 0xEC, 0xEA,
                0xEE, 0xE9, 0xED, 0xEB, 0x9F, 0x30, 0x71, 0xD2, 0xE4, 0x29, 0x53, 0xA7, 0x4D, 0x9F,
                0x31, 0x73, 0xD6, 0xEC, 0x39, 0x73, 0xE7, 0xCD, 0x5F, 0xB0, 0x70, 0xD1, 0xE2, 0x25,
                0x4B, 0x97, 0x2D, 0x5F, 0xB1, 0x72, 0xD5, 0xEA, 0x35, 0x6B, 0xD7, 0xAD, 0xDF, 0xB0,
                0x71, 0xD3, 0xE6, 0x2D, 0x5B, 0xB7, 0x6D, 0xDF, 0xB1, 0x73, 0xD7, 0xEE, 0x3D, 0x7B,
                0xF7, 0xED, 0x3F, 0x70, 0xF0, 0xD0, 0xE1, 0x23, 0x47, 0x8F, 0x1D, 0x3F, 0x71, 0xF2,
                0xD4, 0xE9, 0x33, 0x67, 0xCF, 0x9D, 0xBF, 0x70, 0xF1, 0xD2, 0xE5, 0x2B, 0x57, 0xAF,
                0x5D, 0xBF, 0x71, 0xF3, 0xD6, 0xED, 0x3B, 0x77, 0xEF, 0xDD, 0x7F, 0xF0, 0xF0, 0xD1,
                0xE3, 0x27, 0x4F, 0x9F, 0x3D, 0x7F, 0xF1, 0xF2, 0xD5, 0xEB, 0x37, 0x6F, 0xDF, 0xBD,
                0xFF, 0xF0, 0xF1, 0xD3, 0xE7, 0x2F, 0x5F, 0xBF, 0x7D, 0xFF, 0xF1, 0xF3, 0xD7, 0xEF,
                0x3F, 0x7F, 0xFF, 0xFD, 0x67, 0x18, 0xF5, 0xFF, 0xA8, 0xFF, 0x47, 0xB0, 0xFF, 0x01,
            ];
            let expected: Vec<u8> = (0..=255u8).cycle().take(1024).collect();
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_length_code_285_then_expands_max_length() {
            let compressed: &[u8] = &[115, 116, 28, 5, 196, 2, 0];
            assert_eq!(inflate(compressed).unwrap(), vec![b'A'; 300]);
        }

        #[test]
        fn when_inflate_with_32k_distance_then_copies_from_far_back() {
            let compressed: &[u8] = &[
                237, 207, 7, 91, 77, 1, 0, 128, 225, 148, 77, 200, 138, 10, 41, 21, 146, 82, 132,
                8, 39, 78, 198, 117, 175, 235, 56, 40, 228, 102, 53, 172, 202, 202, 78, 217, 123,
                132, 74, 11, 161, 236, 153, 213, 176, 183, 118, 246, 104, 136, 80, 52, 205, 20,
                143, 255, 241, 189, 255, 224, 85, 11, 178, 44, 74, 42, 141, 82, 144, 20, 162, 164,
                85, 75, 91, 167, 118, 157, 186, 245, 234, 55, 104, 216, 168, 177, 110, 147, 166,
                205, 244, 154, 183, 104, 217, 170, 181, 126, 155, 182, 6, 134, 70, 237, 218, 119,
                48, 238, 104, 98, 218, 201, 204, 220, 162, 115, 151, 174, 150, 221, 172, 186, 91,
                219, 244, 176, 181, 235, 217, 203, 190, 119, 159, 190, 14, 253, 250, 59, 14, 24,
                56, 72, 112, 26, 60, 68, 116, 30, 58, 108, 248, 8, 197, 72, 165, 106, 148, 122,
                180, 52, 70, 30, 59, 110, 188, 139, 235, 132, 137, 147, 220, 38, 107, 220, 167, 76,
                157, 54, 125, 134, 135, 167, 151, 247, 204, 89, 179, 231, 204, 245, 241, 245, 155,
                55, 127, 193, 194, 69, 254, 139, 151, 44, 93, 182, 124, 197, 202, 128, 85, 129, 65,
                171, 215, 172, 93, 183, 126, 195, 198, 77, 155, 183, 108, 221, 182, 125, 199, 206,
                93, 193, 187, 247, 236, 13, 9, 13, 219, 23, 30, 17, 25, 21, 189, 255, 192, 193,
                152, 67, 135, 143, 196, 198, 29, 61, 118, 252, 196, 201, 83, 167, 207, 156, 61,
                119, 254, 66, 252, 197, 75, 151, 175, 92, 77, 72, 76, 74, 190, 118, 253, 198, 205,
                91, 183, 239, 220, 189, 119, 255, 193, 195, 71, 143, 83, 82, 211, 210, 51, 50, 179,
                178, 159, 60, 125, 246, 252, 197, 203, 87, 175, 223, 188, 205, 201, 205, 203, 127,
                87, 240, 254, 67, 225, 199, 79, 159, 139, 138, 191, 124, 45, 41, 45, 43, 175, 168,
                252, 246, 253, 199, 207, 95, 191, 171, 254, 84, 215, 252, 229, 207, 159, 63, 127,
                254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63,
                127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159,
                63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207,
                159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231,
                207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243,
                231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252, 249,
                243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254, 252,
                249, 243, 231, 207, 159, 63, 127, 254, 252, 249, 243, 231, 207, 159, 63, 127, 254,
                252, 255, 255, 213, 130, 44, 139, 146, 74, 163, 20, 36, 133, 40, 253, 3,
            ];
            let filler: Vec<u8> = (0..=255).cycle().take(32768).collect();
            let mut expected = b"PATTERN_MARKER".to_vec();
            expected.extend_from_slice(&filler);
            expected.extend_from_slice(b"PATTERN_MARKER");
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_stored_256_values_then_returns_complete_range() {
            let compressed: &[u8] = &[
                1, 0, 1, 255, 254, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17,
                18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
                39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59,
                60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80,
                81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100,
                101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116,
                117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130, 131, 132,
                133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148,
                149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164,
                165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180,
                181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196,
                197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207, 208, 209, 210, 211, 212,
                213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228,
                229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 240, 241, 242, 243, 244,
                245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255,
            ];
            let expected: Vec<u8> = (0..=255).collect();
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_alternating_bytes_then_returns_pattern() {
            let compressed: &[u8] = &[75, 76, 74, 28, 133, 163, 112, 20, 14, 115, 8, 0];
            let expected: Vec<u8> = b"ab".iter().copied().cycle().take(1000).collect();
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_dynamic_multiblock_then_returns_5000_bytes() {
            let compressed: &[u8] = &[
                237, 193, 49, 1, 0, 0, 0, 194, 160, 218, 139, 111, 10, 63, 160, 0, 0, 0, 0, 128,
                183, 1,
            ];
            assert_eq!(inflate(compressed).unwrap(), vec![b'x'; 5000]);
        }

        #[test]
        fn when_inflate_with_rle_frequency_distribution_then_returns_correct_data() {
            let compressed: &[u8] = &[
                149, 193, 135, 2, 66, 80, 20, 0, 80, 30, 73, 42, 69, 162, 33, 162, 168, 52, 232,
                255, 127, 46, 90, 214, 27, 247, 158, 35, 201, 50, 33, 68, 41, 169, 149, 193, 155,
                246, 49, 252, 210, 127, 70, 127, 70, 109, 220, 48, 105, 154, 182, 152, 109, 179,
                142, 121, 151, 213, 99, 247, 45, 40, 28, 154, 37, 149, 75, 231, 49, 172, 88, 214,
                76, 27, 182, 45, 135, 207, 179, 227, 10, 248, 66, 129, 189, 72, 36, 20, 139, 29, 0,
                142, 16, 9, 72, 10, 115, 2, 58, 67, 93, 192, 50, 184, 43, 194, 13, 227, 142, 242,
                192, 201, 145, 10, 172, 39, 218, 11,
            ];
            let expected: Vec<u8> = (0u8..50).flat_map(|i| vec![i; i as usize + 1]).collect();
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_growing_runs_then_returns_4950_bytes() {
            let compressed: &[u8] = &[
                237, 193, 49, 1, 0, 0, 0, 194, 160, 108, 235, 95, 202, 12, 254, 64, 1, 0, 0, 0, 0,
                159, 1,
            ];
            let expected: Vec<u8> = (1..100u32).flat_map(|n| vec![b'A'; n as usize]).collect();
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_null_bytes_then_handles_binary() {
            let compressed: &[u8] = &[99, 96, 160, 61, 96, 164, 3, 248, 79, 7, 0, 0];
            let mut expected = vec![0x00; 100];
            expected.extend(vec![0x01; 100]);
            expected.extend(vec![0xFF; 100]);
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        #[test]
        fn when_inflate_with_stored_then_fixed_blocks_then_concatenates() {
            let input: &[u8] = &[
                0x00, 0x03, 0x00, 0xFC, 0xFF, b'H', b'e', b'l', 0xCB, 0xC9, 0x07, 0x00,
            ];
            assert_eq!(inflate(input).unwrap(), b"Hello");
        }

        #[test]
        fn when_inflate_with_medium_distance_then_copies_across_8k() {
            let compressed: &[u8] = &[
                237, 207, 197, 82, 2, 0, 0, 69, 209, 127, 178, 177, 65, 236, 70, 69, 108, 176, 91,
                81, 108, 236, 198, 238, 192, 192, 238, 192, 86, 76, 236, 86, 108, 236, 238, 238,
                157, 51, 250, 9, 110, 223, 246, 174, 238, 33, 146, 212, 200, 234, 26, 18, 146, 82,
                210, 50, 178, 114, 4, 121, 5, 69, 37, 101, 21, 85, 226, 111, 212, 212, 210, 214,
                209, 213, 211, 55, 48, 52, 50, 166, 152, 152, 154, 81, 205, 105, 22, 150, 86, 214,
                54, 182, 118, 116, 134, 189, 131, 163, 147, 179, 139, 171, 155, 187, 135, 167, 151,
                183, 143, 47, 211, 207, 159, 21, 16, 24, 20, 28, 18, 26, 198, 14, 143, 136, 140,
                138, 142, 137, 141, 139, 79, 72, 76, 74, 230, 164, 164, 166, 165, 103, 100, 102,
                101, 231, 228, 230, 229, 23, 20, 22, 21, 115, 75, 74, 203, 202, 121, 21, 149, 85,
                213, 53, 181, 117, 245, 13, 141, 77, 205, 45, 173, 109, 237, 252, 142, 206, 174,
                238, 158, 222, 190, 254, 1, 193, 224, 208, 240, 200, 232, 152, 112, 124, 98, 114,
                106, 122, 102, 118, 110, 126, 97, 113, 105, 121, 101, 85, 180, 182, 190, 177, 185,
                181, 189, 35, 222, 221, 219, 63, 56, 60, 58, 62, 57, 61, 59, 191, 184, 188, 186,
                190, 185, 189, 187, 127, 120, 124, 122, 126, 121, 125, 123, 255, 248, 252, 250,
                134, 3, 14, 56, 224, 128, 3, 14, 56, 224, 128, 3, 14, 56, 224, 128, 3, 14, 56, 224,
                128, 3, 14, 56, 224, 128, 3, 14, 56, 224, 128, 3, 14, 56, 224, 128, 3, 142, 255,
                56, 254, 198, 127, 0,
            ];
            let filler: Vec<u8> = (0..8000).map(|i| (i % 200 + 50) as u8).collect();
            let mut expected = b"ABCDEF".to_vec();
            expected.extend_from_slice(&filler);
            expected.extend_from_slice(b"ABCDEF");
            assert_eq!(inflate(compressed).unwrap(), expected);
        }

        macro_rules! repeat_byte_test {
            ($name:ident, $compressed:expr, $size:expr) => {
                #[test]
                fn $name() {
                    assert_eq!(inflate($compressed).unwrap(), vec![b'Z'; $size]);
                }
            };
        }

        repeat_byte_test!(
            when_inflate_with_3_repeated_bytes_then_returns_3,
            &[139, 138, 138, 2, 0],
            3
        );
        repeat_byte_test!(
            when_inflate_with_10_repeated_bytes_then_returns_10,
            &[139, 138, 130, 1, 0],
            10
        );
        repeat_byte_test!(
            when_inflate_with_50_repeated_bytes_then_returns_50,
            &[139, 138, 34, 21, 0, 0],
            50
        );
        repeat_byte_test!(
            when_inflate_with_258_repeated_bytes_then_returns_max_single_length,
            &[139, 138, 26, 233, 0, 0],
            258
        );
        repeat_byte_test!(
            when_inflate_with_500_repeated_bytes_then_returns_500,
            &[139, 138, 26, 5, 35, 13, 0, 0],
            500
        );
        repeat_byte_test!(
            when_inflate_with_1000_repeated_bytes_then_returns_1000,
            &[139, 138, 26, 5, 163, 96, 20, 12, 119, 0, 0],
            1000
        );
    }

    mod errors {
        use super::*;

        #[test]
        fn when_inflate_with_empty_input_then_returns_eof_error() {
            assert_eq!(inflate(&[]), Err(InflateError::UnexpectedEof));
        }

        #[test]
        fn when_inflate_with_block_type_3_then_returns_invalid_block_error() {
            assert_eq!(inflate(&[0x07]), Err(InflateError::InvalidBlockType));
        }

        #[test]
        fn when_inflate_with_truncated_data_then_returns_eof_error() {
            let input = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e'];
            assert_eq!(inflate(&input), Err(InflateError::UnexpectedEof));
        }
    }
}
