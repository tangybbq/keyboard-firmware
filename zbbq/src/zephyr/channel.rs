//! Channels for Zephyr.
//!
//! Unlike Mutex and Condvar, channels on Zephyr are more restrictive.  These
//! are inspired by the crossbeam channels, sub simplified, have a const
//! initialization (from a raw underlying mutex/condvar), and currently are
//! shared globally.
