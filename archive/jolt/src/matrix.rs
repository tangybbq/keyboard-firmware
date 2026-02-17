//! Matrix keyboard support
//!
//! Supports a keyboard connected via a pin matrix.

extern crate alloc;

use alloc::vec::Vec;

use zephyr::device::gpio::{GpioPin, GpioToken};
use zephyr::raw::{GPIO_INPUT, GPIO_OUTPUT_INACTIVE, GPIO_PULL_DOWN};
use zephyr::sys::busy_wait;

use bbq_keyboard::Side;

pub struct Matrix {
    token: GpioToken,
    rows: Vec<GpioPin>,
    cols: Vec<GpioPin>,
    state: Vec<Debouncer>,
    side: Side,
}

impl Matrix {
    pub fn new(rows: Vec<GpioPin>, cols: Vec<GpioPin>, side: Side) -> Matrix {
        let state = (0..rows.len() * cols.len())
            .map(|_| Debouncer::new())
            .collect();
        let token = unsafe { GpioToken::get_instance().unwrap() };
        let mut result = Matrix {
            token,
            rows,
            cols,
            state,
            side,
        };
        Self::pin_setup(&mut result.token, &mut result.cols, &mut result.rows);
        result
    }

    /// Perform a single scan of the matrix, calling `act` for every key that changes.
    pub fn scan<F>(&mut self, mut act: F)
    where
        F: FnMut(u8, bool),
    {
        let bias = if self.side.is_left() {
            0
        } else {
            self.state.len()
        };
        let mut states = self.state.iter_mut().enumerate();
        for col in &mut self.cols {
            unsafe {
                col.set(&mut self.token, true);
            }
            unsafe {
                busy_wait(5);
            }
            for row in &mut self.rows {
                let (code, state) = states.next().unwrap();
                match state.react(unsafe { row.get(&mut self.token) }) {
                    KeyAction::Press => {
                        act((code + bias) as u8, true);
                    }
                    KeyAction::Release => {
                        act((code + bias) as u8, false);
                    }
                    _ => (),
                }
            }
            unsafe {
                col.set(&mut self.token, false);
            }
            // busy_wait(5);
        }
    }

    /// Setup the gpios to drive from 'push' and read from 'pull'.
    fn pin_setup(token: &mut GpioToken, push: &mut [GpioPin], pull: &mut [GpioPin]) {
        // The 'push' values are the outputs.
        for col in push {
            unsafe {
                col.configure(token, GPIO_OUTPUT_INACTIVE);
            }
        }

        // And 'pull' are the inputs.
        for row in pull {
            unsafe {
                row.configure(token, GPIO_INPUT | GPIO_PULL_DOWN);
            }
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
                        if target {
                            KeyAction::Press
                        } else {
                            KeyAction::Release
                        }
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }
}
