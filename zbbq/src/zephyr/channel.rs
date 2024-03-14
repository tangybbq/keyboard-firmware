//! Channels for Zephyr.
//!
//! Unlike Mutex and Condvar, channels on Zephyr are more restrictive.  These
//! are inspired by the crossbeam channels, sub simplified, have a const
//! initialization (from a raw underlying mutex/condvar), and currently are
//! shared globally.

use core::cell::RefCell;

use arraydeque::ArrayDeque;

use super::sync::{Mutex, k_mutex, k_condvar, Condvar};

pub struct Channel<T, const N: usize> {
    lock: Mutex<RefCell<ArrayDeque<T, N>>>,
    condvar: Condvar,
}

pub struct SendError<T>(pub T);
pub enum TryRecvError {
    Empty,
    Disconnected,
}
pub struct RecvError;

impl<T, const N: usize> Channel<T, N> {
    pub fn new(mutex: *mut k_mutex, condvar: *mut k_condvar) -> Channel<T, N> {
        unsafe {
            Channel {
                lock: Mutex::new_raw(mutex, RefCell::new(ArrayDeque::new())),
                condvar: Condvar::new_raw(condvar),
            }
        }
    }

    // Try pushing the event, with a failure, if the item can't be removed.
    pub fn try_send(&self, item: T) -> Result<(), SendError<T>> {
        let queue = self.lock.lock();
        let mut queue = queue.borrow_mut();
        match queue.push_back(item) {
            Ok(()) => {
                // TODO: This mandates a single receiver, which we don't enforce
                // yet.
                self.condvar.notify_one();
                Ok(())
            },
            Err(e) => Err(SendError(e.element)),
        }
    }

    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let queue = self.lock.lock();
        let mut queue = queue.borrow_mut();
        match queue.pop_front() {
            Some(item) => Ok(item),
            None => Err(TryRecvError::Empty),
        }
    }

    pub fn recv(&self) -> Result<T, RecvError> {
        let mut queue = self.lock.lock();
        loop {
            if let Some(item) = queue.borrow_mut().pop_front() {
                return Ok(item);
            }
            queue = self.condvar.wait(queue);
        }
    }
}
