use embassy_time::Instant;
// For now, just use panic halt, as panic-probe still wants to use defmt.
pub use log::{debug, info};
use panic_halt as _;
use systemview_target::SystemView;
rtos_trace::global_trace!(SystemView);

pub use core::panic;

static LOGGER: SystemView = SystemView::new();

macro_rules! unwrap {
    ($expr:expr) => {
        $expr.unwrap()
    };
}
pub(crate) use unwrap;

// This needs a timestamp, which annoyingly just uses the lowest 32 bits.
#[no_mangle]
extern "C" fn systemview_get_timestamp() -> u32 {
    Instant::now().as_ticks() as u32
}

pub fn log_init() {
    LOGGER.init();
    // SAFETY: Log doesn't use portable atomic, so detects the target doesn't have atomics.  The
    // racy is provided that we can safely initialize early like this.
    unsafe {
        log::set_logger_racy(&LOGGER).ok();
    }
    // log::set_logger(&LOGGER).ok();
    // log::set_max_level(log::LevelFilter::Info);
}
