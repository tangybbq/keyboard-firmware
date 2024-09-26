//! Device management
//!
//! Management of the various devices used in the keyboards.  Some are just direct types from
//! Zephyr, and others are wrapped.

use bbq_keyboard::Side;

use zephyr::raw::{GPIO_INPUT, GPIO_PULL_UP};
use zephyr::sys::busy_wait;

/// Get the "side" configuration.  Determines which side we are on based on a GPIO.
pub fn get_side() -> Side {
    let side_select = zephyr::devicetree::side_select::get_gpios();
    let mut side_select = match side_select {
        [pin] => pin,
        // Compile error here means other than a single pin is defined in the DT.
    };

    side_select.configure(GPIO_INPUT | GPIO_PULL_UP);
    busy_wait(5);
    if side_select.get() {
        Side::Left
    } else {
        Side::Right
    }
}

pub fn get_translation() -> fn (u8) -> u8 {
    translate_id
}

fn translate_id(code: u8) -> u8 {
    code
}
