//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

use crate::devices::leds::LedRgb;
use zephyr::kobj_define;
use zephyr::device::led_strip::LedStrip;
use zephyr::sync::{Arc, Condvar, Mutex};

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
    color: LedRgb::new(0, 0, 24),
    count: 10000,
}]);

/// Steno mode select
pub static STENO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(0, 0, 24),
        count: 100,
    },
    Step {
        color: LedRgb::new(0, 0, 0),
        count: 100,
    },
]);

/// Steno raw mode
pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: LedRgb::new(16, 8, 0),
    count: 10000,
}]);

/// Steno raw mode select
pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(16, 8, 0),
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
    color: LedRgb::new(16, 8, 24),
    count: 10000,
}]);

/// Taipo select mode
pub static TAIPO_SELECT_INDICATOR: Indication = Indication(&[
    Step {
        color: LedRgb::new(16, 8, 24),
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
    /// The state shared with the thread that actually updates the LEDs.
    info: Arc<InfoPair>,

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

        let sys_mutex = LED_MUTEX_STATIC.init_once(()).unwrap();
        let sys_condvar = LED_CONDVAR_STATIC.init_once(()).unwrap();

        let condvar = Condvar::new_from(sys_condvar);
        let info = LedInfo {
            leds: None,
        };
        let info = Arc::new((Mutex::new_from(info, sys_mutex), condvar));

        let info2 = info.clone();
        let mut thread = LED_THREAD.init_once(LED_STACK.init_once(()).unwrap()).unwrap();
        thread.set_priority(-1);
        thread.spawn(move || {
            led_thread(leds, info2);
        });

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
            info,
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

        self.set_state(colors);
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
        self.set_state([leds; 4]);
    }

    /// Set the led state for the child thread.
    pub fn set_state(&self, leds: [LedRgb; 4]) {
        let (lock, cond) = &*self.info;
        let mut info = lock.lock().unwrap();
        info.leds = Some(leds);
        cond.notify_one();
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

// Currently, the rp2040 driver for the WS2812 LEDs is poll based (even though
// it uses PIO for writing).  We will mitigate this (about 460us to update 4
// leds), but just keeping state here, and having a slower low-priority thread
// actually update.  It will poll basically when things are idle.  This isn't
// good from a power perspective.
// TODO: Can we make this const static (not with the current way this is implemented).

type InfoPair = (Mutex<LedInfo>, Condvar);

struct LedInfo {
    // The values of the LEDs.
    leds: Option<[LedRgb; 4]>,
}

fn led_thread(mut driver: LedStrip, info: Arc<InfoPair>) -> ! {
    let limit = driver.chain_len().min(4);
    let pwm_leds = unsafe { get_pwm() };
    loop {
        let info = get_info(&*info);
        let leds = info.each_ref().map(|l| l.0);
        unsafe { driver.update(&leds[0..limit]).unwrap(); }

        // Also update the LEDs.  For now, just check for 1, but count really should be a multiple
        // of 3.
        if pwm_leds.count >= 3 {
            unsafe {
                let value = (((leds[0].r as u32) * 100) >> 8) as u8;
                pwm_set_brightness(pwm_leds.dev, 0, value);
                let value = (((leds[0].g as u32) * 100) >> 8) as u8;
                pwm_set_brightness(pwm_leds.dev, 1, value);
                let value = (((leds[0].b as u32) * 100) >> 8) as u8;
                pwm_set_brightness(pwm_leds.dev, 2, value);
            }
        }
    }
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct pwm_led_info {
    dev: *const zephyr::raw::device,
    count: u32,
}

extern "C" {
    fn get_pwm() -> pwm_led_info;
    fn pwm_set_brightness(dev: *const zephyr::raw::device,
                          index: u32,
                          value: u8)
        -> core::ffi::c_int;
}

// Helper to get the state, waiting for it to be present.
fn get_info(info: &InfoPair) -> [LedRgb; 4] {
    let (lock, cond) = info;
    let mut lock = lock.lock().unwrap();
    loop {
        if let Some(info) = lock.leds.take() {
            return info;
        }
        lock = cond.wait(lock).unwrap();
    }
}

kobj_define! {
    // Container for the LED state.
    static LED_MUTEX_STATIC: StaticMutex;
    static LED_CONDVAR_STATIC: StaticCondvar;

    // The thread for the LED writer.
    static LED_THREAD: StaticThread;
    static LED_STACK: ThreadStack<2048>;
}
