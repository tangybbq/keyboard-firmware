//! Minder protocol.
//!
//! For the jolt3, USB devices each have an additional vendor-specific interface that defines two
//! bulk endpoints.  The protocol is packetized, per usb-bulk, and the payload itself is CBOR
//! encoded of the Minder protocol.

// The minder packets use alloc, and we use alloc to manage the write buffer.
extern crate alloc;

use alloc::vec::Vec;
use embassy_executor::Spawner;
use embassy_rp::{peripherals::WATCHDOG, watchdog::Watchdog};
use embassy_time::{Duration, Timer};
use embassy_usb::driver::{EndpointError, EndpointIn, EndpointOut};
use minder::{Reply, Request, VERSION};
use sha2::{Digest, Sha256};

#[allow(unused_imports)]
use crate::logging::{info, warn};

pub struct Minder<Rd, Wr>
where
    Rd: EndpointOut,
    Wr: EndpointIn,
{
    reader: Rd,
    writer: Wr,
    unique: &'static str,

    read_buf: [u8; 64],
}

/// Size limit for read. Prevents memory loss from excessive data.
/// Value chosen so cbor data with 4k buffer in it should be fine.
const SIZE_LIMIT: usize = 4200;

impl<Rd: EndpointOut, Wr: EndpointIn> Minder<Rd, Wr> {
    pub fn new(reader: Rd, writer: Wr, unique: &'static str) -> Self {
        Self {
            reader,
            writer,
            unique,
            read_buf: [0; 64],
        }
    }

    /// The main loop, reads requests and replies to them.
    pub async fn main_loop(mut self) -> ! {
        loop {
            let rbuf = match self.bulk_read().await {
                Ok(rbuf) => rbuf,
                Err(err) => {
                    warn!("Minder read error: {:?}", err);
                    continue;
                }
            };

            // info!("Minder read {} bytes", rbuf.len());
            let reply = match minicbor::decode::<Request>(&rbuf) {
                Ok(Request::Hello { version }) => self.hello(&version).await,
                Ok(Request::ReadFlash { offset, size }) => {
                    let _ = (offset, size);
                    todo!();
                }
                Ok(Request::Reset) => self.reset().await,
                Ok(Request::Hash { offset, size }) => self.hash(offset, size),
                Err(_) => {
                    warn!("Error decoding packet");
                    continue;
                }
            };

            // Send the reply back.
            let mut wbuf = Vec::new();
            if minicbor::encode(&reply, &mut wbuf).is_err() {
                warn!("Error encoding packet");
                continue;
            }

            // info!("USB write {} bytes", wbuf.len());
            if let Err(e) = self.bulk_write(&wbuf).await {
                warn!("Error writing to USB: {:?}", e);
            }
        }
    }

    /// Write a packet out, via USB bulk, breaking into individual packets as needed.
    async fn bulk_write(&mut self, packet: &[u8]) -> Result<(), EndpointError> {
        let mut offset = 0;
        let length = packet.len();

        while offset < length {
            let end = (offset + 64).min(length);
            let chunk = &packet[offset..end];
            self.writer.write(chunk).await?;
            offset = end;
        }

        // If the data is a multiple of the packet size, send a zero-byte packet.
        if length % 64 == 0 {
            self.writer.write(&[]).await?;
        }

        Ok(())
    }

    /// Read a full packet, via USB bulk, assembling it back into a proper packet.  Uses the
    /// read-buf for each packet, and places the result into a Vec.
    async fn bulk_read(&mut self) -> Result<Vec<u8>, EndpointError> {
        let mut result = Vec::new();
        let mut warned = false;

        loop {
            let len = self.reader.read(&mut self.read_buf).await?;

            if result.len() + len < SIZE_LIMIT {
                result.extend_from_slice(&self.read_buf[..len]);
            } else {
                if !warned {
                    warn!("Excessively large USB bulk data received, discarding");
                    warned = true;
                }
            }

            if len < 64 {
                break;
            }
        }

        Ok(result)
    }

    /// Given a hello pack, generate our detailed response.
    async fn hello(&mut self, _version: &str) -> Reply {
        // For now, don't worry about protocol versions, and just return ours.
        Reply::Hello {
            version: VERSION.into(),
            info: self.unique.into(),
        }
    }

    /// Trigger a reset shortly after we acknowledge.
    async fn reset(&mut self) -> Reply {
        let spawner = Spawner::for_current_executor().await;

        spawner.spawn(reset_device()).unwrap();

        Reply::Reset
    }

    // TODO: Do this with the flash driver?
    /// Calculate the hash of a region.  Sync.
    fn hash(&self, offset: u32, size: u32) -> Reply {
        // Validate that the offset and size is in a valid region.
        if offset < FLASH_START {
            return Reply::Error { text: "Out of bounds".into() };
        }

        if let Some(end) = offset.checked_add(size) {
            if end > FLASH_START + FLASH_SIZE {
                return Reply::Error { text: "Out of bounds".into() };
            }
        } else {
            return Reply::Error { text: "Out of bounds".into() };
        }

        let mut hasher = Sha256::new();

        let start_addr = offset as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(start_addr, size as usize) };

        // info!("Hashing");
        hasher.update(slice);
        let digest = hasher.finalize();
        // info!("Done Hashing {:x}", digest.as_slice());

        let digest: [u8; 32] = digest.into();
        Reply::Hash { hash: digest.into() }
    }
}

/// The start address of valid flash.
const FLASH_START: u32 = 0x10100000;

/// The size of the valid flash.
const FLASH_SIZE: u32 = 8 * 1024 * 1024 - 0x100000;

/// Reset the device.  This delays a small amount, and then uses the watchdog hardware to reset the
/// device.
#[embassy_executor::task]
async fn reset_device() {
    Timer::after(Duration::from_millis(500)).await;

    let mut dog = Watchdog::new(unsafe { WATCHDOG::steal() });
    dog.trigger_reset();
}
