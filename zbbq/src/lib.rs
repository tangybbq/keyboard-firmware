#![no_std]

use zephyr::log::{log_message, Level};

mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    log_message(Level::Inf, "This is a basic message");
}
