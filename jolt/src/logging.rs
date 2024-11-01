//! Logging
//!
//! An interface to logging that can log to 'printk' as well as capturing previous log messages and
//! retrieving them over some kind of host interface.

// Let's start by just replicating what the printk logger does.

use alloc::{boxed::Box, collections::vec_deque::VecDeque, format, string::String};
use log::{LevelFilter, Log};
use zephyr::sync::{Arc, Mutex};

const LOG_LIMIT: usize = 20;

/// The logger itself.  This is a shared/locked item, so that logging will require exclusive access
/// to it.
pub struct Logger {
    messages: VecDeque<String>,
}

impl Logger {
    /// Construct a new logger.  The logger will be registered with the logging system (which will
    /// increment the Arc refcount, which will never be freed.
    pub fn new() -> Arc<Mutex<Logger>> {
        let log = Arc::new(Mutex::new(Logger {
            messages: VecDeque::new(),
        }));

        // Wrap one, leak that and register with the log system.
        let wrapped = Box::new(LogWrapper(log.clone()));
        set_logger(Box::leak(wrapped));

        log
    }

    /// Attempt to retrieve a log message.
    pub fn pop(&mut self) -> Option<String> {
        self.messages.pop_front()
    }
}

/// The log wrapper, so that we can add the Log trait.
struct LogWrapper(Arc<Mutex<Logger>>);

impl Log for LogWrapper {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let message = format!("{}:{}: {}",
                              record.level(),
                              record.target(),
                              record.args());

        // TODO: Record dropped messages.

        let mut inner = self.0.lock().unwrap();
        if inner.messages.len() < LOG_LIMIT {
            inner.messages.push_back(message);
        }
    }

    fn flush(&self) {
        // Nothing to do here.
    }
}


#[cfg(target_has_atomic = "ptr")]
fn set_logger<L: Log>(logger: &'static L) {
    log::set_logger(logger).unwrap();
    log::set_max_level(LevelFilter::Info);
}

#[cfg(not(target_has_atomic = "ptr"))]
fn set_logger<L: Log>(logger: &'static L) {
    unsafe {
        log::set_logger_racy(logger).unwrap();
        log::set_max_level_racy(LevelFilter::Info);
    }
}
