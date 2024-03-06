//! A Rust global allocator that uses the stdlib allocator through libc.

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};

use alloc::alloc::handle_alloc_error;

/// Define size_t, as it isn't defined within ffi.
#[allow(non_camel_case_types)]
type c_size_t = usize;

extern "C" {
    fn malloc(size: c_size_t) -> *mut u8;
    fn free(ptr: *mut u8);
}

pub struct ZephyrAllocator;

unsafe impl GlobalAlloc for ZephyrAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // The C allocation library assumes a layout of 8.  For now, just panic
        // if this cannot be satisfied.
        if align > 8 {
            handle_alloc_error(layout);
        }

        malloc(size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        free(ptr);
    }
}
