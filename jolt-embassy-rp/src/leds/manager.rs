//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

use heapless::Vec;
use smart_leds::RGB8;

use super::{LedSet, MAX_LEDS};

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
        count: 1,
    },
    Step {
        color: RGB8::new(0, 8, 0),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 8),
        count: 1,
    },
    Step {
        color: OFF,
        count: 3,
    },
]);

/// An unreferenced indicator.  Indicates the indicator has not been assigned.
/// Intended to not be intrusive, but obvious.
pub static UNDEF_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(1, 1, 1),
        count: 1,
    },
    Step {
        color: OFF,
        count: 30,
    },
]);

/// Indicates we are connected to USB, but haven't established communication
/// with the other half of the keyboard.
pub static USB_PRIMARY: Indication = Indication(&[
    Step {
        color: RGB8::new(8, 8, 0),
        count: 3,
    },
    Step {
        color: OFF,
        count: 3,
    },
]);

/// An indicator that just stays off.
pub static OFF_INDICATOR: Indication = Indication(&[Step {
    color: OFF,
    count: 10,
}]);

/// Indicates that something is connected to the gemini protocol.
pub static GEMINI_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 0, 8),
    count: 10,
}]);

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
        count: 30,
    },
    Step {
        color: RGB8::new(0, 0, 16),
        count: 30,
    },
]);

/// Steno mode
pub static STENO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 0, 24),
    count: 100,
}]);

/// Steno mode select
pub static STENO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 0, 24),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Steno mode
pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 8, 24),
    count: 100,
}]);

/// Steno mode select
pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 8, 24),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Steno direct (for plover)
pub static STENO_DIRECT_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 0),
    count: 100,
}]);

/// Steno direct mode select
pub static STENO_DIRECT_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(16, 8, 0),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// NKRO steno mode
pub static NKRO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(32, 0, 32),
    count: 100,
}]);

/// NKRO steno select mode
pub static NKRO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(32, 0, 32),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Artsey mode
pub static ARTSEY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 0, 0),
    count: 100,
}]);

/// Artsey select mode
pub static ARTSEY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(16, 0, 0),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Taipo mode
pub static TAIPO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 24),
    count: 100,
}]);

/// Taipo select mode
pub static TAIPO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(16, 8, 24),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Qwerty mode
pub static QWERTY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 16, 0),
    count: 100,
}]);

/// Qwerty select mode
pub static QWERTY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 16, 0),
        count: 1,
    },
    Step {
        color: RGB8::new(0, 0, 0),
        count: 1,
    },
]);

/// Artsey Nav mode
pub static ARTSEY_NAV_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(20, 20, 0),
    count: 100,
}]);

pub struct LedManager {
    // All of the leds.
    leds: LedSet,

    states: Vec<LedState, MAX_LEDS>,

    /// Override the indicator by LEDs sent from the other side.
    other_side: bool,
}

struct LedState {
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

    /// Cache of the last color displayed.
    last_color: RGB8,
}

impl LedManager {
    pub fn new(leds: LedSet) -> Self {
        let len = leds.len();

        let states: Vec<_, MAX_LEDS> = (0..len)
            .map(|i| {
                if i == 0 {
                    LedState::new(UNDEF_INDICATOR.0, Some(INIT_INDICATOR.0))
                } else {
                    LedState::new(UNDEF_INDICATOR.0, None)
                }
            })
            .collect();

        LedManager {
            leds,
            states,
            other_side: false,
        }
    }

    /// Update the leds based on the various state machines.
    ///
    /// This assumes it will be run approximately every 100ms.
    pub fn tick(&mut self) {
        let colors: Vec<_, MAX_LEDS> = self
            .states
            .iter_mut()
            // TODO: The divide by four makes the colors better on the ws2812, vs the PWM.  This
            // needs to be elsewhere to work with both.
            .map(|st| st.tick() / 4)
            .collect();
        self.set_state(&colors)
    }

    /// Set a global indicator. This will override any other status being
    /// displayed, and usually indicates either an error, or an initial
    /// condition. It also usually indicates that the keyboard can't be used
    /// yet.
    pub fn set_global(&mut self, index: usize, indicator: &Indication) {
        if let Some(st) = self.states.get_mut(index) {
            st.set_global(indicator);
        }
    }

    pub fn clear_global(&mut self, index: usize) {
        if let Some(st) = self.states.get_mut(index) {
            st.clear_global();
        }
    }

    pub fn set_base(&mut self, index: usize, indicator: &Indication) {
        if let Some(st) = self.states.get_mut(index) {
            st.set_base(indicator);
        }
    }

    /// Override the LEDs, setting to just a value sent by the other side.
    /// TODO: This should allow more than one to be set.
    pub fn set_other_side(&mut self, leds: RGB8) {
        self.other_side = true;
        let state: Vec<_, MAX_LEDS> = (0..self.states.len())
            .map(|n| if n == 0 { leds } else { OFF })
            .collect();
        self.set_state(&state);
    }

    /// Update all of the leds.
    fn set_state(&mut self, leds: &[RGB8]) {
        self.leds.update(&leds);
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

impl LedState {
    fn new(base: &'static [Step], global: Option<&'static [Step]>) -> LedState {
        LedState {
            base,
            global,
            oneshot: None,
            count: 0,
            phase: 0,
            last_color: OFF,
        }
    }

    /// Perform the tick for this single LED, returning the color this LED shold
    /// be.
    fn tick(&mut self) -> RGB8 {
        let mut steps = self.base;
        if let Some(gl) = self.global {
            steps = gl;
        }
        if let Some(one) = self.oneshot {
            steps = one;
        }

        if self.count == 0 {
            if self.phase >= steps.len() {
                self.phase = 0;

                // If this is the oneshot, back out of that, and return to an
                // earlier state.
                if self.oneshot.is_some() {
                    self.oneshot = None;

                    // Hack, make the next phase past, as the remaining will
                    // repeat, and this will cause them to restart. Better would
                    // be for each to maintain its own state.
                    self.phase = 1000;

                    // Go to the next step. As long as there isn't a state with
                    // zero states, this will only recurse one deep.
                    return self.tick();
                }
            }

            if self.phase >= steps.len() {
                panic!("Stopping");
            }
            let color = steps[self.phase].color;
            self.count = steps[self.phase].count;
            self.phase += 1;
            self.last_color = color;
            color
        } else {
            self.count -= 1;
            self.last_color
        }
    }

    fn set_global(&mut self, indicator: &Indication) {
        self.global = Some(indicator.0);
        self.count = 0;
        self.phase = 0;
    }

    fn clear_global(&mut self) {
        self.global = None;
        if self.oneshot.is_none() {
            self.count = 0;
            self.phase = 0;
        }
    }

    fn set_base(&mut self, indicator: &Indication) {
        self.base = indicator.0;
        if self.oneshot.is_none() && self.global.is_none() {
            self.count = 0;
            self.phase = 0;
        }
    }
}
