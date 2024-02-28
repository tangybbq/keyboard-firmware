#![no_std]

extern crate alloc;

use core::ffi::c_char;

use core::panic::PanicInfo;

use zephyr_sys::ZephyrAllocator;

use crate::zephyr::message;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        zephyr_sys::c_k_panic();
    }
}

#[global_allocator]
static ZEPHYR_ALLOCATOR: ZephyrAllocator = ZephyrAllocator;

#[no_mangle]
fn rust_main() {
    unsafe {
        msg("Hello from Rust\n\0".as_ptr().cast());
        message("This is another message from Rust.");
    }

    panic!("Rust panic happened");
}

extern "C" {
    fn msg(msg: *const c_char);
}

mod zephyr {
    use alloc::ffi::CString;

    // Print a basic message from a Rust string.  This isn't particularly
    // efficient, as the message will be heap allocated (and then immediately
    // freed).
    pub fn message(text: &str) {
        let text = CString::new(text).expect("CString::new failed");
        unsafe {
            crate::zephyr_sys::msg_string(text.as_ptr());
        }
    }
}

mod zephyr_sys {
    use core::{alloc::{GlobalAlloc, Layout}, ffi::c_char};

    extern "C" {
        pub fn c_k_panic() -> !;

        pub fn malloc(size: c_size_t) -> *mut u8;
        // pub fn realloc(ptr: *mut u8, size: c_size_t) -> *mut u8;
        pub fn free(ptr: *mut u8);

        // Log this message.
        pub fn msg_string(text: *const c_char);
    }

    pub struct ZephyrAllocator;

    unsafe impl GlobalAlloc for ZephyrAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();

            // The picolibc/newlib malloc has an alignment of 8.  Any more than
            // this requires memalign, which isn't efficient.
            if align > 8 {
                panic!("ZephyrAllocator, attempt at large alignment: {}", align);
            }

            malloc(size)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            free(ptr);
        }

        // TODO: realloc might make sense with alignment <= 8.
    }

    // Define locally, as this is experimental.
    #[allow(non_camel_case_types)]
    pub type c_size_t = usize;
}
