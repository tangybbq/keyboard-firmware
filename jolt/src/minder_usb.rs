//! Whole minder messages over the vendor bulk interface in `minder_usb.c`.
//!
//! The C side reassembles a request from its packets and splits a reply into them, so this deals
//! in messages.  Its callbacks come from the USB stack's thread, and only signal.

extern crate alloc;

use alloc::{vec, vec::Vec};
use core::ffi::c_int;

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use zephyr::{
    raw::{EBUSY, ENODEV},
    sync::atomic::{AtomicBool, Ordering},
};

unsafe extern "C" {
    fn minder_usb_read_start() -> c_int;
    fn minder_usb_read_copy(dst: *mut u8, cap: usize) -> usize;
    fn minder_usb_write(src: *const u8, len: usize) -> c_int;
}

/// A failed transfer, as the negative errno from the USB stack.
pub struct Error(c_int);

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "errno {}", -self.0)
    }
}

/// Whether the host has configured the interface.
static READY: AtomicBool = AtomicBool::new(false);
static READY_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// A request's length, or a negative errno, from `minder_usb_read_callback`.
static READ_DONE: Signal<CriticalSectionRawMutex, c_int> = Signal::new();

/// A reply's result, from `minder_usb_write_callback`.
static WRITE_DONE: Signal<CriticalSectionRawMutex, c_int> = Signal::new();

async fn wait_ready() {
    while !READY.load(Ordering::Acquire) {
        READY_CHANGED.wait().await;
    }
}

/// Receive the next request.
///
/// Cancel safe, which `GetEvent` relies on: a read cancelled after it started stays queued in the
/// C side, and its result waits in [`READ_DONE`] for the next call, which finds the read already
/// under way rather than starting another.  Nothing is lost between the result arriving and the
/// copy, as there is no await between them.
pub async fn read() -> Result<Vec<u8>, Error> {
    loop {
        // SAFETY: no arguments, and minder_usb.c guards its own state.
        match unsafe { minder_usb_read_start() } {
            0 => (),
            // A read is already under way, or done and waiting to be copied.
            err if err == -(EBUSY as c_int) => (),
            err if err == -(ENODEV as c_int) => {
                wait_ready().await;
                continue;
            }
            err => return Err(Error(err)),
        }

        let result = READ_DONE.wait().await;
        if result < 0 {
            return Err(Error(result));
        }

        let mut message = vec![0; result as usize];
        // SAFETY: `message` is writable for its whole length, which is what is passed.
        let len = unsafe { minder_usb_read_copy(message.as_mut_ptr(), message.len()) };
        message.truncate(len);
        return Ok(message);
    }
}

/// Send a reply, and wait for it to have gone.
pub async fn write(message: &[u8]) -> Result<(), Error> {
    WRITE_DONE.reset();
    // SAFETY: `message` is readable for its whole length, and minder_usb.c copies it before
    // returning.
    let ret = unsafe { minder_usb_write(message.as_ptr(), message.len()) };
    if ret != 0 {
        return Err(Error(ret));
    }

    match WRITE_DONE.wait().await {
        err if err < 0 => Err(Error(err)),
        _ => Ok(()),
    }
}

#[unsafe(no_mangle)]
extern "C" fn minder_usb_ready_callback(ready: bool) {
    READY.store(ready, Ordering::Release);
    READY_CHANGED.signal(());
}

#[unsafe(no_mangle)]
extern "C" fn minder_usb_read_callback(result: c_int) {
    READ_DONE.signal(result);
}

#[unsafe(no_mangle)]
extern "C" fn minder_usb_write_callback(result: c_int) {
    WRITE_DONE.signal(result);
}
