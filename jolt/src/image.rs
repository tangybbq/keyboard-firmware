//! Confirming the running image to MCUboot, on a board it boots (`image.c`).
//!
//! An update runs on trial, and MCUboot puts the old image back at the next reset unless it is
//! confirmed first.  The proof that an image is good enough to keep is that a host can reach it,
//! over USB or Bluetooth, since that is how the next update would come: so the first time a route
//! for reports arrives (`hid::sender_task`), the image is confirmed.

#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
unsafe extern "C" {
    fn jolt_image_confirm();
}

/// A host can reach the keyboard: confirm the image, the first time.  Nothing on a board MCUboot
/// does not boot.
pub fn host_reached() {
    #[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
    {
        static DONE: AtomicBool = AtomicBool::new(false);
        if !DONE.swap(true, Ordering::AcqRel) {
            // SAFETY: no preconditions; it reads, and at most once writes, slot 0's trailer.
            unsafe { jolt_image_confirm() }
        }
    }
}
