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
//! A single-color LED (the mesa3w's white ones) shows the same levels by brightness alone; see
//! [`brightness`].
//!
//! Over all of that, jolt alone shows the host commands (`bbq_keyboard::layout::host`): every LED
//! lit while the layer is armed, and then for a moment the LED a command was about, or every LED
//! red when it could not be done.  Where typing goes is shown the same way, after an output
//! command or when it changes by itself: the profile's LED for Bluetooth, the first and fourth
//! for USB.  See [`Feedback`].
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

/// Every LED while the host command layer is armed.
const HOST_ARMED: Shade = Shade {
    color: RGB8::new(0, 0, 8),
    brightness: 30,
};

/// The LED a host command was about, alone.
const HOST_TAKEN: Shade = Shade {
    color: RGB8::new(8, 8, 8),
    brightness: 100,
};

/// Every LED, when a host command could not be carried out.
const HOST_REFUSED: Shade = Shade {
    color: RGB8::new(8, 0, 0),
    brightness: 100,
};

/// What a host command did, or where typing now goes, shown for a moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    /// The command was about this LED, a profile (from 0); or typing goes to that profile's
    /// Bluetooth host.
    Taken(usize),
    /// Typing goes to USB: the first and fourth LEDs, which no profile shows alone.
    Usb,
    /// It could not be done: a profile the board does not have, or Bluetooth on a board without;
    /// or after an output command, typing goes nowhere, as no host is there for it.
    Refused,
}

/// The LEDs that show [`Feedback::Usb`].
const USB_SLOTS: [usize; 2] = [0, 3];

/// What a level looks like on a single-color LED, as a brightness in percent.
///
/// Brightness is all such an LED has, and its position already says which modifier it is, so
/// these only have to tell the levels apart.  Each is about three times the one below, which the
/// eye sees as roughly even steps: the mode dimmest, as in Orsy it is on the whole time, then
/// Orsy's pending state, then held, with latched at full, as held to latched is the distinction
/// the indicator turns on.  The pending capital and being within a word are on different LEDs, so
/// they can share a level.  Provisional, to be tuned by eye on the board.
fn brightness(level: Level) -> u8 {
    match level {
        Level::Off => 0,
        Level::Mode => 3,
        Level::CapNext | Level::InWord => 10,
        Level::Held => 30,
        Level::Latched => 100,
    }
}

pub struct LedManager {
    leds: LedSet,

    /// What is currently displayed, kept so [`Self::refresh`] can rewrite it.
    shades: Vec<Shade>,

    /// The mode and modifier state, and what each LED should show for it.
    indicator: Indicator,

    /// The host command layer is armed.  Shown over everything else.
    host_armed: bool,

    /// A host command's result, while it is being shown; over the indicator.
    feedback: Option<Feedback>,
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
            host_armed: false,
            feedback: None,
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

    /// Show the host command layer armed, or not.
    pub fn set_host_armed(&mut self, armed: bool) {
        self.host_armed = armed;
        self.render();
    }

    /// Show a host command's result, or stop showing it.
    pub fn set_feedback(&mut self, feedback: Option<Feedback>) {
        self.feedback = feedback;
        self.render();
    }

    /// Redraw from the host command state, or else the mode and modifier state.
    fn render(&mut self) {
        if self.host_armed {
            self.shades.fill(HOST_ARMED);
            self.leds.update(&self.shades);
            return;
        }
        match self.feedback {
            Some(Feedback::Taken(slot)) => {
                for (index, shade) in self.shades.iter_mut().enumerate() {
                    *shade = if index == slot {
                        HOST_TAKEN
                    } else {
                        Shade::default()
                    };
                }
                self.leds.update(&self.shades);
                return;
            }
            Some(Feedback::Usb) => {
                for (index, shade) in self.shades.iter_mut().enumerate() {
                    *shade = if USB_SLOTS.contains(&index) {
                        HOST_TAKEN
                    } else {
                        Shade::default()
                    };
                }
                self.leds.update(&self.shades);
                return;
            }
            Some(Feedback::Refused) => {
                self.shades.fill(HOST_REFUSED);
                self.leds.update(&self.shades);
                return;
            }
            None => (),
        }

        // Only the first four LEDs have a modifier; the rest are dark, whatever an overlay left.
        self.shades.fill(Shade::default());
        for (slot, (shade, held)) in self.shades.iter_mut().zip(&MOD_LEDS).enumerate() {
            let level = self.indicator.level(slot);
            *shade = Shade {
                color: match level {
                    Level::Off => OFF,
                    Level::Mode => MODE,
                    Level::Held => *held,
                    Level::Latched => add(*held, LATCH),
                    Level::CapNext => CAP_NEXT,
                    Level::InWord => IN_WORD,
                },
                brightness: brightness(level),
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
