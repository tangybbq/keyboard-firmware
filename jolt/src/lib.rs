#![no_std]

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    zephyr::printkln!("Helo world");
    zephyr::printkln!("This is a second line");
}
