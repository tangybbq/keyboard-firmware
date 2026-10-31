//! The modifier indicator.
//!
//! This is jolt-embassy-rp's `leds/manager.rs`, and shows the same thing in the same colors; the
//! reasoning behind the colors is there, and the two should be kept in step.  Which LED shows
//! what, when, is `bbq_keyboard::indicator`, shared by both firmwares; this module is only the
//! colors.
//!
//! In short: every LED on the board is one Taipo modifier, in `Mods` bit order (control, shift,
//! alt, GUI), showing the mode's background while that modifier is not held, lit in that
//! modifier's color while it is one-shot, and with white added while it is latched.  Taipo is
//! dark underneath; Orsy lights all four white, and shows its pending capital (green, second LED)
//! and being within a word (blue, third LED) in place of the white.
//!
//! Only the steady display is here.  The mode flash (`ModeDisplay::Flash`) needs `layout_task` to
//! wake at the flash's end, which it does not schedule yet, and no board uses it.

extern crate alloc;

use alloc::vec::Vec;
use bbq_keyboard::LayoutMode;
use bbq_keyboard::Mods;
use bbq_keyboard::indicator::{Indicator, Level, ModeDisplay};
use smart_leds::RGB8;

use super::{LedSet, Shade};

const OFF: RGB8 = RGB8::new(0, 0, 0);

/// Each LED's color while its modifier is held, in the order they appear on the board.  Latched
/// adds [`LATCH`] to this.
static MOD_LEDS: [RGB8; 4] = [
    RGB8::new(0, 3, 0),  // green: control
    RGB8::new(8, 0, 0),  // red: shift
    RGB8::new(0, 0, 24), // blue: alt
    RGB8::new(3, 3, 0),  // yellow: GUI
];

/// Added to a lit LED while its modifier is latched.
const LATCH: RGB8 = RGB8::new(4, 4, 4);

/// An LED showing the mode: in Orsy, every LED that has no modifier to show.
const MODE: RGB8 = RGB8::new(1, 1, 1);

/// Orsy: the next letter will be a capital.  Green, on the second LED.
const CAP_NEXT: RGB8 = RGB8::new(0, 2, 0);

/// Orsy: within a word.  Blue, on the third LED.
const IN_WORD: RGB8 = RGB8::new(0, 0, 16);

pub struct LedManager {
    leds: LedSet,

    /// What is currently displayed, kept so [`Self::refresh`] can rewrite it.
    shades: Vec<Shade>,

    /// The mode and modifier state, and what each LED should show for it.
    indicator: Indicator,
}

impl LedManager {
    pub fn new(mut leds: LedSet) -> Self {
        // Written once here rather than waiting for the first modifier: after a soft reset the
        // ws2812s hold whatever the last run left in them.
        let shades: Vec<Shade> = core::iter::repeat_n(Shade::default(), leds.len()).collect();
        leds.update(&shades);

        LedManager {
            leds,
            shades,
            indicator: Indicator::new(ModeDisplay::Steady),
        }
    }

    /// Show the layout mode.
    ///
    /// The layout tells us the mode on its first tick as well as on every change, so the board is
    /// honest about its mode without waiting for a key.
    pub fn set_mode(&mut self, mode: LayoutMode) {
        self.indicator.set_mode(mode);
        self.render();
    }

    /// Show the modifier state.
    ///
    /// `oneshot` is every modifier held and `sticky` the subset of those that survives a
    /// keypress.
    pub fn set_mods(&mut self, oneshot: Mods, sticky: Mods) {
        self.indicator.set_mods(oneshot, sticky);
        self.render();
    }

    /// Show what Orsy's output stage has pending for the next stroke.
    #[cfg(feature = "orsy")]
    pub fn set_orsy_pending(&mut self, cap: bool, in_word: bool) {
        self.indicator.set_orsy_pending(cap, in_word);
        self.render();
    }

    /// Redraw from the mode and modifier state.
    fn render(&mut self) {
        for (slot, (shade, held)) in self.shades.iter_mut().zip(&MOD_LEDS).enumerate() {
            shade.color = match self.indicator.level(slot) {
                Level::Off => OFF,
                Level::Mode => MODE,
                Level::Held => *held,
                Level::Latched => add(*held, LATCH),
                Level::CapNext => CAP_NEXT,
                Level::InWord => IN_WORD,
            };
        }
        self.leds.update(&self.shades);
    }

    /// Rewrite the LEDs with what they are already showing, if they need it.
    ///
    /// Nothing here animates, so this is only insurance: a ws2812 that has taken a bad bit holds
    /// it until it is written again, and a stuck modifier light is a lie about what the next
    /// keypress will do.  That only matters while someone is typing, so this is called on each
    /// key press rather than on a timer.
    pub fn refresh(&mut self) {
        if self.leds.needs_refresh() {
            self.leds.update(&self.shades);
        }
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
