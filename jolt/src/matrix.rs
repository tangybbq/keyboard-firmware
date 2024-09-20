//! Matrix keyboard support
//!
//! Supports a keyboard connected via a pin matrix.

extern crate alloc;

use alloc::vec::Vec;

use zephyr::sys::busy_wait;
use zephyr::sys::gpio::GpioPin;
use zephyr::raw::{
    GPIO_OUTPUT_INACTIVE,
    GPIO_INPUT,
    GPIO_PULL_DOWN,
};

pub struct Matrix {
    rows: Vec<GpioPin>,
    cols: Vec<GpioPin>,
    state: Vec<Debouncer>,
}

impl Matrix {
    pub fn new(rows: Vec<GpioPin>, cols: Vec<GpioPin>) -> Matrix {
        let state = (0 .. rows.len() * cols.len()).map(|_| Debouncer::new()).collect();
        let mut result = Matrix { rows, cols, state };
        Self::pin_setup(&mut result.cols, &mut result.rows);
        result
    }

    /// Perform a single scan of the matrix, calling `act` for every key that changes.
    pub fn scan<F>(&mut self, mut act: F)
        where F: FnMut(u8, bool),
    {
        let mut states = self.state.iter_mut().enumerate();

        for col in &mut self.cols {
            col.set(true);
            busy_wait(5);
            for row in &self.rows {
                let (code, state) = states.next().unwrap();
                match state.react(row.get()) {
                    KeyAction::Press => {
                        act(code as u8, true);
                    }
                    KeyAction::Release => {
                        act(code as u8, false);
                    }
                    _ => (),
                }
            }
            col.set(false);
            // busy_wait(5);
        }
    }

    /// Setup the gpios to drive from 'push' and read from 'pull'.
    fn pin_setup(push: &mut [GpioPin], pull: &mut [GpioPin]) {
        // The 'push' values are the outputs.
        for col in push {
            col.configure(GPIO_OUTPUT_INACTIVE);
        }

        // And 'pull' are the inputs.
        for row in pull {
            row.configure(GPIO_INPUT | GPIO_PULL_DOWN);
        }
    }
}

/// The state of an individual key.
#[derive(Clone, Copy, Eq, PartialEq)]
enum KeyState {
    /// Key is in the released state.
    Released,
    /// Key is in the pressed state.
    Pressed,
    /// We've seen a release edge, and will consider it released when consistent.
    DebounceRelease,
    /// We've seen a press edge, and will consider it pressed when consistent.
    DebouncePress,
}

/// The action keys undergo.
#[derive(Clone, Copy)]
enum KeyAction {
    None,
    Press,
    Release,
}

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
            // TODO: Perhaps just two states, and a press/release flag.
            KeyState::DebouncePress => {
                if !pressed {
                    // Reset the counter any time we see a press state.
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
}
