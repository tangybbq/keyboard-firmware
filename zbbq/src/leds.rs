//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

use core::mem::MaybeUninit;

use crate::{devices::leds::{LedRgb, LedStrip}, zephyr::{sync::{Mutex, k_mutex}, struct_timer, Timer}};

const OFF: LedRgb = LedRgb::new(0, 0, 0);
// const INIT: LedRgb = LedRgb::new(8, 8, 0);

pub struct Indication(&'static [Step]);

struct Step {
    color: LedRgb,
    count: usize,
}

/// Indicates we are initializing, waiting for either USB configuration, or
/// successful communication with the primary side, which does have USB.
pub static INIT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(8, 0, 0),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 8, 0),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 8),
        count: 100,
    },
    Step {
        color: OFF,
        count: 300,
    },
]);

/// An unreferenced indicator.  Indicates the indicator has not been assigned.
/// Intended to not be intrusive, but obvious.
pub static UNDEF_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(1, 1, 1),
        count: 10,
    },
    Step {
        color: OFF,
        count: 3000,
    },
]);

/// Indicates we are connected to USB, but haven't established communication
/// with the other half of the keyboard.
pub static USB_PRIMARY: Indication = Indication(&[
    Step {
        color: LedRgb::new(8, 8, 0),
        count: 300,
    },
    Step {
        color: OFF,
        count: 300,
    },
]);

/// An indicator that just stays off.
pub static OFF_INDICATOR: Indication = Indication(&[
    Step {
        color: OFF,
        count: 1000,
    },
]);

/// Indicates that something is connected to the gemini protocol.
pub static GEMINI_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(0, 0, 8),
        count: 1000,
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
        color: LedRgb::new(0, 0, 8),
        count: 3000,
    },
    Step {
        color: LedRgb::new(0, 0, 16),
        count: 3000,
    },
]);

/// Steno mode
pub static STENO_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(0, 0, 8),
    count: 10000,
}]);

/// Steno mode select
pub static STENO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(0, 0, 8),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Steno raw mode
pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(8, 8, 0),
    count: 10000,
}]);

/// Steno raw mode select
pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(8, 8, 0),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// NKRO steno mode
pub static NKRO_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(32, 0, 32),
    count: 10000,
}]);

/// NKRO steno select mode
pub static NKRO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(32, 0, 32),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Artsey mode
pub static ARTSEY_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(16, 0, 0),
    count: 10000,
}]);

/// Artsey select mode
pub static ARTSEY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(16, 0, 0),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Taipo mode
pub static TAIPO_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(4, 8, 8),
    count: 10000,
}]);

/// Taipo select mode
pub static TAIPO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(4, 8, 8),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Qwerty mode
pub static QWERTY_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(0, 16, 0),
    count: 10000,
}]);

/// Qwerty select mode
pub static QWERTY_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(0, 16, 0),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Artsey Nav mode
pub static ARTSEY_NAV_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(20, 20, 0),
    count: 10000,
}]);

pub struct LedManager {
    states: [LedState; 4],

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
    last_color: LedRgb,
}

impl LedManager {
    pub fn new(leds: LedStrip) -> Self {
        // Give the driver to the thread, through the shared data.
        led_state().lock().driver = Some(leds);
        LedManager {
            states: [
                // Assumes that we are in this state.
                // Two row keyboards don't have qwerty, and start in taipo.
                // base: QWERTY_INDICATOR.0,
                LedState::new(TAIPO_INDICATOR.0, Some(INIT_INDICATOR.0)),
                LedState::new(UNDEF_INDICATOR.0, None),
                LedState::new(UNDEF_INDICATOR.0, None),
                LedState::new(UNDEF_INDICATOR.0, None),
            ],
            other_side: false,
        }
    }

    pub fn tick(&mut self) {
        // If the other side is active, just leave the LED alone.
        if self.other_side {
            return;
        }

        // TODO: Is the double iteration costly? This could use MaybeUninit, but
        // that seems overkill here.
        let mut colors = [OFF; 4];
        for (i, state) in self.states.iter_mut().enumerate() {
            let color = state.tick();
            colors[i] = color;
        }

        led_state().lock().leds = colors;
    }

    /// Set a global indicator. This will override any other status being
    /// displayed, and usually indicates either an error, or an initial
    /// condition. It also usually indicates that the keyboard can't be used
    /// yet.
    pub fn set_global(&mut self, index: usize, indicator: &Indication) {
        self.states[index].set_global(indicator);
    }

    pub fn clear_global(&mut self, index: usize) {
        self.states[index].clear_global();
    }

    pub fn set_base(&mut self, index: usize, indicator: &Indication) {
        self.states[index].set_base(indicator);
    }

    /// Override the LEDs, setting to just a value sent by the other side.
    /// TODO: This should allow more than one to be set.
    pub fn set_other_side(&mut self, leds: LedRgb) {
        self.other_side = true;
        led_state().lock().leds = [leds; 4];
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
    fn tick(&mut self) -> LedRgb {
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

/// Currently, the rp2040 driver for the WS2812 LEDs is poll based (even though
/// it uses PIO for writing).  We will mitigate this (about 460us to update 4
/// leds), but just keeping state here, and having a slower low-priority thread
/// actually update.  It will poll basically when things are idle.  This isn't
/// good from a power perspective.
/// TODO: Can we make this const static (not with the current way this is implemented).
static mut LED_STATE: MaybeUninit<Mutex<LedInfo>> = MaybeUninit::uninit();

struct LedInfo {
    // The driver itself.
    driver: Option<LedStrip>,

    // The values of the LEDs.
    leds: [LedRgb; 4],
}

#[no_mangle]
extern "C" fn init_led_state() {
    unsafe {
        LED_STATE.write(Mutex::new_raw(&mut led_mutex,
                                       LedInfo {
                                           driver: None,
                                           leds: [LedRgb::default(); 4]
                                       }));
    }
}

fn led_state() -> &'static Mutex<LedInfo> {
    unsafe {
        &*LED_STATE.as_ptr()
    }
}

#[no_mangle]
extern "C" fn led_thread_main() -> ! {
    let mut heartbeat = unsafe {
        Timer::new_from_c(&mut led_timer)
    };

    heartbeat.start(100);

    // For startup, just wait until we have our driver, which we will then take
    // for ourselves.  We need it to not be shared.
    let driver = loop {
        heartbeat.wait();
        if let Some(driver) = led_state().lock().driver.take() {
            break driver;
        }
    };

    loop {
        heartbeat.wait();
        let leds = led_state().lock().leds.clone();
        let _ = driver.update(&leds);
    }
}

extern "C" {
    static mut led_mutex: k_mutex;
    static mut led_timer: struct_timer;
}
