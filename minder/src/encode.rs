//! The minder protocol
//!
//! Minder is a custom protocol for configuring and controlling USB (and BLE) steno keyboards.
//!
//! As such, it is built around representing the payload in as close to 64-byte packets as possible,
//! with the best that HID can do is use 1 frame per milisecond.
//!
//! HID and similar protocols are based around the idea of reports, which can be sent in either
//! direction.  The device can generate these on its own.  This mirrors the "input" aspect of HID
//! that it is commonly used for.
//!
//! We could enable certain reports automatically, but as HID doesn't have a concept of a
//! connection, the keyboard would receive no notification if the monitoring tool were disconnected.
//! As such, reports will only be generated on-demand, and the protocol will implement a fairly
//! strict request/reply, in the manner of a REST API.  The messages a encoded in a Request, and
//! Reply enum.
//!
//! The encoding used by minicbor is intended to be robust against upgrades.  There is a hello
//! request and reply that can be used to learn various information about the devices, but this
//! shouldn't prevent mismatched versions from being able to communicate.

use core::fmt::Display;

use minicbor::{encode::Write, Encode, Encoder};

use crate::PACKET_SIZE;

/// Encoding needs to be able to write HID packets.  This is done through this trait.
pub trait HidWrite {
    type Error;

    fn write_packet(&mut self, buf: &[u8]) -> Result<(), Self::Error>;
}

impl<W: HidWrite + ?Sized> HidWrite for &mut W {
    type Error = W::Error;

    fn write_packet(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        (**self).write_packet(buf)
    }
}

struct HidEncoder<W> {
    /// Buffer we're writing into.
    buffer: [u8; PACKET_SIZE],
    /// Sequence number for the given packet.
    seq: u8,
    /// Position within the buffer.
    pos: usize,
    /// The underlying writer.
    write: W,
}

impl<W> HidEncoder<W> {
    fn new(write: W) -> Self {
        HidEncoder {
            buffer: [0u8; PACKET_SIZE],
            pos: 1,
            seq: 0,
            write,
        }
    }
}

impl<W: HidWrite> HidEncoder<W>  {
    /// Underlying write, attempts to write as much as it can, filling up the current buffer.
    /// Returns how much was written.
    fn write_partial(&mut self, buf: &[u8]) -> usize {
        let count = buf.len().min(PACKET_SIZE - self.pos);
        self.buffer[self.pos..self.pos+count].copy_from_slice(&buf[..count]);
        self.pos += count;
        count
    }

    /// Flush out the current buffer.  `last` indicates if the is the last packet in the item.
    fn flush(&mut self, last: bool) -> Result<(), W::Error> {
        self.buffer[0] = self.seq | (if last { 0x80 } else { 0x00 });
        self.write.write_packet(&self.buffer[..])?;
        self.buffer.fill(0);

        self.seq += 1;
        self.pos = 1;
        Ok(())
    }
}

impl<W: HidWrite> Write for HidEncoder<W> {
    type Error = W::Error;

    fn write_all(&mut self, mut buf: &[u8]) -> Result<(), Self::Error> {
        while !buf.is_empty() {
            if self.pos == PACKET_SIZE {
                self.flush(false)?;
            }
            let count = self.write_partial(buf);
            buf = &buf[count..];
        }
        Ok(())
    }
}

/// Encode a given item (that implements Encode)
///
/// The item is placed in HID packets, with a first byte indicating the sequence number of this
/// packet.
///
/// The sequence number is 0-127, and the high bit will be set on the last packet of a given
/// sequence.
///
/// This implementation currently allocates a buffer to use for this.
pub fn hid_encode<T: Encode<()>, W: HidWrite>(item: T, write: W) -> Result<(), minicbor::encode::Error<W::Error>>
where
    W::Error: Display,
{
    let mut dest = HidEncoder::new(write);
    let mut enc = Encoder::new(&mut dest);
    enc.encode(item)?;
    match dest.flush(true) {
        Ok(()) => (),
        Err(e) => return Err(minicbor::encode::Error::message(e)),
    }
    Ok(())
}
