//! Key Matrix Handler

use core::pin::Pin;

use bbq_keyboard::{KeyEvent, Side};
use embassy_futures::select::select_slice;
use embassy_rp::gpio::{Input, Output};
use embassy_time::{Delay, Duration, Ticker};
use embedded_hal_1::delay::DelayNs;

use crate::logging::unwrap;

const MAX_ROWS: usize = 6;
const MAX_COLS: usize = 4;
const MAX_KEYS: usize = MAX_ROWS * MAX_COLS;

/// Idle timeout for the matrix scanner.
///
/// Switching to idle mode does create a set of Futures that wait on the rows.  Not that much
/// overhead, so this doesn't need to be too large.  In idle mode, no scanning happens, and the
/// gpios are configured to interrupt.
const IDLE_TIME_US: usize = 500;

/// What to do with scan events.
pub trait MatrixAction {
    async fn handle_key(&self, event: KeyEvent);
}

pub struct Matrix {
    cols: &'static mut [Output<'static>],
    rows: &'static mut [Input<'static>],

    states: heapless::Vec<Debouncer, MAX_KEYS>,
    xlate: fn(u8) -> u8,
    side: Side,
}

impl Matrix {
    pub fn new(
        cols: &'static mut [Output<'static>],
        rows: &'static mut [Input<'static>],
        xlate: fn(u8) -> u8,
        side: Side,
    ) -> Self {
        let size = rows.len() * cols.len();

        let mut this = Self {
            cols,
            rows,
            states: heapless::Vec::new(),
            xlate,
            side,
        };

        // Create debouncers for each key.
        for _ in 0..size {
            unwrap! {this.states.push(Debouncer::new())};
        }

        this
    }

    // /// The main scanning loop.  Scans the keyboard forever, performing the specified action.
    pub async fn scanner(&mut self, action: &impl MatrixAction) {
        loop {
            self.key_wait().await;
            self.scan(action).await;
        }
    }

    /// Wait for keys.
    ///
    /// The first phase of the scanner enables all columns, and wants for any row to become high.
    /// This alleviates the need to scan when there are no keys down.
    async fn key_wait(&mut self) {
        // Assert all of the columns.
        for col in self.cols.iter_mut() {
            col.set_high();
        }

        // A short delay so we can avoid an interrupt if something is already pressed.
        Delay.delay_us(5);

        let mut row_wait: heapless::Vec<_, MAX_ROWS> =
            self.rows.iter_mut().map(|r| r.wait_for_high()).collect();
        let row_wait = unsafe { Pin::new_unchecked(row_wait.as_mut_slice()) };
        select_slice(row_wait).await;

        // Desassert all of the columns, and return so we can begin scanning.
        for col in self.cols.iter_mut() {
            col.set_low();
        }
    }

    /// Scan the matrix repeatedly.
    ///
    /// Run a once per ms scan of the matrix, responding to any events.  After a period of time that
    /// everything has settled, returns, assuming the keyboard is idle.
    async fn scan(&mut self, action: &impl MatrixAction) {
        let mut ticker = Ticker::every(Duration::from_millis(1));
        let mut pressed = 0;
        let mut idle_count = 0;

        let bias = if self.side.is_left() {
            0
        } else {
            self.states.len()
        };

        // info!("Scanner: active scanning");
        loop {
            let mut states_iter = self.states.iter_mut().enumerate();

            for col in self.cols.iter_mut() {
                col.set_high();
                Delay.delay_us(5);

                for row in self.rows.iter() {
                    let (code, state) = unwrap!(states_iter.next());
                    match state.react(row.is_high()) {
                        KeyAction::Press => {
                            action
                                .handle_key(KeyEvent::Press((self.xlate)((code + bias) as u8)))
                                .await;
                            // info!("Press: {}", code);
                            pressed += 1;
                            idle_count = 0;
                        }
                        KeyAction::Release => {
                            action
                                .handle_key(KeyEvent::Release((self.xlate)((code + bias) as u8)))
                                .await;
                            // info!("Release: {}", code);
                            pressed -= 1;
                        }
                        _ => (),
                    }
                }

                col.set_low();
            }

            if pressed == 0 {
                idle_count += 1;
                if idle_count == IDLE_TIME_US {
                    break;
                }
            }

            ticker.next().await;
        }

        // info!("Scanner: idle");
    }
}

/// The state of an individual key.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
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

impl Default for Debouncer {
    fn default() -> Self {
        Self::new()
    }
}
