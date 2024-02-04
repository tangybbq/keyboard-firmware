//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

use core::iter::once;

use bbq_keyboard::Event;
use rtic_sync::channel::Sender;
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
    Step {
        color: RGB8::new(8, 0, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 8, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 8),
        count: 100,
    },
    Step {
        color: OFF,
        count: 300,
    },
]);

/// Indicates we are connected to USB, but haven't established communication
/// with the other half of the keyboard.
pub static USB_PRIMARY: Indication = Indication(&[
    Step {
        color: RGB8::new(8, 8, 0),
        count: 300,
    },
    Step {
        color: OFF,
        count: 300,
    },
]);

/// Just off.
/*
pub static OFF_INDICATOR: Indication = Indication(&[
    Step { color: OFF,                count: 10000 },
]);
*/

/// Show we are sleeping.
pub static SLEEP_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 0, 8),
        count: 3000,
    },
    Step {
        color: RGB8::new(0, 0, 16),
        count: 3000,
    },
]);

/// Steno mode
pub static STENO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 0, 32),
    count: 10000,
}]);

/// Steno mode select
pub static STENO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 0, 32),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// Steno raw mode
pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(32, 32, 0),
    count: 10000,
}]);

/// Steno raw mode select
pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(32, 32, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// NKRO steno mode
pub static NKRO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(32, 0, 32),
    count: 10000,
}]);

/// NKRO steno select mode
pub static NKRO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(32, 0, 32),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// Artsey mode
pub static ARTSEY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 0, 0),
    count: 10000,
}]);

/// Artsey select mode
pub static ARTSEY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(16, 0, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// Taipo mode
pub static TAIPO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 0),
    count: 10000,
}]);

/// Taipo select mode
pub static TAIPO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(16, 8, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// Qwerty mode
pub static QWERTY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 16, 0),
    count: 10000,
}]);

/// Qwerty select mode
pub static QWERTY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 16, 0),
        count: 100,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 100,
    },
]);

/// Artsey Nav mode
pub static ARTSEY_NAV_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(20, 20, 0),
    count: 10000,
}]);

pub struct LedManager<L: SmartLedsWrite<Color = RGB8>> {
    leds: L,

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

    /// Override the indicator by LEDs sent from the other side.
    other_side: bool,
}

impl<L: SmartLedsWrite<Color = RGB8>> LedManager<L> {
    pub fn new(leds: L) -> Self {
        LedManager {
            leds,
            // Assumes that we are in this state.
            base: STENO_INDICATOR.0,
            global: Some(INIT_INDICATOR.0),
            oneshot: None,
            count: 0,
            phase: 0,
            other_side: false,
        }
    }

    pub fn tick(
        &mut self,
        events: &mut Sender<'static, Event, { crate::app::EVENT_CAPACITY }>,
    ) {
        // If the other side is active, just leave the LED alone.
        if self.other_side {
            return;
        }

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

            let rgb = steps[self.phase].color;
            let _ = events.try_send(Event::SendLed(rgb));

            let _ = self.leds.write(once(rgb));
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

    /// Override the LEDs, setting to just a value sent by the other side.
    pub fn set_other_side(&mut self, leds: RGB8) {
        self.other_side = true;
        let _ = self.leds.write(once(leds));
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
