//! Whole minder messages over the Bluetooth service in `minder_ble.c`.
//!
//! As with [`crate::minder_usb`], the C side reassembles a request from its packets and splits a
//! reply into them, so this deals in messages.  Its callbacks come from the Bluetooth stack's
//! threads, and only signal.

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::ffi::c_int;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

unsafe extern "C" {
    fn minder_ble_read_copy(dst: *mut u8, cap: usize) -> usize;
    fn minder_ble_write(src: *const u8, len: usize) -> c_int;
}

/// A failed reply, as a negative errno: no host connected and subscribed, an ATT MTU too small
/// for the packets, or the host gone before the reply was.
pub struct Error(c_int);

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "errno {}", -self.0)
    }
}

/// A whole request's length, from `minder_ble_read_callback`.
static READ_DONE: Signal<CriticalSectionRawMutex, usize> = Signal::new();

/// A reply's result, from `minder_ble_write_callback`.
static WRITE_DONE: Signal<CriticalSectionRawMutex, c_int> = Signal::new();

/// Receive the next request.
///
/// Cancel safe, which `GetEvent` relies on: the C side holds a whole request until it is copied,
/// and its length waits in [`READ_DONE`] for the next call.  Nothing is lost between the two, as
/// there is no await between them.
pub async fn read() -> Vec<u8> {
    let len = READ_DONE.wait().await;
    let mut message = vec![0; len];
    // SAFETY: `message` is writable for its whole length, which is what is passed.
    let len = unsafe { minder_ble_read_copy(message.as_mut_ptr(), message.len()) };
    message.truncate(len);
    message
}

/// Send a reply, and wait for it to have gone.
pub async fn write(message: &[u8]) -> Result<(), Error> {
    WRITE_DONE.reset();
    // SAFETY: `message` is readable for its whole length, and minder_ble.c copies it before
    // returning.
    let ret = unsafe { minder_ble_write(message.as_ptr(), message.len()) };
    if ret != 0 {
        return Err(Error(ret));
    }

    match WRITE_DONE.wait().await {
        err if err < 0 => Err(Error(err)),
        _ => Ok(()),
    }
}

#[unsafe(no_mangle)]
extern "C" fn minder_ble_read_callback(len: usize) {
    READ_DONE.signal(len);
}

#[unsafe(no_mangle)]
extern "C" fn minder_ble_write_callback(result: c_int) {
    WRITE_DONE.signal(result);
}
