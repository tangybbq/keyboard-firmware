//! Inter keyboard communication.

// At this point, we're just using the rp2040_hal UART type directly.

use core::mem::replace;

use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;
use defmt::{warn, info};
use embedded_hal::serial::Read;
use smart_leds::RGB8;
use sparkfun_pro_micro_rp2040::hal;
use sparkfun_pro_micro_rp2040::hal::uart::UartPeripheral;

use crate::{Side, InterState, EventQueue, Event};

use bbq_keyboard::KeyEvent;

use bbq_keyboard::serialize::{PacketBuffer, Decoder, Packet, EventVec};

pub struct InterHandler<D, P>
    where D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>
{
    uart: UartPeripheral<hal::uart::Enabled, D, P>,
    // state: State,
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
    state: InterState,
    keys: EventVec,
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
            state: InterState::Idle,
            keys: ArrayVec::new(),
        }
    }

    pub(crate) fn poll(&mut self, events: &mut EventQueue) {
        self.try_recv(events);
        self.try_send();
    }

    pub fn tick(&mut self) {
        // If we are transmitting still, just warn about the overflow.
        if !self.xmit_buffer.is_empty() {
            warn!("transmit overflow");
            return;
        }

        // Build a new packet.  Note that it is already empty.
        match self.state {
            InterState::Idle => {
                // info!("Send idle");
                Packet::Idle { side: self.side }.encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Primary => {
                // info!("Send primary");
                Packet::Primary {
                    side: self.side,
                    led: RGB8::new(0, 0, 8),
                }.encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Secondary => {
                let keys = replace(&mut self.keys, ArrayVec::new());
                if !keys.is_empty() {
                    info!("Send secondary {} keys", keys.len());
                }
                Packet::Secondary {
                    side: self.side,
                    keys,
                }.encode(&mut self.xmit_buffer, &mut self.seq);
            }
        }

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

    fn try_recv(&mut self, events: &mut EventQueue) {
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
                        // warn!("Idle packet");
                        if side == self.side {
                            warn!("Both parts are the same side")
                        }
                        // This seems to make us toggle a lot to idle state, for
                        // now just ignore it.
                        // self.set_state(InterState::Idle, events);
                    }
                    Packet::Primary { side: _, led: _ } => {
                        // Upon receiving a primary message, this tells us we
                        // are secondary.
                        info!("Got primary");
                        self.set_state(InterState::Secondary, events);
                    }
                    Packet::Secondary { side: _, keys } => {
                        events.push(Event::Heartbeat);
                        if !keys.is_empty() {
                            info!("{} keys", keys.len());
                        }
                    }
                }
            }
        }
    }

    /// Set our current state.  This is generally either Primary or Idle, where
    /// Primary indicates we have become the primary in the communication, and
    /// Idle which indicates we have disconnected from USB.
    pub(crate) fn set_state(&mut self, state: InterState, events: &mut EventQueue) {
        if self.state != state {
            self.state = state;
            events.push(Event::BecomeState(state));
        }
    }

    pub fn add_key(&mut self, key: KeyEvent) {
        self.keys.push(key);
    }
}
