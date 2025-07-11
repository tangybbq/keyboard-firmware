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

#![cfg_attr(not(any(feature = "std", test)), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use minicbor::{bytes::{ByteArray, ByteVec}, Decode, Encode};

mod decode;
mod encode;

pub mod cobs;

pub use decode::{HidDecoder, SerialDecoder};
pub use encode::{HidWrite, hid_encode, SerialWrite, serial_encode};

pub const PACKET_SIZE: usize = 64;

// The version of the protocol described here.
pub static VERSION: &'static str = "2024-11-01a";

#[derive(Debug, Encode, Decode, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Request {
    #[n(1)]
    Hello {
        #[n(1)]
        version: String,
    },
    #[n(2)]
    ReadFlash {
        #[n(0)]
        offset: u32,
        #[n(1)]
        size: u32,
    },
    #[n(4)]
    Hash {
        #[n(0)]
        offset: u32,
        #[n(1)]
        size: u32,
    },
    #[n(5)]
    /// Program a single page of the flash.
    Program {
        #[n(0)]
        offset: u32,
        #[n(1)]
        #[cfg_attr(feature = "defmt", defmt(Debug2Format))]
        data: ByteVec,
    },
    #[n(255)]
    Reset,
}

#[derive(Debug, Encode, Decode)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Reply {
    #[n(1)]
    Hello {
        /// The protocol version.
        #[n(1)]
        version: String,
        /// Version information about this device.
        #[n(2)]
        info: String,
    },
    #[n(2)]
    Log {
        /// The message to log.
        #[n(1)]
        message: String,
    },
    #[n(3)]
    FlashData {
        /// Offset the data came from.
        #[n(0)]
        offset: u32,
        /// The data itself.
        #[n(1)]
        data: Vec<u8>,
    },
    #[n(4)]
    Hash {
        #[n(0)]
        #[cfg_attr(feature = "defmt", defmt(Debug2Format))]
        hash: ByteArray<32>,
    },
    #[n(5)]
    ProgramDone,
    #[n(254)]
    Error {
        #[n(0)]
        text: String,
    },
    #[n(255)]
    Reset,
}

#[cfg(test)]
mod tests_hid {
    use core::convert::Infallible;

    use crate::{hid_encode, HidDecoder, HidWrite, Request};

    struct HidBuf(Vec<Vec<u8>>);

    impl HidBuf {
        fn new() -> HidBuf {
            HidBuf(Vec::new())
        }
    }

    impl HidWrite for HidBuf {
        type Error = Infallible;

        fn write_packet(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
            self.0.push(buf.to_vec());
            Ok(())
        }
    }

    #[test]
    fn test_encode() {
        check_roundtrip(&[
            Request::Hello {
                version: "This is a string long enough to make it just 64 bytes.12".to_string(),
            },
        ]);
        check_roundtrip(&[
            Request::Hello {
                version: "This is a string long enough to make it just 64 bytes.123".to_string(),
            },
        ]);
    }

    fn check_roundtrip(item: &[Request]) {
        let mut buf = HidBuf::new();
        hid_encode(&item, &mut buf).unwrap();

        // Make sure we can decode this.
        let mut dec = HidDecoder::new();

        for packet in &buf.0 {
            assert!(!dec.is_ready());
            dec.add_packet(packet.as_slice());
        }
        assert!(dec.is_ready());

        let resp: Vec<Request> = dec.decode().unwrap();
        assert_eq!(item, resp);
    }
}

#[cfg(test)]
mod tests_serial {
    use crate::{serial_encode, Request, SerialDecoder};

    #[test]
    fn test_encode() {
        check_roundtrip(&Request::Hello {
            version: "This is a string".to_string(),
        }, false);

        check_roundtrip(&Request::Hello {
            version: "This b is a string".to_string(),
        }, true);
    }

    fn check_roundtrip(item: &Request, use_crc: bool) {
        let mut buf = Vec::new();
        serial_encode(item, &mut buf, use_crc).unwrap();

        // println!("buf: {:02x?}", buf);
        
        let mut dec = SerialDecoder::new();
        let mut count = 0;
        for &byte in &buf {
            if let Some(resp) = dec.add_decode::<Request>(byte) {
                count += 1;
                assert_eq!(item, &resp);
            }
        }
        assert_eq!(count, 1);
    }
}
