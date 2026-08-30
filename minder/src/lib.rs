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
//!
//! The one departure from strict request/reply is [`Request::GetEvent`], a long poll that lets the
//! device push something to the host without the host having to ask repeatedly.  There may be at
//! most one outstanding at a time, and the device answers it with [`Reply::Event`] when it has
//! something, [`Reply::NoEvent`] when the timeout expires, and also [`Reply::NoEvent`] if another
//! request arrives while it is pending -- that reply comes first, and the new request is then
//! answered normally.  Every request therefore still gets exactly one reply, in order, which is
//! what makes this work on a single pipe without tagging requests.

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
//
// Bumped when `Reply::Hello` gained `boot_id`, `layout_fingerprint` and
// `capabilities`.  The version is informational; `capabilities` is what a host should
// actually branch on, because it degrades one feature at a time instead of assuming a
// total order on releases.
pub static VERSION: &'static str = "2026-08-30a";

/// The capability names a device may report in [`Reply::Hello`].
///
/// A host asks "can this device do X" rather than "is this device newer than Y", so that
/// a firmware built without some feature -- or a port that has not caught up -- degrades
/// instead of hanging on a request that will never be answered.
pub mod cap {
    /// `GetEvent` long polling is answered.  Without this, a host must never issue one:
    /// an old device does not recognize the request and simply never replies.
    pub const EVENTS: &str = "events";
    /// `TestEvent` will raise synthetic events, for checking the event path.
    pub const TEST_EVENTS: &str = "test-events";
    /// `Hash` and `Program` work, so flash can be updated.
    pub const FLASH: &str = "flash";
}

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
    #[n(6)]
    /// Wait for the device to have an event to report.
    ///
    /// Replied to with [`Reply::Event`] if one arrives within `timeout_ms`, and [`Reply::NoEvent`]
    /// if the timeout expires or if another request arrives while this one is pending.  Only one
    /// of these may be outstanding at a time.
    GetEvent {
        #[n(0)]
        timeout_ms: u32,
    },
    #[n(7)]
    /// Ask the device to raise `count` test events, `delay_ms` milliseconds from now.
    ///
    /// This exists to exercise the event path.  The delay is the point of it: with it, the events
    /// arrive while a `GetEvent` is already pending, which is the case that is otherwise hard to
    /// provoke.  Answered immediately with [`Reply::Ok`].
    TestEvent {
        #[n(0)]
        count: u32,
        #[n(1)]
        delay_ms: u32,
    },
    #[n(255)]
    Reset,
}

/// Something the device wants to tell the host about, delivered through [`Request::GetEvent`].
#[derive(Debug, Clone, Encode, Decode, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Event {
    #[n(1)]
    /// A test event, raised by [`Request::TestEvent`].
    ///
    /// `seq` counts from 1 within each requested batch, so the host can see both the ordering and,
    /// when a batch is larger than the device's queue, which of them the queue dropped.
    Test {
        #[n(0)]
        seq: u32,
    },
}

#[derive(Debug, Encode, Decode, Eq, PartialEq)]
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
        /// A value that changes on every boot.
        ///
        /// A log stream is only continuous within one `boot_id`; a change means the
        /// device restarted and whatever it was buffering is gone.
        ///
        /// `None` from firmware predating this field.
        #[n(3)]
        boot_id: Option<u64>,
        /// Identifies the chord tables this firmware was built with.
        ///
        /// The host compares it against the `fingerprint` field of `layouts.json`.  They
        /// differ when the firmware is older or newer than the tables the host is reading,
        /// which makes replaying a key log through those tables quietly wrong -- so this
        /// is what lets the host refuse rather than mislead.
        ///
        /// `None` from firmware predating this field.
        #[n(4)]
        layout_fingerprint: Option<u64>,
        /// What this device can do; see [`crate::cap`].
        ///
        /// `None` from firmware predating this field, which is not the same as an empty
        /// list: it means "did not say", and such a device still answers `Hash` and
        /// `Program`.  [`Reply::supports`] treats it that way.
        #[n(5)]
        capabilities: Option<Vec<String>>,
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
    #[n(6)]
    /// The device has an event to report.  See [`Request::GetEvent`].
    Event {
        #[n(0)]
        event: Event,
    },
    #[n(7)]
    /// The device has no event to report: either the `GetEvent` timeout expired, or another
    /// request arrived while it was pending.
    NoEvent,
    #[n(8)]
    /// A generic acknowledgement, for requests with nothing to say back.
    Ok,
    #[n(254)]
    Error {
        #[n(0)]
        text: String,
    },
    #[n(255)]
    Reset,
}

impl Reply {
    /// Whether a `Hello` reply claims a capability.
    ///
    /// Firmware predating the capability list reports nothing, and is treated as having
    /// the capabilities that existed before the list did -- flash access -- rather than
    /// as having none, which would make an old device look less capable than it is.
    pub fn supports(&self, cap: &str) -> bool {
        match self {
            Reply::Hello { capabilities: Some(caps), .. } => caps.iter().any(|c| c == cap),
            Reply::Hello { capabilities: None, .. } => cap == crate::cap::FLASH,
            _ => false,
        }
    }
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

        check_roundtrip(&Request::GetEvent { timeout_ms: 5000 }, false);
        check_roundtrip(&Request::TestEvent { count: 3, delay_ms: 250 }, true);
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

/// Round trips over plain minicbor, which is what the USB bulk endpoint carries: the vendor
/// interface has packet boundaries of its own, so neither the HID nor the serial framing is
/// involved there.
#[cfg(test)]
mod tests_bulk {
    use crate::{Event, Reply, Request};

    #[test]
    fn test_request_roundtrip() {
        check::<Request>(&Request::Hello { version: crate::VERSION.to_string() });
        check::<Request>(&Request::GetEvent { timeout_ms: 0 });
        check::<Request>(&Request::GetEvent { timeout_ms: 30_000 });
        check::<Request>(&Request::GetEvent { timeout_ms: u32::MAX });
        check::<Request>(&Request::TestEvent { count: 1, delay_ms: 0 });
        check::<Request>(&Request::TestEvent { count: 100, delay_ms: 1000 });
    }

    #[test]
    fn test_reply_roundtrip() {
        check::<Reply>(&Reply::Event { event: Event::Test { seq: 1 } });
        check::<Reply>(&Reply::Event { event: Event::Test { seq: u32::MAX } });
        check::<Reply>(&Reply::NoEvent);
        check::<Reply>(&Reply::Ok);
    }

    /// The new variants must not disturb the encoding of the old ones, since an old `keyminder`
    /// and the checked-in Swift spike's hard coded bytes both have to keep working.
    #[test]
    /// A `Reply::Hello` from firmware predating `boot_id` and its neighbours still
    /// decodes, with the fields it did not send reported as absent.
    ///
    /// These are the exact bytes a mesa1 running the 2024-11-01a firmware sent.  Adding
    /// fields to a variant has to stay backward compatible in this direction, or a host
    /// update would stop being able to talk to a keyboard until it was reflashed -- which
    /// is precisely the situation where being able to talk to it matters.
    #[test]
    fn test_hello_from_older_firmware() {
        let old: &[u8] = &[
            0x82, 0x01, 0x83, 0xf6, 0x6b, 0x32, 0x30, 0x32, 0x34, 0x2d, 0x31, 0x31, 0x2d, 0x30,
            0x31, 0x61, 0x76, 0x6d, 0x65, 0x73, 0x61, 0x31, 0x2d, 0x70, 0x6e, 0x62, 0x67, 0x6d,
            0x6c, 0x69, 0x62, 0x64, 0x62, 0x69, 0x6a, 0x6b, 0x63, 0x61, 0x64,
        ];
        let reply: Reply = minicbor::decode(old).unwrap();
        let Reply::Hello {
            version,
            info,
            boot_id,
            layout_fingerprint,
            capabilities,
        } = &reply
        else {
            panic!("decoded to the wrong variant: {reply:?}");
        };
        assert_eq!(version, "2024-11-01a");
        assert_eq!(info, "mesa1-pnbgmlibdbijkcad");
        assert_eq!(*boot_id, None);
        assert_eq!(*layout_fingerprint, None);
        assert_eq!(*capabilities, None);

        // Such a device predates the capability list, but it can still flash.
        assert!(reply.supports(crate::cap::FLASH));
        assert!(!reply.supports(crate::cap::EVENTS));
    }

    fn test_hello_bytes_unchanged() {
        let mut buf = Vec::new();
        minicbor::encode(&Request::Hello { version: "2024-11-01a".to_string() }, &mut buf).unwrap();
        assert_eq!(
            buf,
            vec![
                0x82, 0x01, 0x82, 0xf6, 0x6b, 0x32, 0x30, 0x32, 0x34, 0x2d, 0x31, 0x31, 0x2d, 0x30,
                0x31, 0x61,
            ]
        );
    }

    fn check<T>(item: &T)
    where
        T: minicbor::Encode<()> + for<'a> minicbor::Decode<'a, ()> + PartialEq + std::fmt::Debug,
    {
        let mut buf = Vec::new();
        minicbor::encode(item, &mut buf).unwrap();
        let back: T = minicbor::decode(&buf).unwrap();
        assert_eq!(item, &back);
    }
}
