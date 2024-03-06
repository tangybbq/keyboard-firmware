//! Keyboard matrix scanner.

use alloc::vec;
use alloc::vec::Vec;

use crate::devices::{PinMatrix, GpioFlags};

use crate::Result;

pub struct Matrix {
    pins: PinMatrix,
    state: Vec<Debouncer>,
}

impl Matrix {
    pub fn new(pins: PinMatrix) -> Result<Matrix> {
        // Configure the columns as outputs, driving low/high.
        for col in &pins.cols {
            col.pin_configure(GpioFlags::GPIO_OUTPUT_INACTIVE)?;
        }

        // Configure the rows as inputs.
        for row in &pins.rows {
            row.pin_configure(GpioFlags::GPIO_INPUT)?;
        }

        let state = vec![Debouncer::new(); pins.cols.len() * pins.rows.len()];

        Ok(Matrix {pins, state})
    }

    /// Perform a single scan of the matrix, calling act for every key that changes.
    pub fn scan<F>(&mut self, mut act: F) -> Result<()>
    where F: FnMut(u8, bool) -> Result<()>,
    {
        let mut states = self.state.iter_mut().enumerate();
        for col in &self.pins.cols {
            col.pin_set(true)?;
            for row in &self.pins.rows {
                let (code, state) = states.next().unwrap();
                match state.react(row.pin_get()?) {
                    KeyAction::Press => {
                        act(code as u8, true)?;
                    }
                    KeyAction::Release => {
                        act(code as u8, false)?;
                    }
                    _ => (),
                }
            }
            col.pin_set(false)?;
            // TODO: Scan delay.
        }
        Ok(())
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
