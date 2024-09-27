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

use bbq_keyboard::Side;

pub struct Matrix {
    rows: Vec<GpioPin>,
    cols: Vec<GpioPin>,
    state: Vec<Debouncer>,
    side: Side
}

impl Matrix {
    pub fn new(rows: Vec<GpioPin>, cols: Vec<GpioPin>, side: Side) -> Matrix {
        let state = (0 .. rows.len() * cols.len()).map(|_| Debouncer::new()).collect();
        let mut result = Matrix { rows, cols, state, side };
        Self::pin_setup(&mut result.cols, &mut result.rows);
        result
    }

    /// Perform a single scan of the matrix, calling `act` for every key that changes.
    pub fn scan<F>(&mut self, mut act: F)
        where F: FnMut(u8, bool),
    {
        let bias = if self.side.is_left() { 0 } else { self.state.len() };
        let mut states = self.state.iter_mut().enumerate();
        for col in &mut self.cols {
            col.set(true);
            busy_wait(5);
            for row in &self.rows {
                let (code, state) = states.next().unwrap();
                match state.react(row.get()) {
                    KeyAction::Press => {
                        act((code + bias) as u8, true);
                    }
                    KeyAction::Release => {
                        act((code + bias) as u8, false);
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
    /// Key is stable with the given pressed state.
    Stable(bool),
    /// We've detected the start of a transition to the dest, but need to see it stable before
    /// considering it done.
    Debounce(bool),
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
            state: KeyState::Stable(false),
            counter: 0,
        }
    }

    fn react(&mut self, pressed: bool) -> KeyAction {
        match self.state {
            KeyState::Stable(cur) => {
                if cur != pressed {
                    self.state = KeyState::Debounce(pressed);
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::Debounce(target) => {
                if target != pressed {
                    // Reset the counter any time the state isn't our goal.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Stable(target);
                        if target { KeyAction::Press } else { KeyAction::Release }
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }
}
