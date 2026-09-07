//! The modifier indicator.
//!
//! Every LED on the board is one Taipo modifier: dark while that modifier is not held, lit in
//! that modifier's color while it is one-shot, and lit with white mixed in while it is latched.
//! Nothing else is shown.  A keyboard that is only ever in Taipo, on one chord table, spends its
//! whole life in one mode and one variant, so the indicators for those said the same thing every
//! day; what actually changes minute to minute is the modifier state, and it was the one thing
//! being squeezed -- 31 states into a single LED, as a color that had to be learned.
//!
//! One modifier per LED makes the *position* carry which modifier it is, which is the part that
//! needs no learning at all.  That leaves each LED with three states to show rather than 31, so
//! the color only has to separate held from latched, and it can do that by a wide margin: the
//! whitened form is 0.10 to 0.12 apart in Oklab, against the 0.067 that the packed 31-color
//! palette could manage between any two of its states.
//!
//! The colors are ordinary saturated primaries, so they can be named -- "red is shift" is
//! something you can be told, where "the mauve one" is not.  Their wire values are not equal,
//! because equal PWM is not equal light: a ws2812's green is an order of magnitude brighter than
//! its blue at the same value.  These four sit within 0.31 to 0.35 Oklab lightness of each other,
//! which is as near as the budget allows without pulling green down into the quantization noise.
//! That balance assumes sRGB primaries; a real ws2812's green is hotter still than sRGB says, so
//! expect these to want tuning by eye on the board rather than by the numbers.
//!
//! A board with fewer LEDs than modifiers shows the leading ones and drops the rest.  That is not
//! really supported -- the boards in use all have four -- but it is what falls out, rather than a
//! panic.

use bbq_keyboard::Mods;
use heapless::Vec;
use smart_leds::RGB8;

use super::{LedSet, MAX_LEDS};

const OFF: RGB8 = RGB8::new(0, 0, 0);

/// What one LED shows.
struct ModLed {
    /// The modifier this LED is about.
    modifier: Mods,
    /// Its color while held.  Latched adds [`LATCH`] to this.
    color: RGB8,
}

/// The LEDs, in the order they appear on the board.
///
/// `Mods` bit order, which is also the order to change if the physical arrangement wants a
/// different one: this table is the whole mapping.
static MOD_LEDS: [ModLed; 4] = [
    ModLed {
        modifier: Mods::CONTROL,
        color: RGB8::new(0, 12, 0), // green
    },
    ModLed {
        modifier: Mods::SHIFT,
        color: RGB8::new(32, 0, 0), // red
    },
    ModLed {
        modifier: Mods::ALT,
        color: RGB8::new(0, 0, 96), // blue
    },
    ModLed {
        modifier: Mods::GUI,
        color: RGB8::new(12, 12, 0), // yellow
    },
];

/// Added to a lit LED while its modifier is latched.
///
/// White, so it works the same on all four hues and reads as one thing happening to whichever
/// LEDs are lit rather than as four separate colors to learn.  It raises Oklab lightness by about
/// 0.11 and leaves enough chroma that a latched red is still recognizably red.
const LATCH: RGB8 = RGB8::new(11, 11, 11);

pub struct LedManager {
    leds: LedSet,

    /// What is currently displayed, kept so [`Self::tick`] can rewrite it.
    colors: Vec<RGB8, MAX_LEDS>,
}

impl LedManager {
    pub fn new(mut leds: LedSet) -> Self {
        // Written once here rather than waiting for the first modifier: after a soft reset the
        // ws2812s hold whatever the last run left in them.
        let colors: Vec<RGB8, MAX_LEDS> = core::iter::repeat(OFF).take(leds.len()).collect();
        leds.update(&colors);

        LedManager { leds, colors }
    }

    /// Show the modifier state.
    ///
    /// `oneshot` is every modifier held and `sticky` the subset of those that survives a
    /// keypress; `sticky` is always a subset, so a latched modifier is tested for first.
    pub fn set_mods(&mut self, oneshot: Mods, sticky: Mods) {
        for (color, led) in self.colors.iter_mut().zip(&MOD_LEDS) {
            *color = if !oneshot.contains(led.modifier) {
                OFF
            } else if sticky.contains(led.modifier) {
                add(led.color, LATCH)
            } else {
                led.color
            };
        }
        self.set_state();
    }

    /// Rewrite the LEDs with what they are already showing.
    ///
    /// Nothing here animates, so this is only insurance: a ws2812 that has taken a bad bit holds
    /// it until it is written again, and a stuck modifier light is a lie about what the next
    /// keypress will do.
    pub fn tick(&mut self) {
        self.set_state();
    }

    fn set_state(&mut self) {
        self.leds.update(&self.colors);
    }
}

/// Add two colors, clamping rather than wrapping.
fn add(a: RGB8, b: RGB8) -> RGB8 {
    RGB8::new(
        a.r.saturating_add(b.r),
        a.g.saturating_add(b.g),
        a.b.saturating_add(b.b),
    )
}
