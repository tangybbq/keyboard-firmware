//! Inter keyboard communication.

use core::{ffi::c_int, mem::replace};

use alloc::vec::Vec;
use arraydeque::ArrayDeque;
use bbq_keyboard::{Side, serialize::{Decoder, Packet, EventVec, PacketBuffer}, Timable, InterState, Event, KeyEvent};

use crate::{info, warn, WrapTimer, event_queue, devices::leds::LedRgb};

pub struct InterHandler {
    xmit_buffer: PacketBuffer,
    receiver: Decoder,
    side: Side,
    seq: u8,
    state: InterState,
    keys: EventVec,
    leds: LedRgb,

    side_warn: bool,
    times: Vec<u64>,
}

impl InterHandler {
    pub fn new(side: Side) -> Self {
        unsafe { inter_uart_setup() }
        Self {
            xmit_buffer: ArrayDeque::new(),
            receiver: Decoder::new(),
            seq: 1,
            leds: LedRgb::default(),
            side,
            state: InterState::Idle,
            keys: EventVec::new(),
            side_warn: false,
            times: Vec::new(),
        }
    }

    pub fn tick(&mut self) {
        // Let's make sure we are being called once a ms.
        if self.times.len() < 8 {
            self.times.push(WrapTimer.get_ticks());
            if self.times.len() == 8 {
                let mut last = self.times[0];
                for num in &self.times[1..] {
                    let delta = (*num - last) as f64 / 125.0e6;
                    info!("tick {}ms", delta * 1000.0);
                    last = *num;
                }
            }
        }

        // Make an assumption that the uart fifo is large enough to hold an
        // entire packet, and that this packet can be sent entirely in the 1ms
        // tick we have.  Zephyr doesn't have a non-blocking polling write, so
        // this would block, and if it gets stuck would block lots of things.
        loop {
            let mut ch = 0;
            match unsafe { inter_uart_poll_in(&mut ch) } {
                0 => {
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
                            }
                            Packet::Secondary { side: _, keys } => {
                                // info!("Secondary: {:?}", keys);
                                if event_queue().try_send(Event::Heartbeat).is_err() {
                                    warn!("UART: event queue full");
                                }
                                for key in &keys {
                                    // info!("interkey: {:?}", key);
                                    if event_queue().try_send(Event::InterKey(*key)).is_err() {
                                        warn!("UART: event queue full");
                                    }
                                }
                            }
                        }
                    }
                }
                -1 => break,
                e => panic!("Uart driver error: {}", e),
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
        while let Some(ch) = self.xmit_buffer.pop_front() {
            unsafe { inter_uart_poll_out(ch); }
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
            if event_queue().try_send(Event::BecomeState(state)).is_err() {
                warn!("set_state: UART: event queue full");
            }
        }
    }

    pub fn add_key(&mut self, key: KeyEvent) {
        self.keys.push(key);
    }

    pub fn set_other_led(&mut self, leds: LedRgb) {
        self.leds = leds;
    }
}

extern "C" {
    fn inter_uart_poll_in(ch: *mut u8) -> c_int;
    fn inter_uart_setup();
    fn inter_uart_poll_out(ch: u8);
}
