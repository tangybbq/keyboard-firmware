//! Serial-based packetized encoding.
//!
//! Encode a CBOR packet for sending over a serial port (or something like a serial port).  The
//! data stream uses simple framing to be able to recover from various data errors.
//!
//! Note that, as of this point, this protocol is too simplistic to use over an actual serial port,
//! and only adds framing support.  CRCs and such would need to be added for real serial port use.

use core::convert::Infallible;

use alloc::vec::Vec;
use minicbor::Encode;

/// Write trait for our data.  Just borrows the minicbor trait, as it is exactly what we want, and
/// makes error propagation rather easy.
pub use minicbor::encode::Write as SerialWrite;

// The encoding is simple. Packets start with START, end with END, and QUOTE followed by the
// following byte xor-d with QUOTE_FLIP are used to keep these bytes from being present in the
// stream.
//
// These values are chosen as they are reserved code in CBOR, not valid UTF-8, and for most encoded
// data, will probably only occur within embedded numbers.

pub const START: u8 = 0xfe;
pub const END: u8 = 0xfd;
pub const QUOTE: u8 = 0xfc;
pub const END_CRC: u8 = 0xfb;

pub const QUOTE_FLIP: u8 = 0x80;

/// Implements Write for a destination vector, applying the given quoting.
struct VecWrite {
    buffer: Vec<u8>,
}

impl VecWrite {
    fn new() -> VecWrite {
        VecWrite {
            buffer: Vec::new(),
        }
    }
}

impl SerialWrite for VecWrite {
    type Error = Infallible;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        for &b in buf {
            if b == START || b == END || b == QUOTE || b == END_CRC {
                self.buffer.push(QUOTE);
                self.buffer.push(b ^ QUOTE_FLIP);
            } else {
                self.buffer.push(b);
            }
        }
        Ok(())
    }
}

// At this point, the encoder allocates the transmit buffer before sending.  Since we don't have the
// same kinds of constraints as HID, there is little reason to encode more than one top-level
// Request or Reply, so this generally isn't an issue.

pub fn serial_encode<T: Encode<()>, W: SerialWrite>(item: T, mut write: W) -> Result<(), W::Error> {
    let mut buf = VecWrite::new();
    buf.buffer.push(START);
    minicbor::encode(item, &mut buf).unwrap();
    buf.buffer.push(END);

    write.write_all(&buf.buffer)
}
