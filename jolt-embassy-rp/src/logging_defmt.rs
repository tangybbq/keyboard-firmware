#[allow(unused_imports)]
pub use defmt::{debug, info, panic, unwrap};
use defmt_rtt as _;
use panic_probe as _;

pub fn log_init() {}
