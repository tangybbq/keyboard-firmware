//! Inter keyboard communication.

// At this point, we're just using the rp2040_hal UART type directly.

use arraydeque::ArrayDeque;
use defmt::warn;
use embedded_hal::serial::Read;
use sparkfun_pro_micro_rp2040::hal;
use sparkfun_pro_micro_rp2040::hal::uart::UartPeripheral;

use crate::Side;

use self::serialize::{PacketBuffer, Decoder, Packet};

mod serialize;

pub struct InterHandler<D, P>
    where D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>
{
    uart: UartPeripheral<hal::uart::Enabled, D, P>,
    // state: State,
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
}

impl<D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>>
    InterHandler<D, P>
{
    pub fn new(uart: UartPeripheral<hal::uart::Enabled, D, P>, side: Side) -> Self {
        Self {
            uart,
            // state: State::Idle,
            xmit_buffer: ArrayDeque::new(),
            receiver: Decoder::new(),
            // TODO: Can we come up with a more distinct sequence number?
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
        Packet::Idle { side: self.side }.encode(&mut self.xmit_buffer, &mut self.seq);

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
            if let Some(packet) = self.receiver.add_byte(byte) {
                match packet {
                    Packet::Idle { side } => {
                        if side == self.side {
                            warn!("Both parts are the same side")
                        }
                    }
                    Packet::Primary { side: _, led: _ } => {
                    }
                    Packet::Secondary { side: _, keys: _ } => {
                    }
                }
            }
        }
    }
}

