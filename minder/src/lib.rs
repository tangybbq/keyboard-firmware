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

extern crate alloc;

use alloc::string::String;

use minicbor::{Decode, Encode};

mod encode;

pub use encode::{HidWrite, hid_encode};

pub const PACKET_SIZE: usize = 64;

// The version of the protocol described here.
pub static VERSION: &'static str = "2024-10-03a";

#[derive(Debug, Encode, Decode)]
pub enum Request {
    #[n(1)]
    Hello {
        #[n(1)]
        version: String,
    }
}

#[derive(Debug, Encode, Decode)]
pub enum Reply {
    #[n(1)]
    Hello {
        /// The protocol version.
        #[n(1)]
        version: String,
        /// Version information about this device.
        #[n(2)]
        info: String,
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use crate::{hid_encode, HidWrite, Request};

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
        let item = vec![
            Request::Hello {
                version: "This is a string long enough to make it just 64 bytes.12".to_string(),
            },
        ];
        let mut buf = HidBuf::new();
        hid_encode(&item, &mut buf).unwrap();

        let exp: Vec<Vec<u8>> = vec![];
        assert_eq!(buf.0, exp);
    }
}
