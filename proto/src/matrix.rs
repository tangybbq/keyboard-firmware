//! Keyboard matrix handling.

use core::fmt::Debug;

use defmt::warn;
use embedded_hal::digital::v2::{InputPin, OutputPin};

use bbq_keyboard::{Event, KeyEvent, Side};
// use rtic_monotonics::Monotonic;
use rtic_monotonics::rp2040::ExtU64;
use rtic_monotonics::rp2040::Timer;
use rtic_sync::channel::Sender;

pub struct Matrix<
    E,
    I: InputPin<Error = E>,
    O: OutputPin<Error = E>,
    const NCOLS: usize,
    const NROWS: usize,
    const NKEYS: usize,
> {
    cols: [O; NCOLS],
    rows: [I; NROWS],
    keys: [Debouncer; NKEYS],
    side: Side,
}

impl<
        E: Debug,
        I: InputPin<Error = E>,
        O: OutputPin<Error = E>,
        const NCOLS: usize,
        const NROWS: usize,
        const NKEYS: usize,
    > Matrix<E, I, O, NCOLS, NROWS, NKEYS>
{
    pub fn new(cols: [O; NCOLS], rows: [I; NROWS], side: Side) -> Self {
        let keys = [Debouncer::new(); NKEYS];
        Matrix {
            cols,
            rows,
            keys,
            side,
        }
    }

    // pub fn poll(&mut self) {
    // }

    pub(crate) async fn tick<'a>(
        &mut self,
        events: &mut Sender<'a, Event, { crate::app::EVENT_CAPACITY }>,
    ) {
        for col in 0..self.cols.len() {
            self.cols[col].set_high().unwrap();
            for row in 0..self.rows.len() {
                let key = col * NROWS + row;
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
                    if events.send(Event::Matrix(act)).await.is_err() {
                        warn!("Unable to send key event");
                    }
                }
            }
            self.cols[col].set_low().unwrap();
            Timer::delay(5.micros()).await;
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
