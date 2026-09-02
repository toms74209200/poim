use core::alloc::Layout;

const ALIGNMENT: usize = 1;

pub fn allocate(size: usize) -> *mut u8 {
    if size == 0 {
        return core::ptr::null_mut();
    }
    match Layout::from_size_align(size, ALIGNMENT) {
        Ok(layout) => unsafe { std::alloc::alloc(layout) },
        Err(_) => core::ptr::null_mut(),
    }
}

/// # Safety
///
/// `ptr` must have come from [`allocate`] with the same `size`, and must not
/// have been deallocated already.
pub unsafe fn deallocate(ptr: *mut u8, size: usize) {
    if ptr.is_null() || size == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(size, ALIGNMENT) {
        unsafe { std::alloc::dealloc(ptr, layout) };
    }
}

pub const STATUS_OK: u32 = 0;
pub const STATUS_ERROR: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    Epub,
    Pdf,
}

impl InputFormat {
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(Self::Epub),
            1 => Some(Self::Pdf),
            _ => None,
        }
    }
}

pub fn convert_packed(data: &[u8], format: u32) -> Vec<u8> {
    let Some(format) = InputFormat::from_code(format) else {
        return pack_error(&format!("unsupported input format: {format}"));
    };
    match format {
        InputFormat::Epub => match crate::convert::convert_epub(data) {
            Ok(conversion) => pack_conversion(&conversion),
            Err(error) => pack_error(&error.to_string()),
        },
        InputFormat::Pdf => match crate::convert::convert_pdf(data) {
            Ok(conversion) => pack_conversion(&conversion),
            Err(error) => pack_error(&error.to_string()),
        },
    }
}

fn pack_conversion(conversion: &crate::convert::Conversion) -> Vec<u8> {
    let mut payload = Vec::new();
    push_bytes(&mut payload, conversion.markdown.as_bytes());
    push_u32(&mut payload, conversion.images.len() as u32);
    for image in &conversion.images {
        push_bytes(&mut payload, image.path.as_str().as_bytes());
        push_bytes(&mut payload, &image.data);
    }
    finish(STATUS_OK, payload)
}

fn pack_error(message: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    push_bytes(&mut payload, message.as_bytes());
    finish(STATUS_ERROR, payload)
}

fn finish(status: u32, payload: Vec<u8>) -> Vec<u8> {
    let total = (payload.len() + 8) as u32;
    let mut packed = Vec::with_capacity(total as usize);
    push_u32(&mut packed, total);
    push_u32(&mut packed, status);
    packed.extend_from_slice(&payload);
    packed
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_bytes(buffer: &mut Vec<u8>, bytes: &[u8]) {
    push_u32(buffer, bytes.len() as u32);
    buffer.extend_from_slice(bytes);
}

#[cfg(target_arch = "wasm32")]
fn leak(packed: Vec<u8>) -> *mut u8 {
    let pointer = allocate(packed.len());
    if !pointer.is_null() {
        unsafe { core::ptr::copy_nonoverlapping(packed.as_ptr(), pointer, packed.len()) };
    }
    pointer
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    allocate(size)
}

/// # Safety
///
/// `ptr` must point to `len` readable bytes, as returned by `alloc`.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn convert(ptr: *const u8, len: usize, format: u32) -> *mut u8 {
    let data = unsafe { core::slice::from_raw_parts(ptr, len) };
    leak(convert_packed(data, format))
}

/// # Safety
///
/// `ptr` must have come from `alloc` with the same `size`, and must not have
/// been freed already.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8, size: usize) {
    unsafe { deallocate(ptr, size) };
}

#[cfg(test)]
mod tests {
    use super::*;

    mod convert_packed {
        use super::*;

        struct Reader<'a> {
            bytes: &'a [u8],
            pos: usize,
        }

        impl<'a> Reader<'a> {
            fn new(bytes: &'a [u8]) -> Self {
                Self { bytes, pos: 0 }
            }

            fn u32(&mut self) -> u32 {
                let value =
                    u32::from_le_bytes(self.bytes[self.pos..self.pos + 4].try_into().unwrap());
                self.pos += 4;
                value
            }

            fn bytes(&mut self) -> &'a [u8] {
                let len = self.u32() as usize;
                let slice = &self.bytes[self.pos..self.pos + len];
                self.pos += len;
                slice
            }

            fn text(&mut self) -> String {
                String::from_utf8(self.bytes().to_vec()).unwrap()
            }
        }

        fn epub() -> Vec<u8> {
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let container = br#"<container><rootfiles>
<rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;
            crate::convert::tests_support::zip(&[
                ("META-INF/container.xml", container.to_vec()),
                ("OEBPS/content.opf", opf.to_vec()),
                (
                    "OEBPS/chapter1.xhtml",
                    br#"<h1>Title</h1><img src="fig.png" alt="F"/>"#.to_vec(),
                ),
                ("OEBPS/fig.png", b"PNGDATA".to_vec()),
            ])
        }

        #[test]
        fn when_conversion_succeeds_then_packs_markdown_and_images() {
            let packed = convert_packed(&epub(), 0);

            let mut reader = Reader::new(&packed);
            assert_eq!(reader.u32() as usize, packed.len());
            assert_eq!(reader.u32(), STATUS_OK);
            assert!(reader.text().contains("# Title"));
            assert_eq!(reader.u32(), 1);
            assert_eq!(reader.text(), "OEBPS/fig.png");
            assert_eq!(reader.bytes(), b"PNGDATA");
            assert_eq!(reader.pos, packed.len());
        }

        #[test]
        fn when_input_is_not_an_epub_then_packs_the_error_message() {
            let packed = convert_packed(b"not an epub", 0);

            let mut reader = Reader::new(&packed);
            assert_eq!(reader.u32() as usize, packed.len());
            assert_eq!(reader.u32(), STATUS_ERROR);
            assert!(!reader.text().is_empty());
            assert_eq!(reader.pos, packed.len());
        }

        #[test]
        fn when_input_is_a_pdf_then_packs_its_markdown() {
            let pdf = crate::convert::tests_support::pdf("BT /F1 12 Tf 50 700 Td (Title) Tj ET");
            let packed = convert_packed(&pdf, 1);

            let mut reader = Reader::new(&packed);
            assert_eq!(reader.u32() as usize, packed.len());
            assert_eq!(reader.u32(), STATUS_OK);
            assert_eq!(reader.text(), "Title");
            assert_eq!(reader.u32(), 0);
            assert_eq!(reader.pos, packed.len());
        }

        #[test]
        fn when_input_is_not_a_pdf_then_packs_the_error_message() {
            let packed = convert_packed(b"not a pdf", 1);

            let mut reader = Reader::new(&packed);
            assert_eq!(reader.u32() as usize, packed.len());
            assert_eq!(reader.u32(), STATUS_ERROR);
            assert!(!reader.text().is_empty());
            assert_eq!(reader.pos, packed.len());
        }

        #[test]
        fn when_format_is_unknown_then_packs_the_error_message() {
            let packed = convert_packed(&epub(), 99);

            let mut reader = Reader::new(&packed);
            reader.u32();
            assert_eq!(reader.u32(), STATUS_ERROR);
            assert_eq!(reader.text(), "unsupported input format: 99");
        }

        #[test]
        fn when_there_is_no_image_then_image_count_is_zero() {
            let container = br#"<container><rootfiles>
<rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
</rootfiles></container>"#;
            let opf = br#"<package>
  <manifest><item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#;
            let epub = crate::convert::tests_support::zip(&[
                ("META-INF/container.xml", container.to_vec()),
                ("OEBPS/content.opf", opf.to_vec()),
                ("OEBPS/chapter1.xhtml", b"<h1>Only</h1>".to_vec()),
            ]);

            let packed = convert_packed(&epub, 0);

            let mut reader = Reader::new(&packed);
            reader.u32();
            assert_eq!(reader.u32(), STATUS_OK);
            reader.text();
            assert_eq!(reader.u32(), 0);
            assert_eq!(reader.pos, packed.len());
        }
    }

    mod allocate {
        use super::*;

        #[test]
        fn when_size_is_positive_then_returns_non_null() {
            let ptr = allocate(16);

            assert!(!ptr.is_null());

            unsafe { deallocate(ptr, 16) };
        }

        #[test]
        fn when_size_is_zero_then_returns_null() {
            assert!(allocate(0).is_null());
        }

        #[test]
        fn when_allocated_then_buffer_is_writable_and_readable() {
            let size = 5;
            let ptr = allocate(size);

            let written = b"hello";
            unsafe {
                core::ptr::copy_nonoverlapping(written.as_ptr(), ptr, size);
                assert_eq!(core::slice::from_raw_parts(ptr, size), written);
                deallocate(ptr, size);
            }
        }

        #[test]
        fn when_allocated_twice_then_returns_distinct_buffers() {
            let first = allocate(8);
            let second = allocate(8);

            assert_ne!(first, second);

            unsafe {
                deallocate(first, 8);
                deallocate(second, 8);
            }
        }
    }

    mod deallocate {
        use super::*;

        #[test]
        fn when_pointer_is_null_then_does_nothing() {
            unsafe { deallocate(core::ptr::null_mut(), 8) };
        }

        #[test]
        fn when_size_is_zero_then_does_nothing() {
            let ptr = allocate(4);

            unsafe { deallocate(ptr, 0) };

            unsafe { deallocate(ptr, 4) };
        }

        #[test]
        fn when_deallocated_then_allocator_reuses_the_block() {
            let size = 1024;
            let first = allocate(size);
            unsafe { deallocate(first, size) };
            let second = allocate(size);

            assert_eq!(
                first, second,
                "freed block was not returned to the allocator"
            );

            unsafe { deallocate(second, size) };
        }

        #[test]
        fn when_allocation_is_reused_then_round_trip_succeeds() {
            for _ in 0..64 {
                let ptr = allocate(1024);
                assert!(!ptr.is_null());
                unsafe { deallocate(ptr, 1024) };
            }
        }
    }
}
