//! Device management
//!
//! Management of the various devices used in the keyboards.  Some are just direct types from
//! Zephyr, and others are wrapped.

use core::ffi::c_int;

use bbq_keyboard::Side;

use zephyr::raw::{GPIO_INPUT, GPIO_PULL_DOWN};
use zephyr::sys::busy_wait;

/// Get the "side" configuration.  Determines which side we are on based on a GPIO.
pub fn get_side() -> Side {
    let side_select = zephyr::devicetree::side_select::get_gpios();
    let mut side_select = match side_select {
        [pin] => pin,
        // Compile error here means other than a single pin is defined in the DT.
    };

    side_select.configure(GPIO_INPUT | GPIO_PULL_DOWN);
    busy_wait(5);
    if side_select.get() {
        Side::Right
    } else {
        Side::Left
    }
}

pub fn get_translation() -> fn (u8) -> u8 {
    translate_id
}

fn translate_id(code: u8) -> u8 {
    code
}

pub fn hid_is_accepting() -> bool {
    (unsafe {is_hid_accepting()}) != 0
}

/// Send a single report via HID.
///
/// If `hid_is_accepting()` didn't return true, this might block.
pub fn hid_send_keyboard_report(mods: u8, keys: &[u8]) {
    if keys.len() > 6 {
        // Don't panic, just ignore?
        return;
    }

    let mut report = [0u8; 8];
    report[0] = mods;
    for (i, key) in keys.iter().enumerate() {
        report[i+2] = *key;
    }
    unsafe {hid_report(report.as_ptr())};
}

extern "C" {
    fn is_hid_accepting() -> c_int;
    fn hid_report(report: *const u8);
}
