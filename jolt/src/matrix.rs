//! Matrix keyboard support
//!
//! Supports a keyboard connected via a pin matrix.

extern crate alloc;

use alloc::vec::Vec;

use zephyr::sys::busy_wait;
use zephyr::device::gpio::{GpioPin, GpioToken};
use zephyr::raw::{
    GPIO_OUTPUT_INACTIVE,
    GPIO_INPUT,
    GPIO_PULL_DOWN,
};

use bbq_keyboard::Side;

pub struct Matrix {
    token: GpioToken,
    rows: Vec<GpioPin>,
    cols: Vec<GpioPin>,
    state: Vec<Debouncer>,
    side: Side,

    // What column are we scanning.
    column: usize,
}

impl Matrix {
    pub fn new(rows: Vec<GpioPin>, cols: Vec<GpioPin>, side: Side) -> Matrix {
        let state = (0 .. rows.len() * cols.len()).map(|i| Debouncer::new(i)).collect();
        let token = unsafe { GpioToken::get_instance().unwrap() };
        let mut result = Matrix { token, rows, cols, state, side, column: 0 };
        Self::pin_setup(&mut result.token, &mut result.cols, &mut result.rows);
        result
    }

    /// Perform a single scan of the matrix, calling `act` for every key that changes.
    ///
    /// To avoid using quite as much CPU on the Matrix scan, only scan a single column each tick.
    pub fn scan<F>(&mut self, mut act: F)
        where F: FnMut(u8, bool),
    {
        let bias = if self.side.is_left() { 0 } else { self.state.len() };
        let mut states = self.state.iter_mut()
            // It is important to enumerate before skipping so that the numbers are still correct.
            .enumerate()
            .skip(self.column * self.rows.len());
        let col = &mut self.cols[self.column];
        unsafe { col.set(&mut self.token, true); }
        unsafe { busy_wait(5); }
        for row in &mut self.rows {
            let (code, state) = states.next().unwrap();
            if code != state.index {
                panic!("Mismatch on scan, key {} vs {}", code, state.index);
            }
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
        unsafe { col.set(&mut self.token, false); }
        // busy_wait(5);

        // Advance to the next tick.
        self.column += 1;
        if self.column >= self.cols.len() {
            self.column = 0;
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

    /// For debugging, put the index of the debouncer here.
    index: usize,
}

// This is the count of steps to debounce.  This really needs to be defined in terms of the number
// of columns in the keyboard.  For now, just define based on my current keyboard which happen to
// always have 6 rows.
const DEBOUNCE_COUNT: usize = 20_usize.div_ceil(6);

impl Debouncer {
    fn new(index: usize) -> Debouncer {
        Debouncer {
            state: KeyState::Stable(false),
            counter: 0,
            index,
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
