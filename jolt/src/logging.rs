//! Logging
//!
//! An interface to logging that can log to 'printk' as well as capturing previous log messages and
//! retrieving them over some kind of host interface.

// Let's start by just replicating what the printk logger does.

use alloc::{boxed::Box, collections::vec_deque::VecDeque, format, string::String};
use log::{LevelFilter, Log};
use zephyr::sync::{Arc, Mutex};

/// The maximum number of messages that will be queued, whether read or not.
const LOG_LIMIT: usize = 20;

/// The expected number of readers.
const NUM_READERS: usize = 2;

/// The logger itself.  This is a shared/locked item, so that logging will require exclusive access
/// to it.
#[derive(Debug)]
pub struct Logger {
    /// The messages queued to print.
    messages: VecDeque<String>,
    /// The position each reader is at.  Zero indicates the "front" of the Deque.  When all readers
    /// are greater than 0, the front can be popped (and all readers adjusted).
    readers: [usize; NUM_READERS],
    /// Incremented when a reader drops messages.
    drops: [usize; NUM_READERS],
}

impl Logger {
    /// Construct a new logger.  The logger will be registered with the logging system (which will
    /// increment the Arc refcount, which will never be freed.
    pub fn new() -> Arc<Mutex<Logger>> {
        let log = Arc::new(Mutex::new(Logger {
            messages: VecDeque::new(),
            readers: [0, NUM_READERS],
            drops: [0, NUM_READERS],
        }));

        // Wrap one, leak that and register with the log system.
        let wrapped = Box::new(LogWrapper(log.clone()));
        set_logger(Box::leak(wrapped));

        log
    }

    /// Attempt to retrieve a log message.
    pub fn pop(&mut self, reader: usize) -> Option<String> {
        // If this reader has been dropping, return a message indicating that.
        let count = self.drops[reader];
        if count > 0 {
            self.drops[reader] = 0;
            return Some(format!("[{} messages dropped]", count));
        }

        // TODO: if we are going to drop, we could just clone.
        if let Some(msg) = self.messages.get(self.readers[reader]) {
            let msg = msg.clone();
            self.readers[reader] += 1;
            if self.readers.iter().all(|&r| r > 0) {
                self.drop_message();
            }
            Some(msg)
        } else {
            None
        }
    }

    /// Drop a message, adjusting the readers.
    fn drop_message(&mut self) {
        // Pop, and return if there were no messages.
        if self.messages.pop_front().is_none() {
            return;
        }
        for (i, r) in self.readers.iter_mut().enumerate() {
            if *r > 0 {
                *r -= 1;
            } else {
                self.drops[i] += 1;
            }
        }
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
        inner.messages.push_back(message);

        if inner.messages.len() > LOG_LIMIT {
            inner.drop_message();
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
