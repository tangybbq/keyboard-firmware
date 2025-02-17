//! A COBS encoder/decoder

/// A COBS encoder.
///
/// Encodes a packet with into a buffer length 'N', using cobs padding.
#[derive(Debug)]
pub struct CobsEncoder<const N: usize> {
    /// The buffer we are encoding into.
    buffer: heapless::Vec<u8, N>,
    /// The last place we have a stuffed value.
    last: usize,
}

impl<const N: usize> CobsEncoder<N> {
    pub fn new() -> Self {
        let mut this = Self {
            buffer: heapless::Vec::new(),
            last: 0,
        };

        // Push a zero byte, to indicate the first stuffed value.
        this.buffer.push(0).unwrap();
        this
    }

    /// Push a single byte into the buffer.
    ///
    /// For ergonomics, just panic if the heapless Vec overflows.
    pub fn push(&mut self, data: u8) {
        if data != 0 {
            // Non zero data is just pushed as is.
            self.buffer.push(data).unwrap();
        } else {
            // Store a stuffed value.  The stuff byte is replaced by an offset to this value, which
            // will then become the next stuffed value.
            // TODO: Should this check for overflow?
            // println!("Pre stuff: {:02x?}", self);
            self.buffer[self.last] = (self.buffer.len() - self.last) as u8;
            self.last = self.buffer.len();
            self.buffer.push(0).unwrap();
            // println!("Post stuff: {:02x?}", self);
        }
    }

    /// Push a slice of bytes into the buffer.
    pub fn push_slice(&mut self, data: &[u8]) {
        for &b in data {
            self.push(b);
        }
    }

    /// Return the resulting buffer, finalizing the last push.
    pub fn finish(mut self) -> heapless::Vec<u8, N> {
        // The final stuffed value will point to a zero, indicating the end of the packet.
        self.buffer[self.last] = (self.buffer.len() - self.last) as u8;
        self.buffer.push(0).unwrap();
        self.buffer
    }
}

/// Internal state of the decoder.
#[derive(Clone, Copy, Debug)]
enum DecodeState {
    /// Starting, no bytes seen.
    Start,
    /// In the midst of encoding, with 'n' bytes before the next code byte.
    Running(usize),
}

/// A COBS decoder.  Uses a buffer, of length 'N' (which does not need storage for the COBS header.
#[derive(Debug)]
pub struct CobsDecoder<const N: usize> {
    /// Buffer we are decoding into.
    buffer: heapless::Vec<u8, N>,
    /// The state of the encoder.
    state: DecodeState,
}

impl<const N: usize> CobsDecoder<N> {
    pub fn new() -> Self {
        Self {
            buffer: heapless::Vec::new(),
            state: DecodeState::Start,
        }
    }

    /// Add a byte from the decoder.  The return value indicates the packet state.
    ///
    /// Will return None if the decode is still in the midst of decoding (or if an empty packet was
    /// received).  Some(buffer) will give a slice of the data.
    #[inline]
    pub fn add_byte(&mut self, byte: u8) -> Option<&[u8]> {
        match self.state {
            DecodeState::Start => {
                self.buffer.clear();

                if byte == 0 {
                    // Empty packet.
                    return None;
                }

                // This byte is our length, with no data to be decoded.
                self.state = DecodeState::Running(byte as usize);
                None
            }
            DecodeState::Running(count) => {
                if count == 1 {
                    // This is the end, and the new byte is a length, or done indicator.
                    if byte == 0 {
                        self.state = DecodeState::Start;
                        Some(self.buffer.as_ref())
                    } else {
                        // This is a placeholder for a zero byte.
                        if self.buffer.push(0).is_err() {
                            self.state = DecodeState::Start;
                        } else {
                            self.state = DecodeState::Running(byte as usize);
                        }
                        None
                    }
                } else {
                    // A zero now, is a premature end of packet.
                    if byte == 0 {
                        self.state = DecodeState::Start;
                        return None;
                    }

                    // Otherwise, normal data.
                    self.state = DecodeState::Running(count - 1);
                    if self.buffer.push(byte).is_err() {
                        // If we get an overflow, just reset to Start, and wait for the 0 to end
                        // things.
                        self.state = DecodeState::Start;
                    }
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{CobsDecoder, CobsEncoder};

    #[test]
    fn test_encode() {
        let mut dec = CobsDecoder::<16>::new();

        try_round_trip(&mut dec, &[0, 1, 0, 2, 0, 3, 0, 4]);
        try_round_trip(&mut dec, &[1, 2, 3, 0xff, 0xfe]);
    }

    /// Try a round-trip of the given value.
    ///
    /// The decoder is shared to make sure it handles that.
    fn try_round_trip<const N: usize>(dec: &mut CobsDecoder<N>, bytes: &[u8]) {
        let mut enc = CobsEncoder::<N>::new();

        enc.push_slice(bytes);
        let encoded = enc.finish();
        println!("Result: {:#02x?}", encoded);

        let mut found = false;
        for &b in &encoded {
            if let Some(buf) = dec.add_byte(b) {
                assert_eq!(bytes, buf);

                if found {
                    panic!("Multiple packets found");
                }
                found = true;
            }
        }

        if !found {
            panic!("Decoded packet not found");
        }
    }
}
