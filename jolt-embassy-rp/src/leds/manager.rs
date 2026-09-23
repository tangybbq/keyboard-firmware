//! The modifier indicator.
//!
//! Every LED on the board is one Taipo modifier: showing the mode's background while that
//! modifier is not held, lit in that modifier's color while it is one-shot, and lit with white
//! mixed in while it is latched.  A keyboard that is only ever in Taipo, on one chord table, spends its
//! whole life in one mode and one variant, so the indicators for those said the same thing every
//! day; what actually changes minute to minute is the modifier state, and it was the one thing
//! being squeezed -- 31 states into a single LED, as a color that had to be learned.
//!
//! One modifier per LED makes the *position* carry which modifier it is, which is the part that
//! needs no learning at all.  That leaves each LED with three states to show rather than 31, so
//! the color only has to separate held from latched, and it can do that by a wide margin: the
//! whitened form is 0.08 to 0.10 apart in Oklab, against the 0.067 that the packed 31-color
//! palette could manage between any two of its states.
//!
//! The colors are ordinary saturated primaries, so they can be named -- "red is shift" is
//! something you can be told, where "the mauve one" is not.  Their wire values are not equal,
//! because equal PWM is not equal light: a ws2812's green is an order of magnitude brighter than
//! its blue at the same value.  These four sit within 0.20 to 0.22 Oklab lightness of each other.
//!
//! They are dim on purpose -- a quarter of the light the first version used, which was glaring on
//! the board -- and that is about as far down as this can go.  Balancing the four costs green
//! most, so green is the channel that runs out first: it is at 3 of 255, where a ws2812 has only
//! a few steps left and the cheaper ones start behaving oddly.  Below this, take the balance out
//! rather than the brightness.  The balance also assumes sRGB primaries, and a real ws2812's
//! green is hotter still than sRGB says, so expect these to want tuning by eye on the board
//! rather than by the numbers.
//!
//! A board with fewer LEDs than modifiers shows the leading ones and drops the rest.  That is not
//! really supported -- the boards in use all have four -- but it is what falls out, rather than a
//! panic.
//!
//! Which LED shows what, when, is `bbq_keyboard::indicator`, shared with any other firmware that
//! shows the same thing; this module is only the colors.
//!
//! A board can instead show the mode only in a short flash when it changes
//! (`ModeDisplay::Flash`), which is what a battery-powered board will do, as nothing may keep an
//! LED lit indefinitely there.  Every board here keeps the steady display described above; the
//! flash is here so it can be tried.
//!
//! The one thing besides the modifiers is the layout mode, and it is shown underneath them rather
//! than on an LED of its own, as the color an LED falls back to when its modifier is not held.
//! Taipo, whichever chord table it is on, keeps the dark board it has always had; Orsy lights all
//! four white.  Orsy and Dosh are the same nine keys and produce the same kind of output, so
//! there is otherwise nothing on the board or on the screen to say which one a chord is about to
//! be read as, and that is worth a whole board's worth of light.  Modifiers still paint over it,
//! so a modifier held in Orsy reads as its own color against the three white ones.

use bbq_keyboard::indicator::{Indicator, Level, ModeDisplay, MODE_FLASH_MS};
use bbq_keyboard::{LayoutMode, Mods};
use heapless::Vec;
use smart_leds::RGB8;

use super::{LedSet, MAX_LEDS};

const OFF: RGB8 = RGB8::new(0, 0, 0);

/// Each LED's color while its modifier is held, in the order they appear on the board.  Latched
/// adds [`LATCH`] to this.
///
/// The LEDs are in `Mods` bit order (control, shift, alt, GUI), which is what
/// `bbq_keyboard::indicator` assigns them.
static MOD_LEDS: [RGB8; 4] = [
    RGB8::new(0, 3, 0),  // green: control
    RGB8::new(8, 0, 0),  // red: shift
    RGB8::new(0, 0, 24), // blue: alt
    RGB8::new(3, 3, 0),  // yellow: GUI
];

/// Added to a lit LED while its modifier is latched.
///
/// White, so it works the same on all four hues and reads as one thing happening to whichever
/// LEDs are lit rather than as four separate colors to learn.  It raises Oklab lightness by about
/// 0.09 and leaves enough chroma that a latched red is still recognizably red.
///
/// A little more than a quarter of what it was, where the held colors are exactly a quarter.  At
/// a straight quarter this fell to 3, and the held-to-latched difference with it, to 0.066 --
/// which is the distinction the whole indicator turns on, and the one worth spending the light
/// on.
const LATCH: RGB8 = RGB8::new(4, 4, 4);

/// An LED showing the mode, white.  In Orsy, with the steady display, that is every LED that has
/// no modifier to show, for as long as Orsy is on; with the mode flash, it is the mode's image,
/// for a moment.
///
/// Equal channels, like [`LATCH`], because that is what actually looks white on these parts.
/// Summing the three balanced primaries is the version of this that reasons from their numbers,
/// and on the board it came out both glaring and distinctly purple -- the balance is eye-tuned
/// against a real ws2812's green, which is hotter than sRGB says, so it buys green's lightness
/// with far more red and blue than a white wants.  Equal channels give that back.
///
/// One, the bottom step of the part, and it is enough: the whole board is lit for as long as
/// Orsy is, and a light that is simply on needs far less of itself to be noticed than the
/// modifier colors do.  The 3 that the palette above treats as green's floor does not bind here
/// -- that floor is about holding a *ratio* between channels, and at the bottom step all three
/// are equally untrustworthy, so the worst this can do is tint a little.
///
/// It sits well below the held modifiers, which leaves a modifier in Orsy the brightest thing on
/// the board rather than a dim patch in a white field.
const MODE: RGB8 = RGB8::new(1, 1, 1);

pub struct LedManager {
    leds: LedSet,

    /// What is currently displayed, kept so [`Self::refresh`] can rewrite it.
    colors: Vec<RGB8, MAX_LEDS>,

    /// The mode and modifier state, and what each LED should show for it.
    indicator: Indicator,

    /// When the mode flash showing should end, in `Instant` milliseconds.
    flash_until: Option<u64>,
}

impl LedManager {
    pub fn new(mut leds: LedSet, display: ModeDisplay) -> Self {
        // Written once here rather than waiting for the first modifier: after a soft reset the
        // ws2812s hold whatever the last run left in them.
        let colors: Vec<RGB8, MAX_LEDS> = core::iter::repeat(OFF).take(leds.len()).collect();
        leds.update(&colors);

        LedManager {
            leds,
            colors,
            indicator: Indicator::new(display),
            flash_until: None,
        }
    }

    /// Show the layout mode.
    ///
    /// The layout tells us the mode on its first tick as well as on every change, so this is not
    /// waiting on the user to do something before the board is honest about which mode it is in.
    ///
    /// `now` is the time in `Instant` milliseconds, from which a mode flash is timed.  The caller
    /// has to wake [`Self::wake`] at [`Self::flash_deadline`], which this may have moved.
    pub fn set_mode(&mut self, mode: LayoutMode, now: u64) {
        if self.indicator.set_mode(mode) {
            self.flash_until = Some(now + MODE_FLASH_MS);
        }
        self.render();
    }

    /// When [`Self::wake`] next has to be called, if ever: the end of the mode flash showing.
    pub fn flash_deadline(&self) -> Option<u64> {
        self.flash_until
    }

    /// End the mode flash, if it is due.  Waking early does nothing.
    pub fn wake(&mut self, now: u64) {
        if self.flash_until.is_some_and(|until| until <= now) {
            self.flash_until = None;
            self.indicator.end_flash();
            self.render();
        }
    }

    /// Show the modifier state.
    ///
    /// `oneshot` is every modifier held and `sticky` the subset of those that survives a
    /// keypress.
    pub fn set_mods(&mut self, oneshot: Mods, sticky: Mods) {
        self.indicator.set_mods(oneshot, sticky);
        self.render();
    }

    /// Redraw from the mode and modifier state.
    fn render(&mut self) {
        let mode = self.mode_color();
        for (slot, (color, held)) in self.colors.iter_mut().zip(&MOD_LEDS).enumerate() {
            *color = match self.indicator.level(slot) {
                Level::Off => OFF,
                Level::Mode => mode,
                Level::Held => *held,
                Level::Latched => add(*held, LATCH),
            };
        }
        self.set_state();
    }

    /// The color of an LED showing the mode.
    ///
    /// The same for every mode: which LEDs are lit is what tells them apart.
    fn mode_color(&self) -> RGB8 {
        MODE
    }

    /// Rewrite the LEDs with what they are already showing, if they need it.
    ///
    /// Nothing here animates, so this is only insurance: a ws2812 that has taken a bad bit holds
    /// it until it is written again, and a stuck modifier light is a lie about what the next
    /// keypress will do.  That only matters while someone is typing, so this is called on each
    /// key press rather than on a timer, which would wake an idle keyboard for nothing.
    pub fn refresh(&mut self) {
        if self.leds.needs_refresh() {
            self.set_state();
        }
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
