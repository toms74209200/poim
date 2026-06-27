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
        fn read_single_bits() {
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
        fn read_multi_bits() {
            let mut r = BitReader::new(&[0b11010010]);
            assert_eq!(r.read_bits(3).unwrap(), 0b010);
            assert_eq!(r.read_bits(5).unwrap(), 0b11010);
        }

        #[test]
        fn read_across_bytes() {
            let mut r = BitReader::new(&[0xFF, 0x01]);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
            assert_eq!(r.read_bits(2).unwrap(), 0b01);
        }

        #[test]
        fn read_bits_spanning_boundary() {
            let mut r = BitReader::new(&[0b11110000, 0b00001111]);
            assert_eq!(r.read_bits(4).unwrap(), 0b0000);
            assert_eq!(r.read_bits(8).unwrap(), 0b00001111_1111);
            // Wait, that's not right. Let me recalculate.
            // After reading 4 bits from byte 0: got 0b0000 (bits 0-3)
            // Now at byte 0, bit 4. Next 8 bits: bits 4-7 of byte 0 (1111) + bits 0-3 of byte 1 (1111)
            // LSB first: value = 1111_1111 = 0xFF
        }

        #[test]
        fn read_bits_spanning_boundary_value() {
            let mut r = BitReader::new(&[0b11110000, 0b00001111]);
            assert_eq!(r.read_bits(4).unwrap(), 0b0000);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
        }

        #[test]
        fn read_zero_bits() {
            let mut r = BitReader::new(&[0xFF]);
            assert_eq!(r.read_bits(0).unwrap(), 0);
            assert_eq!(r.read_bits(8).unwrap(), 0xFF);
        }

        #[test]
        fn eof_returns_error() {
            let mut r = BitReader::new(&[0x00]);
            r.read_bits(8).unwrap();
            assert_eq!(r.read_bit(), Err(InflateError::UnexpectedEof));
        }

        #[test]
        fn align_skips_remaining_bits() {
            let mut r = BitReader::new(&[0xFF, 0x42]);
            r.read_bits(3).unwrap();
            r.align();
            assert_eq!(r.pos, 1);
            assert_eq!(r.bit, 0);
            assert_eq!(r.read_bits(8).unwrap(), 0x42);
        }

        #[test]
        fn align_at_boundary_is_noop() {
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
        fn single_symbol() {
            let tree = HuffmanTree::from_lengths(&[0, 1]).unwrap();
            let mut r = BitReader::new(&[0b0]);
            assert_eq!(tree.decode(&mut r).unwrap(), 1);
        }

        #[test]
        fn two_symbols() {
            let tree = HuffmanTree::from_lengths(&[1, 1]).unwrap();
            let mut r = BitReader::new(&[0b10]);
            assert_eq!(tree.decode(&mut r).unwrap(), 0);
            assert_eq!(tree.decode(&mut r).unwrap(), 1);
        }

        #[test]
        fn three_symbols() {
            // Lengths: A=1, B=2, C=2 → codes: A=0, B=10, C=11
            let tree = HuffmanTree::from_lengths(&[1, 2, 2]).unwrap();
            // Bit stream: A(0), B(10), C(11) → bits LSB first: 0, 01, 11
            // byte: bits 0=0(A), 1=0(B msb), 2=1(B lsb), 3=1(C msb), 4=1(C lsb)
            // Actually for Huffman, MSB is read first from the stream.
            // read_bit returns bits in stream order (LSB of byte first).
            // For code A=0 (1 bit): read_bit returns 0 → follows left → leaf A. ✓
            // For code B=10 (2 bits): first bit is 1 (MSB), second is 0 (LSB).
            //   read_bit returns 1 → right child, then 0 → left child → leaf B. ✓
            // For code C=11 (2 bits): first bit is 1, second is 1.
            //   read_bit returns 1 → right, 1 → right → leaf C. ✓
            // Bits in order: 0, 1, 0, 1, 1
            // Packed into byte LSB first: bit0=0, bit1=1, bit2=0, bit3=1, bit4=1 = 0b11010 = 0x1A
            let mut r = BitReader::new(&[0b00011010]);
            assert_eq!(tree.decode(&mut r).unwrap(), 0); // A
            assert_eq!(tree.decode(&mut r).unwrap(), 1); // B
            assert_eq!(tree.decode(&mut r).unwrap(), 2); // C
        }

        #[test]
        fn empty_tree_errors_on_decode() {
            let tree = HuffmanTree::from_lengths(&[0, 0, 0]).unwrap();
            let mut r = BitReader::new(&[0xFF]);
            assert_eq!(tree.decode(&mut r), Err(InflateError::InvalidHuffmanCode));
        }
    }

    mod stored_block {
        use super::*;

        #[test]
        fn simple_stored() {
            // BFINAL=1, BTYPE=00 → first 3 bits: 1,0,0 → byte 0x01
            // Then align to byte boundary (already aligned after 3 bits? No, 3 bits used, 5 remaining)
            // Actually: byte[0] = 0b_xxxxx_00_1 where 00=BTYPE, 1=BFINAL
            // Low 3 bits used, 5 bits padding after align
            // LEN=5 (0x0500), NLEN=0xFAFF
            // Data: "Hello"
            let input = [
                0x01, // BFINAL=1, BTYPE=00 (stored)
                0x05, 0x00, // LEN = 5
                0xFA, 0xFF, // NLEN = !5
                b'H', b'e', b'l', b'l', b'o',
            ];
            assert_eq!(inflate(&input).unwrap(), b"Hello");
        }

        #[test]
        fn empty_stored() {
            let input = [
                0x01, // BFINAL=1, BTYPE=00
                0x00, 0x00, // LEN = 0
                0xFF, 0xFF, // NLEN = !0
            ];
            assert_eq!(inflate(&input).unwrap(), b"");
        }

        #[test]
        fn bad_nlen() {
            let input = [
                0x01, 0x05, 0x00, 0x00, 0x00, // NLEN != !LEN
                b'H', b'e', b'l', b'l', b'o',
            ];
            assert_eq!(inflate(&input), Err(InflateError::InvalidStoredLen));
        }

        #[test]
        fn multiple_stored_blocks() {
            let input = [
                0x00, // BFINAL=0, BTYPE=00
                0x03, 0x00, 0xFC, 0xFF, // LEN=3, NLEN=!3
                b'H', b'e', b'l', 0x01, // BFINAL=1, BTYPE=00
                0x02, 0x00, 0xFD, 0xFF, // LEN=2, NLEN=!2
                b'l', b'o',
            ];
            assert_eq!(inflate(&input).unwrap(), b"Hello");
        }
    }

    mod fixed_huffman {
        use super::*;

        #[test]
        fn empty() {
            let compressed: &[u8] = &[0x03, 0x00];
            assert_eq!(inflate(compressed).unwrap(), b"");
        }

        #[test]
        fn single_byte() {
            let compressed: &[u8] = &[0x73, 0x04, 0x00];
            assert_eq!(inflate(compressed).unwrap(), b"A");
        }

        #[test]
        fn hello() {
            let compressed: &[u8] = &[0xF3, 0x48, 0xCD, 0xC9, 0xC9, 0x07, 0x00];
            assert_eq!(inflate(compressed).unwrap(), b"Hello");
        }

        #[test]
        fn repeated_a() {
            let compressed: &[u8] = &[0x73, 0x74, 0xC4, 0x0E, 0x00];
            assert_eq!(inflate(compressed).unwrap(), b"AAAAAAAAAAAAAAAAAAAAAAAA");
        }

        #[test]
        fn alphabet() {
            let compressed: &[u8] = &[
                0x4B, 0x4C, 0x4A, 0x4E, 0x49, 0x4D, 0x4B, 0xCF, 0xC8, 0xCC, 0xCA, 0xCE, 0xC9, 0xCD,
                0xCB, 0x2F, 0x28, 0x2C, 0x2A, 0x2E, 0x29, 0x2D, 0x2B, 0xAF, 0xA8, 0xAC, 0x02, 0x00,
            ];
            assert_eq!(inflate(compressed).unwrap(), b"abcdefghijklmnopqrstuvwxyz");
        }

        #[test]
        fn backreference() {
            let compressed: &[u8] = &[0x4B, 0x4C, 0x4A, 0x4E, 0x84, 0x21, 0x00];
            assert_eq!(inflate(compressed).unwrap(), b"abcabcabcabc");
        }

        #[test]
        fn longer_text() {
            let compressed: &[u8] = &[
                0x0B, 0xC9, 0x48, 0x55, 0x28, 0x2C, 0xCD, 0x4C, 0xCE, 0x56, 0x48, 0x2A, 0xCA, 0x2F,
                0xCF, 0x53, 0x48, 0xCB, 0xAF, 0x50, 0xC8, 0x2A, 0xCD, 0x2D, 0x28, 0x56, 0xC8, 0x2F,
                0x4B, 0x2D, 0x52, 0x28, 0x01, 0x4A, 0xE7, 0x24, 0x56, 0x55, 0x2A, 0xA4, 0xE4, 0xA7,
                0xEB, 0x29, 0x84, 0x90, 0xA0, 0x18, 0x00,
            ];
            assert_eq!(
                inflate(compressed).unwrap(),
                b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog."
            );
        }
    }

    mod dynamic_huffman {
        use super::*;

        #[test]
        fn repeated_byte_multiblock() {
            let compressed: &[u8] = &[
                236, 193, 1, 1, 0, 0, 0, 128, 144, 219, 205, 239, 8, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 219, 131, 3,
                18, 0, 0, 0, 0, 65, 255, 95, 247, 35, 84, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 128, 147, 0,
            ];
            let expected = vec![b'x'; 50000];
            assert_eq!(inflate(compressed).unwrap(), expected);
        }
    }

    mod all_bytes {
        use super::*;

        #[test]
        fn all_byte_values_repeated() {
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
            let mut expected = Vec::new();
            for _ in 0..4 {
                for b in 0..=255u8 {
                    expected.push(b);
                }
            }
            assert_eq!(inflate(compressed).unwrap(), expected);
        }
    }

    mod errors {
        use super::*;

        #[test]
        fn empty_input() {
            assert_eq!(inflate(&[]), Err(InflateError::UnexpectedEof));
        }

        #[test]
        fn invalid_block_type() {
            // BFINAL=1, BTYPE=11 → bits: 1,1,1 → byte 0b00000111 = 0x07
            assert_eq!(inflate(&[0x07]), Err(InflateError::InvalidBlockType));
        }

        #[test]
        fn truncated_stored_block() {
            let input = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e'];
            assert_eq!(inflate(&input), Err(InflateError::UnexpectedEof));
        }
    }
}
