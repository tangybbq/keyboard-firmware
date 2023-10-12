//! Inter keyboard communication.

// At this point, we're just using the rp2040_hal UART type directly.

use arraydeque::ArrayDeque;
use crc::{Crc, CRC_16_IBM_SDLC};
use defmt::{warn, info};
use embedded_hal::serial::Read;
use sparkfun_pro_micro_rp2040::hal;
use sparkfun_pro_micro_rp2040::hal::uart::UartPeripheral;

use crate::Side;

pub const CRC: Crc<u16> = Crc::<u16>::new(&CRC_16_IBM_SDLC);

pub struct InterHandler<D, P>
    where D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>
{
    uart: UartPeripheral<hal::uart::Enabled, D, P>,
    // state: State,
    xmit_buffer: ArrayDeque<u8, 32>,
    recv_buffer: ArrayDeque<u8, 32>,
    crc_pos: u8,
    crc_low: u8,
    crc_high: u8,
    seq: u16,
    side: Side,
}

impl<D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>>
    InterHandler<D, P>
{
    pub fn new(uart: UartPeripheral<hal::uart::Enabled, D, P>, side: Side) -> Self {
        Self {
            uart,
            // state: State::Idle,
            xmit_buffer: ArrayDeque::new(),
            // TODO: Can we come up with a more distinct sequence number?
            recv_buffer: ArrayDeque::new(),
            crc_pos: 0,
            crc_low: 0,
            crc_high: 0,
            seq: 1,
            side,
        }
    }

    pub fn poll(&mut self) {
        self.try_recv();
        self.try_send();
    }

    pub fn tick(&mut self) {
        // If we are transmitting still, just warn about the overflow.
        if !self.xmit_buffer.is_empty() {
            warn!("transmit overflow");
            return;
        }

        // Build a new packet.  Note that it is already empty.

        // Idle header.
        self.xmit_buffer.push_back(Token::new_start(self.side) as u8).unwrap();
        push_u16(&mut self.xmit_buffer, self.seq);
        self.seq += 1;
        if self.seq > 0x3fff {
            self.seq = 0;
        }

        // End of packet, with CRC.
        self.xmit_buffer.push_back(Token::EOP as u8).unwrap();
        push_crc(&mut self.xmit_buffer);

        self.try_send();
    }

    fn try_send(&mut self) {
        while !self.xmit_buffer.is_empty() {
            let (piece, _) = self.xmit_buffer.as_slices();

            let rest = self.uart.write_raw(piece).unwrap_or(piece);
            let count = piece.len() - rest.len();
            if count == 0 {
                // If bytes weren't accepted, the UART is full.
                return;
            }
            // TODO: Is there a better way to remove these?
            for _ in 0..count {
                let _ = self.xmit_buffer.pop_front();
            }
        }
    }

    fn try_recv(&mut self) {
        while self.uart.uart_is_readable() {
            let byte = match self.uart.read() {
                Ok(b) => b,
                Err(_) => {
                    warn!("Uart recv error");
                    0x80
                }
            };
            match Token::from_byte(byte) {
                Conversion::Token(tok) => {
                    if tok.is_start() {
                        self.recv_buffer.clear();
                    }
                    let _ = self.recv_buffer.push_back(tok as u8);
                    if tok == Token::EOP {
                        self.crc_pos = 1;
                    } else {
                        self.crc_pos = 0;
                    }
                }
                Conversion::Invalid => {
                    warn!("Invalid byte received");
                }
                Conversion::Data(d) => {
                    match self.crc_pos {
                        0 => {
                            let _ = self.recv_buffer.push_back(d);
                        }
                        1 => {
                            self.crc_low = d;
                            self.crc_pos = 2;
                        }
                        2 => {
                            self.crc_high = d;
                            self.crc_pos = 3;

                            let crc = get_crc(&self.recv_buffer);
                            let crc = crc & 0x3fff;
                            if crc == ((self.crc_high as u16) << 7) | (self.crc_low as u16) {
                                // Received packet.
                                info!("Good");
                            } else {
                                info!("Bad: crc:{:x} high:{:x} low:{:x}", crc, self.crc_high, self.crc_low);
                                for b in &self.recv_buffer {
                                    info!("byte: {:x}", *b);
                                }
                            }
                        }
                        _ => (),
                    }
                }
            }
        }
    }
}

/// Push a 14-bit bit value, discarding the top two bits.  Happens little-endian.
fn push_u16<const CAP: usize>(buf: &mut ArrayDeque<u8, CAP>, value: u16) {
    buf.push_back((value & 0x7f) as u8).unwrap();
    buf.push_back(((value >> 7) & 0x7f) as u8).unwrap();
}

/// Push the low 14-bits of a CRC of the buffer so far.
fn push_crc<const CAP: usize>(buf: &mut ArrayDeque<u8, CAP>) {
    let mut digest = CRC.digest();
    let (a, b) = buf.as_slices();
    digest.update(a);
    digest.update(b);
    let crc = digest.finalize();
    push_u16(buf, crc);
}

/// Get the CRC of the buffer.
fn get_crc<const CAP: usize>(buf: &ArrayDeque<u8, CAP>) -> u16 {
    let mut digest = CRC.digest();
    let (a, b) = buf.as_slices();
    digest.update(a);
    digest.update(b);
    digest.finalize()
}

/*
enum State {
    Idle,
    Primary,
    Secondary,
}
*/

#[repr(u8)]
#[derive(PartialEq, Eq, Clone, Copy)]
enum Token {
    // 0x81 seqlo seqhi
    IdleLeft = 0x81,
    // 0x81 seqlo seqhi
    IdleRight = 0x82,

    // 0xf0 crclo crchi
    EOP = 0xf0,
}

enum Conversion {
    Token(Token),
    Invalid,
    Data(u8),
}

impl Token {
    fn is_start(self) -> bool {
        match self {
            Token::IdleLeft => true,
            Token::IdleRight => true,
            _ => false,
        }
    }

    fn new_start(side: Side) -> Token {
        if side.is_left() {
            Token::IdleLeft
        } else {
            Token::IdleRight
        }
    }

    fn from_byte(byte: u8) -> Conversion {
        match byte {
            0x81 => Conversion::Token(Token::IdleLeft),
            0x82 => Conversion::Token(Token::IdleRight),
            0xf0 => Conversion::Token(Token::EOP),
            b if b >= 0x80 => Conversion::Invalid,
            b => Conversion::Data(b),
        }
    }
}
