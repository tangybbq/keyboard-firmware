//! Firmware updates through MCUboot, over minder.
//!
//! A keyboard MCUboot boots (the mesa3w) says where its update slot is with
//! [`Request::ImageInfo`].  An update is an MCUboot image signed for that slot, padded to its
//! size, with its trailer marked pending: jolt's build makes one, `zephyr.update.bin`.
//! Uploading is writing it into the slot with `Program`, a page at a time, and checking it with
//! `Hash`; nothing else needs saying, as the trailer is what marks it pending.  At the next
//! reset MCUboot swaps it in, on trial, and the firmware confirms it once a host connects.
//!
//! Pages already right are skipped, so an interrupted upload picks up where it stopped.  The
//! slot's last page, which holds the trailer, is erased before anything else is written and
//! written after everything else, so that an upload cut off part way never looks pending.

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use minder::{cap, Reply, Request};
use sha2::{Digest, Sha256};

use crate::{FlashImage, Flasher};

/// MCUboot's image header magic, at the start of an image.
const IMAGE_MAGIC: u32 = 0x96f3_b83d;

/// MCUboot's trailer magic, the last 16 bytes of a slot whose image is pending.
const BOOT_MAGIC: [u8; 16] = [
    0x77, 0xc2, 0x95, 0xf3, 0x60, 0xd2, 0xef, 0x7f, 0x35, 0x52, 0x50, 0x0f, 0x2c, 0xb6, 0x79, 0x80,
];

/// The size of a page, which `Program` erases and writes whole.
const PAGE: usize = 4096;

/// An update image, read from a file.
pub struct UpdateImage {
    pub data: Vec<u8>,
    /// The version in its header, `major.minor.revision+build`.
    pub version: String,
}

impl UpdateImage {
    /// Read an update image, and check that it is one: an MCUboot header at the start, and the
    /// trailer's magic at the end, which only a padded image has.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let data = std::fs::read(path)
            .map_err(|e| anyhow!("cannot read {}: {}", path.display(), e))?;
        Self::from_bytes(data).map_err(|e| anyhow!("{}: {}", path.display(), e))
    }

    /// Check an update image's bytes; see [`load`](Self::load).
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        if data.len() < 32 || u32::from_le_bytes(data[0..4].try_into()?) != IMAGE_MAGIC {
            bail!("not an MCUboot image (no header magic)");
        }
        if !data.ends_with(&BOOT_MAGIC) {
            bail!(
                "not padded with a pending trailer; upload zephyr.update.bin, not \
                 zephyr.signed.bin"
            );
        }
        let version = format!(
            "{}.{}.{}+{}",
            data[20],
            data[21],
            u16::from_le_bytes(data[22..24].try_into()?),
            u32::from_le_bytes(data[24..28].try_into()?),
        );
        Ok(Self { data, version })
    }
}

/// What [`Request::ImageInfo`] says.
#[derive(Debug)]
pub struct ImageInfo {
    pub slot_offset: u32,
    pub slot_size: u32,
    pub version: String,
    pub confirmed: bool,
}

impl Flasher {
    /// Greet the device, and check that it takes updates.  Returns its serial number (the
    /// `info` of its `Hello`), which finds this keyboard again after the reset.
    pub fn check_image_support(&mut self) -> Result<String> {
        let hello: Reply = self.minder.call(&Request::Hello {
            version: minder::VERSION.to_string(),
        })?;
        if !hello.supports(cap::IMAGE) {
            bail!(
                "the keyboard does not take firmware updates (no {:?} capability): {:?}",
                cap::IMAGE,
                hello
            );
        }
        match hello {
            Reply::Hello { info, .. } => Ok(info),
            _ => unreachable!("supports() is only true of a Hello"),
        }
    }

    /// Where the update slot is, and what is running.
    pub fn image_info(&mut self) -> Result<ImageInfo> {
        match self.minder.call(&Request::ImageInfo)? {
            Reply::ImageInfo {
                slot_offset,
                slot_size,
                version,
                confirmed,
            } => Ok(ImageInfo {
                slot_offset,
                slot_size,
                version,
                confirmed,
            }),
            reply => Err(anyhow!("unexpected reply to ImageInfo: {:?}", reply)),
        }
    }

    /// Erase a page, writing nothing.
    pub fn erase(&mut self, offset: u32) -> Result<()> {
        self.write(&[], offset)
    }

    /// Write `image` into the slot `info` names, leaving it pending.  Prints its progress.
    pub fn upload(&mut self, info: &ImageInfo, image: &UpdateImage) -> Result<()> {
        if image.data.len() != info.slot_size as usize {
            bail!(
                "the image is {} bytes, but the keyboard's slot is {}; it was built for \
                 another layout",
                image.data.len(),
                info.slot_size
            );
        }

        let flash = FlashImage {
            data: image.data.clone(),
            offset: info.slot_offset,
        };
        let updates = self.check(&flash)?;
        if !updates.is_empty() {
            let last = info.slot_offset + info.slot_size - PAGE as u32;
            // Whatever trailer the slot has now goes first, so that only the finished upload
            // can look pending.
            self.erase(last)?;
            let pages: Vec<_> = updates.iter().filter(|u| u.offset != last).collect();
            let total = pages.len() + 1;
            for (i, update) in pages.iter().enumerate() {
                print!("Writing [{}/{}]\r", i + 1, total);
                let _ = std::io::Write::flush(&mut std::io::stdout());
                if update.data.iter().all(|&b| b == 0xff) {
                    self.erase(update.offset)?;
                } else {
                    self.write(update.data, update.offset)?;
                }
            }
            println!("Writing [{}/{}] (the trailer)", total, total);
            let start = (last - info.slot_offset) as usize;
            self.write(&image.data[start..], last)?;
        }

        let digest: [u8; 32] = Sha256::digest(&image.data).into();
        if self.hash(info.slot_offset, info.slot_size)? != digest {
            bail!("the slot does not hash to the image after writing it");
        }
        println!("Slot 1 holds {}, pending", image.version);
        Ok(())
    }
}

/// Wait for a keyboard to come back after a reset, and say what it is running.
///
/// MCUboot swaps the images before the firmware starts, which takes a while, and over
/// Bluetooth the host has to reconnect too, so this keeps trying for `limit`.  A link found
/// while the keyboard is still going down fails its request, and is tried again.
pub fn wait_for_keyboard(serial: &str, limit: Duration) -> Result<ImageInfo> {
    let start = Instant::now();
    sleep(Duration::from_secs(5));
    loop {
        let attempt = Flasher::new(serial).and_then(|mut flasher| {
            let mut info = flasher.image_info()?;
            // The firmware confirms once a host has a route to it, which is around now; give
            // it a moment rather than report a state about to change.
            for _ in 0..10 {
                if info.confirmed {
                    break;
                }
                sleep(Duration::from_secs(1));
                info = flasher.image_info()?;
            }
            Ok(info)
        });
        match attempt {
            Ok(info) => return Ok(info),
            Err(err) if start.elapsed() < limit => {
                println!("  not back yet: {}", err);
                sleep(Duration::from_secs(3));
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An image: the header magic and a version, padding, and optionally the trailer magic.
    fn image(trailer: bool) -> Vec<u8> {
        let mut data = vec![0xff; 2 * PAGE];
        data[0..4].copy_from_slice(&IMAGE_MAGIC.to_le_bytes());
        data[20] = 1;
        data[21] = 2;
        data[22..24].copy_from_slice(&3u16.to_le_bytes());
        data[24..28].copy_from_slice(&4u32.to_le_bytes());
        if trailer {
            let end = data.len();
            data[end - 16..].copy_from_slice(&BOOT_MAGIC);
        }
        data
    }

    #[test]
    fn test_update_image_version() {
        let image = UpdateImage::from_bytes(image(true)).unwrap();
        assert_eq!(image.version, "1.2.3+4");
    }

    /// The build's zephyr.signed.bin has no trailer; uploading it would install nothing.
    #[test]
    fn test_update_image_needs_trailer() {
        assert!(UpdateImage::from_bytes(image(false)).is_err());
    }

    #[test]
    fn test_update_image_needs_header() {
        let mut data = image(true);
        data[0] = 0;
        assert!(UpdateImage::from_bytes(data).is_err());
    }
}
