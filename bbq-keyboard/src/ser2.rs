//! Serialze 2.
//!
//! This is an attempt at a new serialization protocol for the keyboards.  There are a few changes
//! here:
//!
//! - The protocol is built using mini-cbor.  This should allow some flexibility while keeping the
//!   protocol robust.
//! - The wire encoding is handled by the 'minder' crate, using the CRC mode.  This protocol allows
//!   for similar framing robustness, without wasting a whole bit in every byte.
//! - The protocol itself is designed to be as robust (dropped packets aren't harmful), but also
//!   that the protocol can spend a majority of its time not actually sending anything.
//!
//! There are three basic pieces of information conveyed in the protocol.  One is what state we are
//! in.  Units will start in the "Idle" state, where they don't know what their purpose is.  The
//! other states are then primary and secondary.  Primary occurs when I side is configured on USB.
//! Secondary occurs when a site sees primary packets from another side.
//!
//! The second, and arguably primary, piece of information conveyed is that of the state of the
//! keyboard.  This is represented as a bitmap, one bit per key, with bits set for those keys that
//! are pressed.  The Idle or Secondary side can send keypresses, and this packet is sent when this
//! changes.  The primary side will send keypress acks, which mirror the state of the keypress.
//! This is used as a simple protocol to ensure both sides quickly settle on the state of the
//! keyboard, even in the light of dropped packets, all without sending packets excessivly.
//!
//! Lastly, the primary side also can send led-state information.  Similar to the key-state
//! protocol, the receiving side will send the same state back, which can alert the sender that it
//! was received, and the data does not need to be sent again.
//!
//! In addition, there can also be payload data of various types.  Generally, this data will be
//! larger, and not sufficient to fit in a single message.
//! 
//! The inter-side manager will generally be wrapped in the implementation side with specific code
//! to read/write the UART or other interface between the boards.

use minicbor::{Decode, Encode};
use smart_leds::RGB8;

use crate::Side;

/// The bits representing the keys that have been pressed.  The bits are numbered with 0x01 in the
/// first byte being 0, 0x80 being bit 7, and bit 8 being 0x01 in the `[1]` byte.  The size
/// corresponds with number of keys possible.
pub type KeyBits = [u8; 6];

/// The packet consists of multiple fields, many of which are optional.
#[derive(Debug, Decode, Encode, PartialEq, Eq)]
pub struct Packet {
    /// Our role, as much as is known.
    #[n(0)]
    pub role: Role,
    /// Our Side, if it is known, otherwise this is not present.
    #[n(1)]
    pub side: Side,
    /// Keypresses. For the Secondary role, this indicates the keys that are currently pressed on
    /// our device.  For the Primary role, this represents the primary's view of what keys are
    /// pressed on the Secondary side.
    #[n(2)]
    #[cbor(with = "minicbor::bytes")]
    pub keys: Option<KeyBits>,
    /// Led indication status.  For Primary, this is an indicator of how to set the LEDs.  For
    /// Secondary, this is the value we think our LEDs should be set to.
    #[n(3)]
    #[cbor(with = "rgbcbor")]
    pub leds: Option<RGB8>,
}

impl Packet {
    pub fn new(role: Role, side: Side) -> Packet {
        Packet {
            role,
            side,
            keys: None,
            leds: None,
        }
    }

    pub fn set_keys(&mut self, keys: KeyBits) -> &mut Packet {
        self.keys = Some(keys);
        self
    }

    pub fn set_leds(&mut self, leds: RGB8) -> &mut Packet {
        self.leds = Some(leds);
        self
    }
}

/// What the transmitter knows about it's role in the communication.
#[derive(Debug, Decode, Encode, Copy, Clone, Eq, PartialEq)]
#[cbor(index_only)]
pub enum Role {
    /// Idle, this device doesn't know its role.
    #[n(0)]
    Idle,
    /// This device has taken the primary role, meaning that it has seen a USB configuration.
    #[n(1)]
    Primary,
    /// This device has taken the secondary role, it is not seen a USB configuration, but has seen a
    /// packet from the other side indicating the other participant is Primary.
    #[n(2)]
    Secondary,
}

mod rgbcbor {
    //! Encoding and decoding of RGB values.
    //!
    //! These are encoded as u32, which will encode compactly with CBOR.

    use minicbor::{data::Type, encode::Write, Decoder, Encoder};
    use smart_leds::RGB8;

    pub fn decode<'b, Ctx>(
        d: &mut Decoder<'b>,
        _ctx: &mut Ctx,
    ) -> Result<Option<RGB8>, minicbor::decode::Error> {
        match d.datatype()? {
            Type::U32 => {
                let item = d.u32()?;
                Ok(Some(from_u32(item)))
            }
            Type::U16 => {
                let item = d.u16()?;
                Ok(Some(from_u32(item as u32)))
            }
            Type::U8 => {
                let item = d.u8()?;
                Ok(Some(from_u32(item as u32)))
            }
            Type::Null => {
                d.null()?;
                Ok(None)
            }
            _ => Err(minicbor::decode::Error::type_mismatch(Type::U32)),
        }
    }

    fn from_u32(item: u32) -> RGB8 {
        RGB8 {
            r: (item & 0xff) as u8,
            g: ((item >> 8) & 0xff) as u8,
            b: ((item >> 16) & 0xff) as u8,
        }
    }

    pub fn encode<Ctx, W: Write>(
        v: &Option<RGB8>,
        e: &mut Encoder<W>,
        _ctx: &mut Ctx,
    ) -> Result<(), minicbor::encode::Error<W::Error>> {
        if let Some(v) = v {
            let item = (v.r as u32) | ((v.g as u32) << 8) | ((v.b as u32) << 16);
            e.u32(item)?;
            Ok(())
        } else {
            e.null()?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod test {
    use minder::{serial_encode, SerialDecoder};
    use smart_leds::RGB8;

    use crate::Side;

    use super::{Packet, Role};

    #[test]
    fn check_packets() {
        check(&Packet::new(Role::Idle, Side::Left));

        check(
            Packet::new(Role::Primary, Side::Right)
            .set_keys([1, 0, 2, 1, 0x80, 1])
        );

        check(
            Packet::new(Role::Primary, Side::Right)
            .set_leds(RGB8::new(16, 8, 2))
        );

        check(
            Packet::new(Role::Primary, Side::Right)
            .set_keys([0xfb, 0xfb, 0xfb, 0xfb, 0xfb, 0xfb])
            .set_leds(RGB8::new(0xfd, 0xfe, 0xff))
        );

        todo!()
    }

    fn check(item: &Packet) {
        let mut buf = Vec::new();
        serial_encode(item, &mut buf, true).unwrap();
        println!("packet: {:02x?}", buf);

        // Make sure the worst case packets still fit in a single FIFO frame.
        assert!(buf.len() <= 32);

        let mut dec = SerialDecoder::new();
        let mut count = 0;
        for &byte in &buf {
            if let Some(resp) = dec.add_decode::<Packet>(byte) {
                count += 1;
                assert_eq!(item, &resp);
            }
        }
        assert_eq!(count, 1);
    }
}
