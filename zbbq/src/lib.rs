#![no_std]

extern crate alloc;

mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    error!("This is a basic message");
    warn!("This is warning {}", 42);
    info!("Informative: {:?}", (42, "Message"));
    debug!("Debug message");
}
