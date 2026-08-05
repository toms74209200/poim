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

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(size: usize) -> *mut u8 {
    allocate(size)
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

            assert_eq!(first, second, "freed block was not returned to the allocator");

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
