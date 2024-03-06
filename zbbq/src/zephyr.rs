//! Zephyr binding and interfaces.

// Eventually, this code will become part of a real Zephyr crate.  For now,
// these bindings and such as tailored to the needs of kbbq.

use core::panic::PanicInfo;

use self::alloc::ZephyrAllocator;

pub mod log;
mod alloc;
mod timer;

pub use timer::{Timer, struct_timer};

extern "C" {
    fn c_k_panic() -> !;
}

// Install a panic handler that just calls the Zephyr one.
// TODO: Instead of just calling the C one, implement one that logs messages
// appropriately, so that the source of the rust panic can be known.
#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        c_k_panic();
    }
}

// Install the Zephyr libc allocator as the global allocator.
#[global_allocator]
static ZEPHYR_ALLOCATOR: ZephyrAllocator = ZephyrAllocator;
