#![no_std]

use core::ffi::c_char;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
fn rust_main() {
    unsafe {
        msg("Hello from Rust\n\0".as_ptr().cast());
    }
}

extern "C" {
    fn msg(msg: *const c_char);
}
