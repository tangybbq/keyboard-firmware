//! Matrix keyboard support
//!
//! Supports a keyboard connected via a pin matrix.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};

use embassy_futures::select::select_slice;
use zephyr::device::gpio::GpioPin;
use zephyr::raw::GPIO_PULL_DOWN;
use zephyr::raw::ZR_GPIO_INPUT;
use zephyr::raw::ZR_GPIO_OUTPUT_INACTIVE;
use zephyr::raw::k_busy_wait;

// TODO: side

pub struct Matrix {
    rows: Vec<GpioPin>,
    cols: Vec<GpioPin>,
    state: Vec<Debouncer>,
    /// How many keys are down, by their debounced state.
    pressed: usize,
    // side: Side,
}

impl Matrix {
    pub fn new(rows: Vec<GpioPin>, cols: Vec<GpioPin> /* , side: Side*/) -> Matrix {
        let state = (0..rows.len() * cols.len())
            .map(|_| Debouncer::new())
            .collect();
        let mut result = Matrix {
            rows,
            cols,
            state,
            pressed: 0,
        };
        Self::pin_setup(&mut result.cols, &mut result.rows);
        result
    }

    /// No key is down, and none is partway through debouncing, as of the last scan.  Scanning can
    /// stop and wait for a key: there is nothing in progress to lose.
    pub fn is_idle(&self) -> bool {
        self.pressed == 0 && self.state.iter().all(Debouncer::is_idle)
    }

    /// Wait for a key to go down.
    ///
    /// Drives every column and waits for any row to go high, so that nothing needs scanning while
    /// no key is down.  Returns with the columns inactive again, ready to scan.  A return is not
    /// proof of a press (it may be a bounce, or noise), so the caller scans to find out.
    pub async fn key_wait(&mut self) {
        for col in self.cols.iter_mut() {
            col.set(true);
        }
        // SAFETY: a busy-wait; no preconditions.
        unsafe {
            k_busy_wait(5);
        }

        // SAFETY: each wait is on a different input pin, and nothing else waits on these.  The
        // waits that lose the select are dropped with it, which disarms their interrupts.
        let waits: Vec<_> = self
            .rows
            .iter_mut()
            .map(|row| unsafe { row.wait_for_high() })
            .collect();
        let mut waits = Box::into_pin(waits.into_boxed_slice());
        select_slice(waits.as_mut()).await;
        drop(waits);

        for col in self.cols.iter_mut() {
            col.set(false);
        }
    }

    /// Perform a single scan of the matrix, calling `act` for every key that changes.
    pub fn scan<F>(&mut self, mut act: F)
    where
        F: FnMut(u8, bool),
    {
        /*
        let bias = if self.side.is_left() {
            0
        } else {
            self.state.len()
        };
        */
        let bias = 0;
        let mut states = self.state.iter_mut().enumerate();
        let push = &mut self.cols;
        let pull = &mut self.rows;
        for col in push {
            col.set(true);
            // SAFETY: a busy-wait; no preconditions.
            unsafe {
                k_busy_wait(5);
            }
            for row in pull.iter_mut() {
                let (code, state) = states.next().unwrap();
                match state.react(row.get()) {
                    KeyAction::Press => {
                        self.pressed += 1;
                        act((code + bias) as u8, true);
                    }
                    KeyAction::Release => {
                        self.pressed -= 1;
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
            col.configure(ZR_GPIO_OUTPUT_INACTIVE);
        }

        // And 'pull' are the inputs.
        for row in pull {
            row.configure(ZR_GPIO_INPUT | GPIO_PULL_DOWN);
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

    /// Up, and not debouncing.
    fn is_idle(&self) -> bool {
        self.state == KeyState::Stable(false)
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
                    // Back where it started: the bounce settled, and there is no transition.  A
                    // further change starts the count over.
                    self.state = KeyState::Stable(pressed);
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
