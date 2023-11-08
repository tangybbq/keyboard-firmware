//! Inter keyboard communication.

// At this point, we're just using the rp2040_hal UART type directly.

use core::mem::replace;

use arraydeque::ArrayDeque;
use defmt::{info, warn};
use embedded_hal::serial::Read;
use rtic_sync::channel::Sender;
use smart_leds::RGB8;
use sparkfun_pro_micro_rp2040::hal;
use sparkfun_pro_micro_rp2040::hal::uart::UartPeripheral;

use bbq_keyboard::{Event, InterState, KeyEvent, Side};

use bbq_keyboard::serialize::{Decoder, EventVec, Packet, PacketBuffer};

pub struct InterHandler<D, P>
where
    D: hal::uart::UartDevice,
    P: hal::uart::ValidUartPinout<D>,
{
    uart: UartPeripheral<hal::uart::Enabled, D, P>,
    // state: State,
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
    state: InterState,
    keys: EventVec,

    /// RGB values to send to other side.
    leds: RGB8,
}

impl<D: hal::uart::UartDevice, P: hal::uart::ValidUartPinout<D>> InterHandler<D, P> {
    pub fn new(mut uart: UartPeripheral<hal::uart::Enabled, D, P>, side: Side) -> Self {
        uart.enable_rx_interrupt();
        uart.enable_tx_interrupt();
        // uart.set_fifos(true);
        // uart.set_tx_watermark(FifoWatermark::Bytes16);
        // uart.set_rx_watermark(FifoWatermark::Bytes4);
        Self {
            uart,
            // state: State::Idle,
            xmit_buffer: ArrayDeque::new(),
            receiver: Decoder::new(),
            // TODO: Can we come up with a more distinct sequence number?
            seq: 1,
            side,
            state: InterState::Idle,
            keys: EventVec::new(),
            leds: RGB8::new(4, 4, 4),
        }
    }

    pub(crate) fn poll(
        &mut self,
        events: &mut Sender<'static, Event, { crate::app::EVENT_CAPACITY }>,
    ) {
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
                    led: self.leds,
                }
                .encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Secondary => {
                let keys = replace(&mut self.keys, EventVec::new());
                // if !keys.is_empty() {
                //     info!("Send secondary {} keys", keys.len());
                // }
                Packet::Secondary {
                    side: self.side,
                    keys,
                }
                .encode(&mut self.xmit_buffer, &mut self.seq);
            }
        }

        self.try_send();
    }

    fn try_send(&mut self) {
        while !self.xmit_buffer.is_empty() {
            let (piece, _) = self.xmit_buffer.as_slices();

            if piece.is_empty() {
                // Nothing to send.
                // self.uart.disable_tx_interrupt();
                break;
            }

            let rest = self.uart.write_raw(piece).unwrap_or(piece);
            let count = piece.len() - rest.len();
            if count == 0 {
                // If bytes weren't accepted, the UART is full.
                // self.uart.enable_tx_interrupt();
                return;
            }
            // info!("Sent: {}", count);
            // TODO: Is there a better way to remove these?
            for _ in 0..count {
                let _ = self.xmit_buffer.pop_front();
            }
        }
    }

    fn try_recv(&mut self, events: &mut Sender<'static, Event, { crate::app::EVENT_CAPACITY }>) {
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
                    Packet::Primary { side: _, led } => {
                        // Upon receiving a primary message, this tells us we
                        // are secondary.
                        // info!("Got primary");
                        self.set_state(InterState::Secondary, events);
                        let _ = events.try_send(Event::RecvLed(led));
                    }
                    Packet::Secondary { side: _, keys } => {
                        // info!("Secondary");
                        if events.try_send(Event::Heartbeat).is_err() {
                            warn!("UART: event queue full");
                        }
                        for key in &keys {
                            if events.try_send(Event::InterKey(*key)).is_err() {
                                warn!("UART: key event queue full");
                            }
                        }
                        // if !keys.is_empty() {
                        //     info!("{} keys", keys.len());
                        // }
                    }
                }
            }
        }
    }

    /// Set our current state.  This is generally either Primary or Idle, where
    /// Primary indicates we have become the primary in the communication, and
    /// Idle which indicates we have disconnected from USB.
    pub(crate) fn set_state(
        &mut self,
        state: InterState,
        events: &mut Sender<'static, Event, { crate::app::EVENT_CAPACITY }>,
    ) {
        if self.state != state {
            self.state = state;
            info!("Inter state change: {}", state);
            if events.try_send(Event::BecomeState(state)).is_err() {
                warn!("set_state: UART: event queue full");
            }
        }
    }

    pub fn add_key(&mut self, key: KeyEvent) {
        self.keys.push(key);
    }

    pub fn set_other_led(&mut self, leds: RGB8) {
        self.leds = leds;
    }
}
