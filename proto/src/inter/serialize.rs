//! Serialization of the inter protocol.

use core::mem::replace;

use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;
use crc::{Crc, CRC_16_IBM_SDLC, Digest};
use smart_leds::RGB8;

#[cfg(not(feature = "std"))]
use defmt::warn;

#[cfg(feature = "std")]
use log::warn;

// TODO: Make the hardcoded sizes part of the board support.

use bbq_keyboard::{Side, KeyEvent};

pub type PacketBuffer = ArrayDeque<u8, 28>;
pub type EventVec = ArrayVec<KeyEvent, 21>;

/// The CRC generator we are using.
pub const CRC: Crc<u16> = Crc::<u16>::new(&CRC_16_IBM_SDLC);

#[derive(Debug, PartialEq, Eq)]
pub enum Packet {
    /// The idle packet is sent before we know anything about which side of the
    /// channel we are on.
    Idle {
        /// Which part of the keyboard we are.
        side: Side,
    },
    /// The "primary" packet is the side connected to the USB port.
    Primary {
        /// Which side we are.
        side: Side,
        /// Set the LEDs to this value (probably should be more of a state)
        led: RGB8,
    },
    Secondary {
        /// Which side of the keyboard we are.
        side: Side,
        /// Key events.
        keys: EventVec,
    },
}

/// The version of the protocol described by this code.  It is a fatal error for
/// there to be a mismatch.  This is a value less than 128.
//const VERSION: u8 = 1;

// The protocol itself is byte-oriented.  The high bit indicates whether or not
// this is a control byte, the 0x40 bit indicates which side this packet
// originates from.  The other bits indicate the particular value being sent.
// This initial version only has three bytes:
//
// 1 - Indicates idle.  (0x81 and 0xc1)
//     followed by a 7-bit sequence number.  This could be used to help diagnose
//     dropped packets.
// 2 - Primary (0x82 and 0xc2)
//     7-bit sequence number
//     3 7-bit numbers of RGB8 values.  The low bit of the intensity is
//     discarded.
// 3 - Slave (0x83 and 0xc3)
//     7-bit sequence number
//     7-bit bytes of key events
// 3f - CRC (0xff) (note that the CRC is the same for either side)
//     2 7-bit bytes, representing the low 14 bits of the CRC16
//
// The key events are encoded as the low 6 bits (0-63) are the key number, and
// the next bit (64) indicates that this is a release event.

impl Packet {
    /// Encode this packet for the on stream.  The encoding will be placed in
    /// the given buffer.
    pub fn encode(&self, buf: &mut PacketBuffer, seq: &mut u8) {
        buf.clear();
        match self {
            Packet::Idle { side } => {
                buf.push_back(token(1, *side)).unwrap();
                buf.push_back(*seq).unwrap();
            }
            Packet::Primary { side, led } => {
                buf.push_back(token(2, *side)).unwrap();
                buf.push_back(*seq).unwrap();
                buf.push_back(led.r >> 1).unwrap();
                buf.push_back(led.g >> 1).unwrap();
                buf.push_back(led.b >> 1).unwrap();
            }
            Packet::Secondary { side, keys } => {
                buf.push_back(token(3, *side)).unwrap();
                buf.push_back(*seq).unwrap();
                for elt in keys {
                    let b = match elt {
                        KeyEvent::Press(code) => *code,
                        KeyEvent::Release(code) => *code | 64,
                    };
                    buf.push_back(b).unwrap();
                }
            }
        }

        // Compute and encode the CRC.
        buf.push_back(0xff).unwrap();
        let (a, b) = crc_split(get_crc(buf));
        buf.push_back(a).unwrap();
        buf.push_back(b).unwrap();

        let tmp = seq.wrapping_add(1);
        *seq = if tmp < 0x80 { tmp } else { 0 };
    }
}

/// A packet decoder.  This maintains all the necessary internal state to decode
/// incoming packets, and return them when they are ready.
pub struct Decoder {
    state: DecodeState,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            state: DecodeState::Init,
        }
    }

    /// Handle another incoming byte.  Returns a decoded packet when completed.
    pub fn add_byte(&mut self, byte: u8) -> Option<Packet> {
        // Is this a CRC indicator.
        if byte == 0xff {
            let state = replace(&mut self.state, DecodeState::Init);
            self.state = state.start_crc();
            return None;
        }
        // Check for tokens.  These will already reset to this "first state"
        if (byte & 0x80) != 0 {
            self.state = DecodeState::First { token: byte };
            return None;
        }

        // Otherwise deal with the data.  This isn't a full match because we
        // want to move out of some states (potentially), and use the data in
        // place in others.
        if let DecodeState::First { token } = self.state {
            let side = if (token & 0x40) == 0 { Side::Left } else { Side::Right };
            let mut crc = CRC.digest();
            let inner = match token & 0x3f {
                1 => Some(InnerDecodeState::Idle),
                2 => Some(InnerDecodeState::Primary {
                    leds: [0, 0, 0],
                    pos: 0,
                }),
                3 => Some(InnerDecodeState::Secondary {
                    events: ArrayVec::new(),
                }),
                // Otherwise, invalid, start over.
                _ => None,
            };
            match inner {
                None => self.state = DecodeState::Init,
                Some(s) => self.state = {
                    // Note that we basically ignore the sequence number, other than
                    // using it for the CRC.
                    crc.update(&[token, byte]);
                    DecodeState::Inside {
                        inner: s,
                        crc: crc,
                        side: side,
                    }
                }
            }
            return None;
        }
        if let DecodeState::Init = self.state {
            return None;
        }
        if let DecodeState::Inside { inner, crc: _, side: _ } = &mut self.state {
            inner.decode(byte);
            return None;
        }

        // Otherwise, we are in the CRC state.
        let (packet, done) = if let DecodeState::CRC { inner, expected_crc, gotten, pos, side } = &mut self.state {
            gotten[*pos] = byte;
            *pos += 1;

            if *pos == 2 {
                if *expected_crc == (gotten[0], gotten[1]) {
                    // The packet is valid.
                    // TODO: This clone is excessive copying.
                    (Some(inner.clone().into_packet(*side)), true)
                } else {
                    warn!("Invalid CRC received");
                    (None, true)
                }
            } else {
                (None, false)
            }
        } else {
            panic!("State error");
        };
        if done {
            self.state = DecodeState::Init;
        }
        packet
    }
}

/// Outer decoder state.
enum DecodeState {
    /// Between packets.
    Init,
    /// We got a token, but don't yet have the sequence number.
    First {
        token: u8,
    },
    /// We got the token and are inside the packet.
    Inside {
        inner: InnerDecodeState,
        crc: Digest<'static, u16>,
        side: Side,
    },
    /// We got the CRC header, waiting for the two bytes of the CRC.
    CRC {
        inner: InnerDecodeState,
        expected_crc: (u8, u8),
        gotten: [u8; 2],
        pos: usize,
        side: Side,
    },
}

impl DecodeState {
    fn start_crc(self) -> Self {
        match self {
            DecodeState::Inside { inner, mut crc, side } => {
                crc.update(&[0xff]);
                let result = crc.finalize();
                DecodeState::CRC {
                    inner: inner,
                    expected_crc: crc_split(result),
                    gotten: [0, 0],
                    pos: 0,
                    side,
                }
            }
            _ => {
                // If we aren't excpecting a CRC, just reset back to the
                // Init state.
                DecodeState::Init
            }
        }
    }
}

/// Internal decoder state, once we know what token this is.
#[derive(Clone)]
enum InnerDecodeState {
    Idle,
    /// Primary token and sequence received.  Waiting for the 3 LED values.
    Primary {
        /// Digest so far.
        leds: [u8; 3],
        /// Position within the components.
        pos: usize,
    },
    Secondary {
        /// Digest so far.
        events: EventVec,
    },
}

impl InnerDecodeState {
    fn decode(&mut self, byte: u8) {
        match self {
            // This is invalid, but just ignore.
            InnerDecodeState::Idle => (),
            // Primary, stores the LED values.
            InnerDecodeState::Primary { leds, pos } => {
                if *pos < 3 {
                    leds[*pos] = byte << 1;
                    *pos += 1;
                }
                // If past end, just discard.
            }
            InnerDecodeState::Secondary { events } => {
                let ev = if (byte & 64) == 0 {
                    KeyEvent::Press(byte)
                } else {
                    KeyEvent::Release(byte & 63)
                };
                events.push(ev);
            }
        }
    }

    fn into_packet(self, side: Side) -> Packet {
        match self {
            InnerDecodeState::Idle => Packet::Idle { side },
            InnerDecodeState::Primary { leds, pos: _ } => {
                let led = RGB8::new(leds[0], leds[1], leds[2]);
                Packet::Primary { side, led }
            }
            InnerDecodeState::Secondary { events } => Packet::Secondary { side, keys: events }
        }
    }
}

/// Calculate the CRC of the contents of the buffer.  Note that we only use the
/// low 14-bits of the CRC.
fn get_crc(buf: &PacketBuffer) -> u16 {
    let mut digest = CRC.digest();
    let (a, b) = buf.as_slices();
    digest.update(a);
    digest.update(b);
    digest.finalize()
}

fn crc_split(crc: u16) -> (u8, u8) {
    ((crc & 0x7f) as u8, ((crc >> 7) & 0x7f) as u8)
}

fn token(code: u8, side : Side) -> u8 {
    0x80 | code | (match side {
        Side::Left => 0x00,
        Side::Right => 0x40,
    })
}
