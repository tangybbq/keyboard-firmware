//! Packet decoding, HID version.

// As of now, the decoder allocates to assemble the entire packet in memory, as the minicbor decode
// needs to see the entire thing.

use alloc::vec::Vec;
use minicbor::Decode;

/// A packet-based CBOR decoder.
pub struct HidDecoder {
    // The state of the decoder.
    state: State,
    // The bytes being seen.
    buffer: Vec<u8>,
}

/// The state of the decoder.
#[derive(Debug)]
enum State {
    /// The initial state, nothing seen, but also nothing ready.
    Empty,
    /// We've seen something, and are expecting a new packet with the given sequence number.
    Seen(u8),
    /// Decoding is finished, and we are ready to decode.
    Ready,
    /// Out of sequence, discard until we see an initial packet again.
    Discard,
}

impl HidDecoder {
    pub fn new() -> HidDecoder {
        HidDecoder {
            state: State::Empty,
            buffer: Vec::new(),
        }
    }

    /// Add a packet to the decoder.
    ///
    /// Use [`is_ready`] to determine, after this, if a packet is ready, and [`decode`] to actually
    /// decode it.
    pub fn add_packet(&mut self, buf: &[u8]) {
        let seq = buf[0] & 0x7f;
        let last = (buf[0] & 0x80) != 0;

        match self.state {
            // Most of the states will transition out, if we see a new seq0 packet.
            State::Empty | State::Ready | State::Discard => {
                if seq != 0 {
                    self.state = State::Discard;
                    return;
                }

                self.buffer.clear();
                self.buffer.extend_from_slice(&buf[1..]);
                self.state = if last { State::Ready } else { State::Seen(1) };
            }
            State::Seen(exp_seq) => {
                if seq != exp_seq {
                    self.buffer.clear();
                    self.state = State::Discard;
                    self.buffer.clear();
                    return;
                }

                self.buffer.extend_from_slice(&buf[1..]);
                self.state = if last { State::Ready } else { State::Seen(exp_seq + 1) };
            }
        }
    }

    pub fn is_ready(&self) -> bool {
        match self.state {
            State::Ready => true,
            _ => false,
        }
    }

    /// Decode the contents of the buffer.  Only valid to be called when is_ready returns true.
    /// Note that the decoded lifetime depends on the buffered data, and adding packets takes a
    /// `&mut self`, which means that lifetime will have to end.
    pub fn decode<'a, T>(&'a mut self) -> Result<T, minicbor::decode::Error>
        where 
            T: Decode<'a, ()>,
    {
        if !self.is_ready() {
            return Err(minicbor::decode::Error::message("Attempt to decode when no message"));
        }

        minicbor::decode(&self.buffer)
    }
}
