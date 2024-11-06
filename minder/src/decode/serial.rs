//! Decode the serial version.
//!
//! This also assembles the entire packet in memory, as the minicbor decoder needs this.

/// Maximum packet length. This prevents attacks that allocate too much.
const MAX_PACKET: usize = 4096;

use alloc::vec::Vec;
use minicbor::Decode;

use crate::encode::serial::{START, END, QUOTE, QUOTE_FLIP};

pub struct SerialDecoder {
    /// Indicates we have seen a valid start of packet.
    inside: bool,
    /// Are we looking at the next character being quoted?
    quoting: bool,
    /// The current packet being assembled.
    buffer: Vec<u8>,
}

impl SerialDecoder {
    pub fn new() -> SerialDecoder {
        SerialDecoder {
            inside: false,
            quoting: false,
            buffer: Vec::new(),
        }
    }

    /// Add a single byte, and decode if that makes sense.  This keeps things fairly simple, and
    /// makes it easier to deal with packate boundaries not lining up with the boundaries of the
    /// received data.
    pub fn add_decode<'a, T>(&'a mut self, byte: u8) -> Option<T>
        where T: Decode<'a, ()>,
    {
        // If the buffer is overflow, discard the rest of this packet.
        if self.buffer.len() >= MAX_PACKET {
            self.inside = false;
            self.quoting = false;
            self.buffer.clear();
        }

        match byte {
            START => {
                // No matter what, forget what we've seen and start a new packet.
                self.buffer.clear();
                self.inside = true;
                self.quoting = false;
            }
            QUOTE => {
                if !self.inside || self.quoting {
                    // Invalid state, discard.
                    self.inside = false;
                    self.quoting = false;
                    return None;
                }
                self.quoting = true;
            }
            END => {
                // If quoting, this is an error.
                if !self.inside || self.quoting {
                    self.inside = false;
                    self.quoting = false;
                    return None;
                }
                let res = minicbor::decode(&self.buffer).ok();
                self.inside = false;
                // We can't clear the buffer yet, because the returned data can have references to
                // it.  Instead clear it on the next packet received.
                // self.buffer.clear();
                return res;
            }
            byte => {
                if !self.inside {
                    // We're not inside of a packet, just reset the state.
                    return None;
                }
                if self.quoting {
                    self.buffer.push(byte ^ QUOTE_FLIP);
                    self.quoting = false;
                    return None;
                }

                self.buffer.push(byte);
            }
        }
        None
    }
}
