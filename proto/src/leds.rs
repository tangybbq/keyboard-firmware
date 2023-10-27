//! Control of the LEDs.

use core::iter::once;

use smart_leds::{SmartLedsWrite, RGB8};

const OFF: RGB8 = RGB8::new(0, 0, 0);
// const INIT: RGB8 = RGB8::new(8, 8, 0);

pub struct Indication(&'static [Step]);

struct Step {
    color: RGB8,
    count: usize,
}

/// Indicates we are initializing, waiting for either USB configuration, or
/// successful communication with the primary side, which does have USB.
pub static INIT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(8, 0, 0), count: 100 },
    Step { color: RGB8::new(0, 8, 0), count: 100 },
    Step { color: RGB8::new(0, 0, 8), count: 100 },
    Step { color: OFF,                count: 300 },
]);

/// Indicates we are connected to USB, but haven't established communication
/// with the other half of the keyboard.
pub static USB_PRIMARY: Indication = Indication(&[
    Step { color: RGB8::new(8, 8, 0), count: 300 },
    Step { color: OFF,                count: 300 },
]);

/// Just off.
/*
pub static OFF_INDICATOR: Indication = Indication(&[
    Step { color: OFF,                count: 10000 },
]);
*/

/// Show we are sleeping.
pub static SLEEP_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 0, 8), count: 3000 },
    Step { color: RGB8::new(0, 0, 16), count: 3000 },
]);

/// Steno mode
pub static STENO_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 0, 32), count: 10000 },
]);

/// NKRO steno mode
pub static NKRO_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(32, 0, 32), count: 10000 },
]);

/// Artsey mode
pub static ARTSEY_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(16, 0, 0), count: 10000 },
]);

/// Artsey mode
pub static QWERTY_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 16, 0), count: 10000 },
]);

/// Artsey Nav mode
pub static ARTSEY_NAV_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(20, 20, 0), count: 10000 },
]);

pub struct LedManager<'a, L: SmartLedsWrite<Color = RGB8>> {
    leds: &'a mut L,

    /// The base display. Shown when there is nothing else. Will repeat
    /// indefinitely.
    base: &'static [Step],

    /// An override display.  Shown instead of base, used to indicate transient status.
    global: Option<&'static [Step]>,

    /// A single shot.  Runs until out of steps, and then is removed.
    oneshot: Option<&'static [Step]>,

    /// Information on the current display.
    count: usize,
    phase: usize,
}

impl<'a, L: SmartLedsWrite<Color = RGB8>> LedManager<'a, L> {
    pub fn new(leds: &'a mut L) -> Self {
        LedManager {
            leds,
            // Assumes that we are in this state.
            base: STENO_INDICATOR.0,
            global: Some(INIT_INDICATOR.0),
            oneshot: None,
            count: 0,
            phase: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.count == 0 {
            let mut steps = self.base;
            if let Some(gl) = self.global {
                steps = gl;
            }
            if let Some(one) = self.oneshot {
                steps = one;
            }

            if self.phase >= steps.len() {
                self.phase = 0;

                // If this is the oneshot, back out of that and return to an earlier state.
                if self.oneshot.is_some() {
                    self.oneshot = None;

                    // Hack, make the phase past, as the remaining will repeat,
                    // and this will cause them to restart. Better would be for
                    // each to maintain its own state.
                    self.phase = 1000;

                    // Just wait until the next tick.
                    return;
                }
            }

            let _ = self.leds.write(once(steps[self.phase].color));
            // let _ = self.leds.write(once(if self.phase { INIT } else { OFF }));
            self.count = steps[self.phase].count;
            self.phase += 1;
        } else {
            self.count -= 1;
        }
    }

    /// Set a global indicator. This will override any other status being
    /// displayed, and usually indicates either an error, or an initial
    /// condition. It also usually indicates that the keyboard can't be used
    /// yet.
    pub fn set_global(&mut self, indicator: &Indication) {
        self.global = Some(indicator.0);
        self.count = 0;
        self.phase = 0;
    }

    pub fn clear_global(&mut self) {
        self.global = None;
        if self.oneshot.is_none() {
            self.count = 0;
            self.phase = 0;
        }
    }

    pub fn set_base(&mut self, indicator: &Indication) {
        self.base = indicator.0;
        if self.oneshot.is_none() && self.global.is_none() {
            self.count = 0;
            self.phase = 0;
        }
    }

    /*
    /// Set a oneshot indicator.
    pub fn set_oneshot(&mut self, indicator: &Indication) {
        self.oneshot = Some(indicator.0);
        self.count = 0;
        self.phase = 0;
    }
    */
}
