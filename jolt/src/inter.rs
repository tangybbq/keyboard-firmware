//! Inter keyboard communication.

use core::mem::replace;

use bbq_keyboard::{Side, serialize::{Decoder, Packet, EventVec, PacketBuffer}, InterState, Event, KeyEvent};

use zephyr::driver::uart::Uart;
use zephyr::sync::channel::Sender;
use log::{warn, info};

use crate::devices::leds::LedRgb;

pub struct InterHandler {
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
    state: InterState,
    keys: EventVec,
    leds: LedRgb,
    events: Sender<Event>,
    uart: Uart,

    side_warn: bool,
}

impl InterHandler {
    #[allow(dead_code)]
    pub fn new(side: Side, uart: Uart, events: Sender<Event>) -> Self {
        Self {
            xmit_buffer: PacketBuffer::new(),
            receiver: Decoder::new(),
            seq: 1,
            leds: LedRgb::default(),
            side,
            state: InterState::Idle,
            keys: EventVec::new(),
            side_warn: false,
            uart,
            events,
        }
    }

    pub fn tick(&mut self) {
        // Make an assumption that the uart fifo is large enough to hold an
        // entire packet, and that this packet can be sent entirely in the 1ms
        // tick we have.  Zephyr doesn't have a non-blocking polling write, so
        // this would block, and if it gets stuck would block lots of things.
        loop {
            match self.uart_read() {
                Ok(Some(ch)) => {
                    if let Some(packet) = self.receiver.add_byte(ch) {
                        match packet {
                            Packet::Idle { side } => {
                                // info!("Idle packet");
                                if side == self.side && !self.side_warn {
                                    warn!("Both parts are same side");
                                    self.side_warn = true;
                                }
                            }
                            Packet::Primary { side: _, led } => {
                                let _ = led;
                                // Upon receiving a primary message, this tells us we
                                // are secondary.
                                // info!("Primary");
                                // ...
                                self.set_state(InterState::Secondary);
                            }
                            Packet::Secondary { side: _, keys } => {
                                // info!("Secondary: {:?}", keys);
                                self.events.send(Event::Heartbeat).unwrap();
                                for key in &keys {
                                    // info!("interkey: {:?}", key);
                                    self.events.send(Event::InterKey(*key)).unwrap();
                                }
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => panic!("Uart driver error: {}", e),
            }
        }

        // Transmit our state packet.
        match self.state {
            InterState::Idle => {
                Packet::Idle { side: self.side }.encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Primary => {
                Packet::Primary {
                    side: self.side,
                    led: self.leds.to_rgb8(),
                }
                .encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Secondary => {
                let keys = replace(&mut self.keys, EventVec::new());
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
        // TODO: Buffer this better.
        while let Some(ch) = self.xmit_buffer.pop_front() {
            let buf = [ch];
            match self.uart.fifo_fill(&buf) {
                Ok(1) => (),
                Ok(_) => (), // TODO: warn?
                Err(_) => (),
            }
        }
    }

    /// Set our current state.  This is generally either Primary or Idle, where
    /// Primary indicates we have become the primary in the communication, and
    /// Idle which indicates we have disconnected from USB.
    pub(crate) fn set_state(
        &mut self,
        state: InterState,
    ) {
        if self.state != state {
            self.state = state;
            info!("Inter state change: {:?}", state);
            self.events.send(Event::BecomeState(state)).unwrap();
        }
    }

    pub fn add_key(&mut self, key: KeyEvent) {
        self.keys.push(key);
    }

    /// Try to read a single byte from the UART.
    /// TODO: Buffer this better.
    fn uart_read(&mut self) -> zephyr::Result<Option<u8>> {
        let mut buf = [0u8];
        match self.uart.fifo_read(&mut buf) {
            Ok(1) => Ok(Some(buf[0])),
            Ok(0) => Ok(None),
            Ok(_) => unreachable!(),
            Err(e) => Err(e),
        }
    }

    /*
    pub fn set_other_led(&mut self, leds: LedRgb) {
        self.leds = leds;
    }
    */
}
