//! Zephyr logging.

use core::ffi::c_char;

use alloc::ffi::CString;

extern crate alloc;

// TODO: Make this a handler for the Rust "log" crate.  Not sure this is ideal,
// or too much overhead.

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

// TODO: This requires the context where this is used to have an `extern crate
// alloc`.  Can we somehow refer to this directly?
/// Log at a given level.
#[macro_export]
macro_rules! log {
    ($lvl:expr, $($arg:tt)+) => {
        {
            let message = alloc::format!($($arg)+);
            $crate::zephyr::log::log_message($lvl, &message);
        }
    };
}

/// Log an error message.
#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => ($crate::log!($crate::zephyr::log::Level::Err, $($arg)+))
}

/// Log a warning message.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => ($crate::log!($crate::zephyr::log::Level::Wrn, $($arg)+))
}

/// Log an Info message.
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => ($crate::log!($crate::zephyr::log::Level::Inf, $($arg)+))
}

/// Log a Debug message.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => ($crate::log!($crate::zephyr::log::Level::Dbg, $($arg)+))
}
