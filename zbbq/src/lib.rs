#![no_std]

use core::ffi::c_char;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        zephyr_sys::c_k_panic();
    }
}

#[no_mangle]
fn rust_main() {
    unsafe {
        msg("Hello from Rust\n\0".as_ptr().cast());
    }

    panic!("Rust panic happened");
}

extern "C" {
    fn msg(msg: *const c_char);
}

mod zephyr_sys {
    extern "C" {
        pub fn c_k_panic() -> !;
    }
}
