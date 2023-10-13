//! Keyboard matrix handling.

use core::fmt::Debug;

use cortex_m::delay::Delay;
use embedded_hal::digital::v2::{OutputPin, InputPin};

use crate::{EventQueue, Event, Side};

pub struct Matrix<'r, 'c, E, const NKEYS: usize> {
    cols: &'c mut [&'c mut dyn OutputPin<Error = E>],
    rows: &'r [&'r dyn InputPin<Error = E>],
    nrows: usize,
    keys: [Debouncer; NKEYS],
    side: Side,
}

impl<'r, 'c, E: Debug, const NKEYS: usize> Matrix<'r, 'c, E, NKEYS> {
    pub fn new(
        cols: &'c mut [&'c mut dyn OutputPin<Error = E>],
        rows: &'r [&'r dyn InputPin<Error = E>],
        side: Side,
    ) -> Self {
        let nrows = rows.len();
        let keys = [Debouncer::new(); NKEYS];
        Matrix { cols, rows, nrows, keys, side }
    }

    pub fn poll(&mut self) {
    }

    pub(crate) fn tick(&mut self, delay: &mut Delay, events: &mut EventQueue) {
        for col in 0..self.cols.len() {
            self.cols[col].set_high().unwrap();
            for row in 0..self.rows.len() {
                let key = col * self.nrows + row;
                let action = self.keys[key].react(self.rows[row].is_high().unwrap());

                let bias = if self.side.is_left() { 0 } else { NKEYS };
                let act = match action {
                    KeyAction::Press => {
                        // info!("press: {}", key);
                        Some(KeyEvent::Press((key + bias) as u8))
                    }
                    KeyAction::Release => {
                        // info!("release: {}", key);
                        Some(KeyEvent::Release((key + bias) as u8))
                    }
                    _ => None,
                };
                if let Some(act) = act {
                    events.push(Event::Matrix(act));
                }
            }
            self.cols[col].set_low().unwrap();
            delay.delay_us(5);
        }
    }
}

/// Key events indicate keys going up or down.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum KeyEvent {
    Press(u8),
    Release(u8),
}

impl KeyEvent {
    pub fn key(&self) -> u8 {
        match self {
            KeyEvent::Press(k) => *k,
            KeyEvent::Release(k) => *k,
        }
    }

    pub fn is_press(&self) -> bool {
        match self {
            KeyEvent::Press(_) => true,
            KeyEvent::Release(_) => false,
        }
    }
}

/// Individual state tracking.
#[derive(Clone, Copy, Eq, PartialEq)]
enum KeyState {
    /// Key is in released state.
    Released,
    /// Key is in pressed state.
    Pressed,
    /// We've seen a release edge, and will consider it released when consistent.
    DebounceRelease,
    /// We've seen a press edge, and will consider it pressed when consistent.
    DebouncePress,
}

#[derive(Clone, Copy)]
enum KeyAction {
    None,
    Press,
    Release,
}

// Don't really want Copy, but needed for init.
#[derive(Clone, Copy)]
struct Debouncer {
    /// State for this key.
    state: KeyState,
    /// Count how many times we've seen a given debounce state.
    counter: usize,
}

const DEBOUNCE_COUNT: usize = 20;

impl Debouncer {
    fn new() -> Debouncer {
        Debouncer {
            state: KeyState::Released,
            counter: 0,
        }
    }

    fn react(&mut self, pressed: bool) -> KeyAction {
        match self.state {
            KeyState::Released => {
                if pressed {
                    self.state = KeyState::DebouncePress;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::Pressed => {
                if !pressed {
                    self.state = KeyState::DebounceRelease;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::DebounceRelease => {
                if pressed {
                    // Reset the counter any time we see a press state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Released;
                        KeyAction::Release
                    } else {
                        KeyAction::None
                    }
                }
            }
            // TODO: We could probably just do two states, and a press/released flag.
            KeyState::DebouncePress => {
                if !pressed {
                    // Reset the counter any time we see a released state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Pressed;
                        KeyAction::Press
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn is_pressed(&self) -> bool {
        self.state == KeyState::Pressed || self.state == KeyState::DebounceRelease
    }
}
