//! Sync for Zephyr
//!
//! Implementation of 'sync' for Zephyr.
//!
//! This module attempts to provide 'std::sync' functionality on top of Zephyr.
//! There are few caveats.
//!
//! The underlying primitives in Zephyr can be allocated statically and given to
//! the initialize (which is unsafe).
//!
//! ...

use core::{ffi::c_int, cell::UnsafeCell, ops::{Deref, DerefMut}};

use super::timer::{struct_k_timeout, K_FOREVER};

/// A mutual exclusion primitive useful for protecting shared data.
pub struct Mutex<T: ?Sized> {
    inner: *mut k_mutex,
    // todo: poison
    data: UnsafeCell<T>,
}

// Implement the attributes that make the Mutex useful shared.
unsafe impl<T: ?Sized + Send> Send for Mutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for Mutex<T> {}

#[must_use = "if unused the Mutex will immediately unlock"]
// #[must_not_suspend = "holding a MutexGuard across suspend \
//                       points can cause deadlocks, delays, \
//                       and cause Futures to not implement `Send`"]
pub struct MutexGuard<'a, T: ?Sized + 'a> {
    lock: &'a Mutex<T>,
    // todo: poison
}

// not stable
// impl<T: ?Sized> !Send for MutexGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for MutexGuard<'_, T> {}

impl<T> Mutex<T> {
    pub const unsafe fn new_raw(inner: *mut k_mutex, t: T) -> Mutex<T> {
        Mutex {
            inner,
            data: UnsafeCell::new(t),
        }
    }
}

impl<T: ?Sized> Mutex<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        unsafe {
            match sys_mutex_lock(self.inner, K_FOREVER) {
                0 => (),
                _ => panic!("Error locking mutex"),
            }
            MutexGuard::new(self)
        }
    }
}

impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
    unsafe fn new(lock: &'mutex Mutex<T>) -> MutexGuard<'mutex, T> {
        MutexGuard { lock }
    }
}

impl<T: ?Sized> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for MutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            match sys_mutex_unlock(self.lock.inner) {
                0 => (),
                _ => panic!("Unlock error"),
            }
        }
    }
}

// TODO timeouts.

pub struct Condvar {
    inner: *mut k_condvar,
}

unsafe impl Send for Condvar {}
unsafe impl Sync for Condvar {}

impl Condvar {
    pub const unsafe fn new_raw(inner: *mut k_condvar) -> Condvar {
        Condvar { inner }
    }

    pub fn wait<'a, T>(&self, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        unsafe {
            match sys_condvar_wait(self.inner, guard.lock.inner, K_FOREVER) {
                0 => (),
                _ => panic!("Lock error"),
            }
        }
        guard
    }

    pub fn notify_one(&self) {
        unsafe {
            match sys_condvar_signal(self.inner) {
                0 => (),
                _ => panic!("Condvar signal error"),
            }
        }
    }

    #[allow(dead_code)]
    pub fn notify_all(&self) {
        unsafe {
            match sys_condvar_broadcast(self.inner) {
                0 => (),
                _ => panic!("Condvar broadcast error"),
            }
        }
    }
}

// Zephyr primitives.
// To start, only support externally provided mutexes.  We don't have to worry
// about how to allocate them, and pin them.
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct k_mutex {
    // Size is not yet determined.
    _pad: u32,
}

#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct k_condvar {
    _pad: u32,
}

extern "C" {
    fn sys_mutex_lock(mutex: *mut k_mutex, timeout: struct_k_timeout) -> c_int;
    fn sys_mutex_unlock(mutex: *mut k_mutex) -> c_int;
    fn sys_condvar_signal(condvar: *mut k_condvar) -> c_int;
    #[allow(dead_code)]
    fn sys_condvar_broadcast(condvar: *mut k_condvar) -> c_int;
    fn sys_condvar_wait(
        condvar: *mut k_condvar,
        mutex: *mut k_mutex,
        timeout: struct_k_timeout)
        -> c_int;
}
