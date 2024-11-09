//! Inter keyboard communication.

use arraydeque::ArrayDeque;
use bbq_keyboard::{ser2::{KeyBits, Packet, Role}, Event, InterState, KeyEvent, Side};

use minder::{serial_encode, SerialDecoder, SerialWrite};
use zephyr::device::uart::Uart;
use zephyr::sync::channel::Sender;
use log::{warn, info};

use crate::devices::leds::LedRgb;

/// A buffer large enough to hold a single packet.
type PacketBuffer = ArrayDeque<u8, 32>;

pub struct InterHandler {
    xmit_buffer: PacketBuffer,
    receiver: SerialDecoder,
    side: Side,
    state: InterState,
    /// Keys being sent.
    keys: KeyBits,
    /// Values of keys pressed since last time we received a packet.
    last_keys: KeyBits,
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
            receiver: SerialDecoder::new(),
            leds: LedRgb::default(),
            side,
            state: InterState::Idle,
            keys: KeyBits::default(),
            last_keys: KeyBits::default(),
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
                    if let Some(packet) = self.receiver.add_decode::<Packet>(ch) {
                        // info!("rcv: {:?}", packet);
                        match packet.role {
                            Role::Idle => {
                                if packet.side == self.side && !self.side_warn {
                                    warn!("Both parts are same side");
                                    self.side_warn = true;
                                }
                            }
                            Role::Primary => {
                                // Upon receiving a primary message, this tells us we are secondary.
                                self.set_state(InterState::Secondary);
                            }
                            Role::Secondary => {
                                self.events.send(Event::Heartbeat).unwrap();
                                if let Some(keys) = packet.keys {
                                    self.update_keys(keys);
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
        let mut packet;
        match self.state {
            InterState::Idle => {
                packet = Packet::new(Role::Idle, self.side);
                // Packet::Idle { side: self.side }.encode(&mut self.xmit_buffer, &mut self.seq);
            }
            InterState::Primary => {
                packet = Packet::new(Role::Primary, self.side);
                packet.set_leds(self.leds.to_rgb8());
            }
            InterState::Secondary => {
                packet = Packet::new(Role::Secondary, self.side);
                packet.set_keys(self.keys);
            }
        }
        self.xmit_buffer.clear();
        serial_encode(&packet, PacketWrap(&mut self.xmit_buffer), true).unwrap();

        self.try_send();
    }

    fn try_send(&mut self) {
        // TODO: Buffer this better.
        while let Some(ch) = self.xmit_buffer.pop_front() {
            let buf = [ch];
            match unsafe { self.uart.fifo_fill(&buf) } {
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
        let index = key.key() / 8;
        let bit = 1u8 << (key.key() % 8);
        if key.is_press() {
            self.keys[index as usize] |= bit;
        } else {
            self.keys[index as usize] &= !bit;
        }
    }

    /// Send events for every key that has changed.
    fn update_keys(&mut self, keys: KeyBits) {
        // Quickly handle the common case of no changes.
        if self.last_keys == keys {
            return;
        }

        // info!("keys: {:02x?}, last: {:02x?}", keys, self.last_keys);

        let mut key = 0;
        for byte in 0..keys.len() {
            for bit in 0..8 {
                let bnum = 1 << bit;
                if (keys[byte] & bnum) != (self.last_keys[byte] & bnum) {
                    let ev = if (keys[byte] & bnum) != 0 {
                        KeyEvent::Press(key)
                    } else {
                        KeyEvent::Release(key)
                    };
                    self.events.send(Event::InterKey(ev)).unwrap();
                }

                key += 1;
            }
        }
        self.last_keys = keys;
    }

    /// Try to read a single byte from the UART.
    /// TODO: Buffer this better.
    fn uart_read(&mut self) -> zephyr::Result<Option<u8>> {
        let mut buf = [0u8];
        match unsafe { self.uart.fifo_read(&mut buf) } {
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

struct PacketWrap<'a>(&'a mut PacketBuffer);

impl<'a> SerialWrite for PacketWrap<'a> {
    type Error = ();

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        // Note that this evicts from the front, which is a different failure than would be seen
        // with repeated push_back.
        self.0.extend_back(buf.into_iter().cloned());
        Ok(())
    }
}
