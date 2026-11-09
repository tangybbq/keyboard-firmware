//! Confirming the running image to MCUboot, on a board it boots (`image.c`).
//!
//! An update runs on trial, and MCUboot puts the old image back at the next reset unless it is
//! confirmed first.  The proof that an image is good enough to keep is that a host can reach it,
//! over USB or Bluetooth, since that is how the next update would come: so the first time a route
//! for reports arrives (`hid::sender_task`), the image is confirmed.
//!
//! It is also minder's way to slot 1, where an update is written, and to the running image's
//! version (`minder::Minder`'s `Hash`, `Program` and `ImageInfo`).  On a board MCUboot does not
//! boot there is no slot, and these say so.

extern crate alloc;

#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
use alloc::format;
use alloc::string::String;
#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
use core::sync::atomic::{AtomicBool, Ordering};

/// `struct jolt_image_info` in `image.c`.
#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
#[repr(C)]
#[derive(Default)]
struct RawInfo {
    slot_addr: u32,
    slot_size: u32,
    major: u8,
    minor: u8,
    revision: u16,
    build: u32,
    confirmed: bool,
}

#[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
unsafe extern "C" {
    fn jolt_image_confirm();
    fn jolt_image_info(info: *mut RawInfo) -> i32;
    fn jolt_slot_region(addr: *mut u32, size: *mut u32);
    fn jolt_slot_read(offset: u32, buf: *mut u8, len: usize) -> i32;
    fn jolt_slot_program(offset: u32, data: *const u8, len: usize, erase_size: usize) -> i32;
}

/// The running image and where an update goes, for minder's `ImageInfo`.
pub struct Info {
    /// Slot 1's start, as minder's `Hash` and `Program` address it, and its size.
    pub slot: (u32, u32),
    /// The running image's version, `major.minor.revision+build`.
    pub version: String,
    /// Whether the running image is confirmed, rather than on trial.
    pub confirmed: bool,
}

/// Slot 1, as minder addresses it, and its size; `None` on a board MCUboot does not boot.
pub fn slot() -> Option<(u32, u32)> {
    #[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
    {
        let (mut addr, mut size) = (0, 0);
        // SAFETY: both pointers are to locals, written before the call returns.
        unsafe { jolt_slot_region(&mut addr, &mut size) };
        Some((addr, size))
    }
    #[cfg(not(CONFIG_MCUBOOT_IMG_MANAGER))]
    None
}

/// The running image's version and state, and where an update goes; an error number if the
/// running image's header cannot be read, or `None` on a board MCUboot does not boot.
pub fn info() -> Option<Result<Info, i32>> {
    #[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
    {
        let mut raw = RawInfo::default();
        // SAFETY: `raw` has `struct jolt_image_info`'s layout, and is written before the return.
        let err = unsafe { jolt_image_info(&mut raw) };
        if err != 0 {
            return Some(Err(err));
        }
        Some(Ok(Info {
            slot: (raw.slot_addr, raw.slot_size),
            version: format!("{}.{}.{}+{}", raw.major, raw.minor, raw.revision, raw.build),
            confirmed: raw.confirmed,
        }))
    }
    #[cfg(not(CONFIG_MCUBOOT_IMG_MANAGER))]
    None
}

/// Read slot 1 at `offset` into it, filling `buf`.
pub fn read(offset: u32, buf: &mut [u8]) -> Result<(), i32> {
    #[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
    {
        // SAFETY: `buf` is writable for its whole length, which is what is passed.
        match unsafe { jolt_slot_read(offset, buf.as_mut_ptr(), buf.len()) } {
            0 => Ok(()),
            err => Err(err),
        }
    }
    #[cfg(not(CONFIG_MCUBOOT_IMG_MANAGER))]
    {
        let _ = (offset, buf);
        Err(-19) // -ENODEV
    }
}

/// Erase `erase_size` bytes of slot 1 at `offset` into it, and write `data` at the start.
pub fn program(offset: u32, data: &[u8], erase_size: usize) -> Result<(), i32> {
    #[cfg(CONFIG_MCUBOOT_IMG_MANAGER)]
    {
        // SAFETY: `data` is readable for its whole length, which is what is passed.
        match unsafe { jolt_slot_program(offset, data.as_ptr(), data.len(), erase_size) } {
            0 => Ok(()),
            err => Err(err),
        }
    }
    #[cfg(not(CONFIG_MCUBOOT_IMG_MANAGER))]
    {
        let _ = (offset, data, erase_size);
        Err(-19) // -ENODEV
    }
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
