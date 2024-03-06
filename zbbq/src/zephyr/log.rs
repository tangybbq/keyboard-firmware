//! Zephyr logging.

use core::ffi::c_char;

use alloc::ffi::CString;

extern crate alloc;

// For now, logging will just be based on strings that we will do the formatting
// ahead of time.
// TODO: Implement better deferred logging with Zephyr's logger.

#[repr(usize)]
#[allow(dead_code)]
pub enum Level {
    None = 0,
    Err = 1,
    Wrn = 2,
    Inf = 3,
    Dbg = 4,
}

// C helper to log a C string at a given level.
extern "C" {
    fn c_log_message(level: usize, message: *const c_char);
}

/// Log a message at a given level.
pub fn log_message(level: Level, message: &str) {
    let raw = CString::new(message).expect("CString::new failed");
    unsafe {
        c_log_message(level as usize, raw.as_ptr());
    }
}
