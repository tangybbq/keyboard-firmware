//! Inter keyboard communication.

use alloc::vec::Vec;
use alloc::vec;
use bbq_keyboard::{Side, serialize::{Decoder, Packet, KeyBits, PacketBuffer}, InterState, Event, KeyEvent};

use zephyr::{device::uart::UartIrq, sync::channel::Sender, time::NoWait};
use log::{warn, info};

use crate::devices::leds::LedRgb;

/// Our local IRQ.
///
/// We queue up two read buffers to get full double buffering.
/// Write doesn't need to be very deep, as long as the packets are always small enough to be fully
/// transmitted each frame.
type Uart = UartIrq<2, 2>;

/// Buffer size for read.  Probably best if this is larger than our largest packet.
const READ_BUFSIZE: usize = 32;

pub struct InterHandler {
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
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
    pub fn new(side: Side, mut uart: Uart, events: Sender<Event>) -> Self {
        // Give two read buffers to the uart reader.
        for _ in 0..2 {
            uart.read_enqueue(vec![0u8; READ_BUFSIZE]).unwrap();
        }

        Self {
            xmit_buffer: PacketBuffer::new(),
            receiver: Decoder::new(),
            seq: 1,
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
            if let Ok(buf) = self.uart.read_wait(NoWait) {
                // Process all of the bytes.
                for &ch in buf.as_slice() {
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
                                self.update_keys(keys);
                                /*
                                for key in &keys {
                                    // info!("interkey: {:?}", key);
                                    self.events.send(Event::InterKey(*key)).unwrap();
                                }
                                */
                            }
                        }
                    }
                }

                // Push buffer back.
                self.uart.read_enqueue(buf.into_inner()).unwrap();
            } else {
                // Once we get a timeout, stop.
                break;
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
                let keys = self.keys;
                Packet::Secondary {
                    side: self.side,
                    keys,
                }
                .encode(&mut self.xmit_buffer, &mut self.seq);
            }
        }

        // Free up any writes.
        while let Ok(_) = self.uart.write_wait(NoWait) {
        }

        // Not exactly the cleanest.
        if !self.xmit_buffer.is_empty() {
            let tmp: Vec<u8> = self.xmit_buffer.iter().cloned().collect();
            self.xmit_buffer.clear();

            // Transmit, ignoring any overflow.
            let len = tmp.len();
            let _ = self.uart.write_enqueue(tmp, 0..len);
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
        let index = key.key() / 7;
        let bit = 1u8 << (key.key() % 7);
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
            for bit in 0..7 {
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

    /*
    pub fn set_other_led(&mut self, leds: LedRgb) {
        self.leds = leds;
    }
    */
}
