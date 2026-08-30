//! The host side of the minder protocol.
//!
//! This is the transport and the flashing logic, split out of the CLI so that anything else
//! wanting to talk to a keyboard -- the collector in `taipo-teacher.md` phase 3, or a test
//! harness -- does not have to go through `keyminder`'s argument parsing.
//!
//! The protocol types themselves live in the `minder` crate; this crate is only how they get
//! on and off the wire.
//!
//! # Ordering
//!
//! The device owes exactly one reply per request, in order, on a single bulk IN pipe.  That is
//! what lets [`VendorMinder::send`] and [`VendorMinder::recv`] be used separately -- a
//! `GetEvent` long poll can be left outstanding and another request sent past it, and the
//! `NoEvent` the poll is owed still arrives first.  See `Minder::main_loop` in the firmware.

use std::{io::Write, path::Path, time::Duration};

use anyhow::Result;
use minder::{cap, Event, Reply, Request, PACKET_SIZE};
use minicbor::{Decode, Encode};
use rusb::{DeviceHandle, Direction, GlobalContext};
use sha2::{Digest, Sha256};

/// A single page to update.
pub struct Update<'a> {
    pub data: &'a [u8],
    pub offset: u32,
}

/// An image to be loaded into flash at a given offset.
pub struct FlashImage {
    pub data: Vec<u8>,
    pub offset: u32,
}

impl FlashImage {
    pub fn load<P: AsRef<Path>>(name: P, offset: u32) -> Result<Self> {
        let data = std::fs::read(name)?;
        Ok(Self { data, offset })
    }
}

pub struct Flasher {
    minder: VendorMinder,
}

impl Flasher {
    pub fn new(serial: &str) -> Result<Self> {
        let mut minder = VendorMinder::new(serial)?;
        minder.drain()?;
        Ok(Self { minder })
    }

    /// Ask the device to reset itself.
    #[allow(dead_code)]
    pub fn reset(&mut self) -> Result<()> {
        let reply: Reply = self.minder.call(&Request::Reset)?;
        println!("Reset: {:?}", reply);

        Ok(())
    }

    /// Hash a region of the flash on the device.
    pub fn hash(&mut self, offset: u32, size: u32) -> Result<[u8; 32]> {
        let reply: Reply = self.minder.call(&Request::Hash { offset, size })?;
        match reply {
            Reply::Hash { hash } => Ok(hash.into()),
            e => Err(anyhow::anyhow!("Error hashing: {:?}", e)),
        }
    }

    /// Work through the image, building a map of what pages need to be updated.
    pub fn check<'a>(&mut self, image: &'a FlashImage) -> Result<Vec<Update<'a>>> {
        let mut offset = 0;
        let length = image.data.len();
        let total_blocks = length.div_ceil(4096);
        let mut block = 0;
        let mut updates = Vec::new();

        // Before getting too far, try hashing the entire image to see if anything needs to be done.
        let mut digest = Sha256::new();
        digest.update(&image.data);
        let digest: [u8; 32] = digest.finalize().into();

        let thash = self.hash(image.offset, image.data.len() as u32)?;
        if digest == thash {
            println!("Image is up to date");
            return Ok(Vec::new());
        }

        while offset < length {
            let count = (image.data.len() - offset).min(4096);
            let slice = &image.data[offset..offset + count];

            print!("[{:4}/{:4}] {} dirty\r", block, total_blocks, updates.len());
            let _ = std::io::stdout().flush();

            let mut digest = Sha256::new();
            digest.update(slice);
            let digest: [u8; 32] = digest.finalize().into();

            // Ask the target to hash this.
            let thash = self.hash(offset as u32 + image.offset, count as u32)?;

            if digest != thash {
                // println!("differs: {:#08x} {:#04x}", offset + image.offset as usize, count);
                updates.push(Update {
                    data: slice,
                    offset: offset as u32 + image.offset,
                });
            }

            offset += count;
            block += 1;
        }

        println!("");

        Ok(updates)
    }

    pub fn write(&mut self, data: &[u8], offset: u32) -> Result<()> {
        let data = data.to_vec();
        match self.minder.call(&Request::Program {
            data: data.into(),
            offset,
        }) {
            Ok(Reply::ProgramDone) => Ok(()),
            Ok(rep) => {
                return Err(anyhow::anyhow!("Erronous reply: {:?}", rep));
            }
            Err(e) => Err(e)?,
        }
    }
}

pub struct VendorMinder {
    /// The handle of the keyboard we a talking to.
    handle: DeviceHandle<GlobalContext>,

    /// The receive buffer.  Zero-copy structs will reference directly into this.
    rbuf: Vec<u8>,

    /// Endpoints to use.
    send: u8,
    recv: u8,

    /// How long to wait for a reply.  The long poll needs more than the default.
    pub read_timeout: Duration,
}

impl VendorMinder {
    /// Attempt to open the keyboard with the given serial number.
    pub fn new(serial: &str) -> Result<Self> {
        for dev in rusb::devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() != 0xc0de || desc.product_id() != 0xcafe {
                continue;
            }

            // Fetch the serial number.
            let serial_index = desc.serial_number_string_index().unwrap();
            let handle = dev.open()?;
            let dev_serial = handle.read_string_descriptor_ascii(serial_index)?;
            if dev_serial != serial {
                continue;
            }

            // Dig down and get the endpoint descriptors.
            let mut send = None;
            let mut recv = None;
            let conf = dev.active_config_descriptor()?;
            for int in conf.interfaces() {
                for desc in int.descriptors() {
                    if desc.class_code() == 0xff {
                        for endp in desc.endpoint_descriptors() {
                            // println!("end: {:?}", endp);
                            if endp.direction() == Direction::In {
                                recv = Some(endp.address());
                            } else {
                                send = Some(endp.number());
                            }
                        }

                        // Be sure to claim this interface.
                        handle.claim_interface(int.number())?;
                    }
                }
            }

            return Ok(Self {
                handle,
                rbuf: vec![0u8; 532],
                send: send.unwrap(),
                recv: recv.unwrap(),
                read_timeout: Duration::from_secs(15),
            });
        }

        Err(anyhow::anyhow!("Unable to find device with given serial"))
    }

    /// Perform a round trip communication.
    pub fn call<'d, In, Out>(&'d mut self, req: &Out) -> Result<In>
    where
        In: Decode<'d, ()>,
        Out: Encode<()>,
    {
        self.send(req)?;
        self.recv()
    }

    /// Send a request, without waiting for its reply.
    ///
    /// Split out from `call` so the long poll's ordering rule can be exercised: a request sent
    /// while a `GetEvent` is pending is answered after the `NoEvent` that the poll is owed.
    pub fn send<Out>(&mut self, req: &Out) -> Result<()>
    where
        Out: Encode<()>,
    {
        let mut obuf = Vec::new();
        minicbor::encode(req, &mut obuf)?;
        let count = self
            .handle
            .write_bulk(self.send, &obuf, Duration::from_secs(1))?;
        if count != obuf.len() {
            panic!("Short write");
        }

        // The device ends a message at the first packet shorter than 64 bytes, so a message whose
        // length is an exact multiple of that needs a zero-length packet to terminate it.  Without
        // one the device waits for a continuation that never comes, and the request that does
        // arrive next is appended to it and lost.  `Minder::bulk_write` does the same on the way
        // back.
        if obuf.len() % PACKET_SIZE == 0 {
            self.handle
                .write_bulk(self.send, &[], Duration::from_secs(1))?;
        }

        Ok(())
    }

    /// Receive a single reply.
    pub fn recv<'d, In>(&'d mut self) -> Result<In>
    where
        In: Decode<'d, ()>,
    {
        let count = self
            .handle
            .read_bulk(self.recv, &mut self.rbuf, self.read_timeout)?;
        let inbuf = &self.rbuf[..count];

        Ok(minicbor::decode(inbuf)?)
    }

    // Drain any pending data on the bulk endpoint.
    pub fn drain(&mut self) -> Result<()> {
        loop {
            match self
                .handle
                .read_bulk(self.recv, &mut self.rbuf, Duration::from_millis(2))
            {
                Ok(_) => (),
                Err(_) => break,
            }
        }

        Ok(())
    }
}

/// Whether an [`EventPump`] callback wants to keep going.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Flow {
    Continue,
    Stop,
}

/// A connection that owns the long-poll loop.
///
/// The device only speaks when spoken to, so "the device pushed an event" really means
/// "a `GetEvent` was outstanding when the event happened".  Keeping exactly one
/// outstanding is this type's job, so that a caller sees a stream of events rather than a
/// polling protocol.  `taipo-teacher.md` phase 3's collector is the intended user; the
/// `poll` subcommand is the current one.
pub struct EventPump {
    minder: VendorMinder,
    timeout_ms: u32,
}

impl EventPump {
    /// Connect, greet, and check that the device can actually do this.
    ///
    /// The capability check is the point of `Reply::Hello` carrying one.  Firmware
    /// predating the long poll does not recognize `GetEvent` and simply never answers it,
    /// so a host that issued one anyway would hang rather than fail, which is the failure
    /// mode hardest to diagnose from the far end of a USB cable.
    ///
    /// Returns the `Hello` reply as well, since a caller generally wants the boot id and
    /// the layout fingerprint from it.
    pub fn connect(serial: &str, timeout_ms: u32) -> Result<(Self, Reply)> {
        let mut minder = VendorMinder::new(serial)?;
        // A device left mid-conversation by a killed client still owes a reply.
        minder.drain()?;

        let hello: Reply = minder.call(&Request::Hello {
            version: minder::VERSION.to_string(),
        })?;
        if !hello.supports(cap::EVENTS) {
            return Err(anyhow::anyhow!(
                "device does not support the {:?} capability; it needs firmware with the \
                 long poll.  It reported: {:?}",
                cap::EVENTS,
                hello,
            ));
        }

        // The device's poll has to expire before the read does, or every poll looks like a
        // USB timeout.
        minder.read_timeout = Duration::from_millis(timeout_ms as u64) + Duration::from_secs(5);

        Ok((Self { minder, timeout_ms }, hello))
    }

    /// The underlying connection, for requests that are not part of the event stream.
    ///
    /// Safe to use between [`poll_once`](Self::poll_once) calls: the pump never leaves a
    /// request outstanding once it has returned.
    pub fn minder(&mut self) -> &mut VendorMinder {
        &mut self.minder
    }

    /// Wait for one event, or for the device's poll to expire.
    ///
    /// `Ok(None)` is a timeout, which is the normal quiet case and not an error.
    pub fn poll_once(&mut self) -> Result<Option<Event>> {
        let reply: Reply = self.minder.call(&Request::GetEvent {
            timeout_ms: self.timeout_ms,
        })?;
        match reply {
            Reply::Event { event } => Ok(Some(event)),
            Reply::NoEvent => Ok(None),
            other => Err(anyhow::anyhow!("Unexpected reply to GetEvent: {:?}", other)),
        }
    }

    /// Poll until the callback says to stop, or a transport error ends it.
    ///
    /// The callback sees `None` for a poll that expired with nothing to report, so that it
    /// can do periodic work without needing a timer of its own.
    pub fn run<F>(&mut self, mut on_event: F) -> Result<()>
    where
        F: FnMut(Option<Event>) -> Flow,
    {
        loop {
            let event = self.poll_once()?;
            if on_event(event) == Flow::Stop {
                return Ok(());
            }
        }
    }
}

/// The layout fingerprint recorded in the checked-in `layouts.json`.
///
/// Compared against the one a device reports in `Reply::Hello`: they differ when the
/// firmware's chord tables are not the tables this host is reading, which is the case
/// where replaying a key log produces a plausible wrong answer rather than an error.
///
/// Parsed rather than deserialized, because pulling in serde for one hex string in a
/// generated file is not worth it.
pub fn layouts_fingerprint() -> Option<u64> {
    let json = include_str!("../../bbq-keyboard/layouts.json");
    let tail = json.split("\"fingerprint\"").nth(1)?;
    let value = tail.split('"').nth(1)?;
    u64::from_str_radix(value.strip_prefix("0x")?, 16).ok()
}

#[cfg(test)]
mod tests {
    /// The fingerprint really is readable out of the checked-in file.  This fails if the
    /// export changes shape, which is exactly when the hand-rolled parse above would
    /// otherwise start silently returning None and reporting "no fingerprint found".
    #[test]
    fn test_layouts_fingerprint_parses() {
        assert!(super::layouts_fingerprint().is_some());
    }
}
