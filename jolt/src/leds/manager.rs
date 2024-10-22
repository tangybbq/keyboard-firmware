//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

extern crate alloc;

use alloc::vec::Vec;
use rgb::RGB8;
use zephyr::kobj_define;
use zephyr::sync::{Arc, Condvar, Mutex};

use crate::Stats;

use super::LedSet;

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
pub static OFF_INDICATOR: Indication = Indication(&[
    Step {
        color: OFF,
        count: 10,
    },
]);

/// Indicates that something is connected to the gemini protocol.
pub static GEMINI_INDICATOR: Indication = Indication(&[
    Step {
        color: RGB8::new(0, 0, 8),
        count: 10,
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

/// Steno raw mode
pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 0),
    count: 100,
}]);

/// Steno raw mode select
pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
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
    /// The state shared with the thread that actually updates the LEDs.
    info: Arc<InfoPair>,

    states: Vec<LedState>,

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
    pub fn new(leds: LedSet, stats: Arc<Stats>) -> Self {
        let len = leds.len();

        let sys_mutex = LED_MUTEX_STATIC.init_once(()).unwrap();
        let sys_condvar = LED_CONDVAR_STATIC.init_once(()).unwrap();

        let condvar = Condvar::new_from(sys_condvar);
        let info = LedInfo {
            leds: None,
        };
        let info = Arc::new((Mutex::new_from(info, sys_mutex), condvar));

        let info2 = info.clone();
        let mut thread = LED_THREAD.init_once(LED_STACK.init_once(()).unwrap()).unwrap();
        // Low priority for PWM, need high priority for WS2812
        thread.set_priority(10);
        thread.set_name(c"leds");
        thread.spawn(move || {
            led_thread(leds, info2, stats);
        });

        let states: Vec<_> = (0..len).map(|i| {
            if i == 0 {
                LedState::new(UNDEF_INDICATOR.0, Some(INIT_INDICATOR.0))
            } else {
                LedState::new(UNDEF_INDICATOR.0, None)
            }
        }).collect();

        LedManager {
            states,
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
        let colors: Vec<_> = self.states
            .iter_mut()
            .map(|st| st.tick())
            .collect();

        self.set_state(colors);
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
        let state: Vec<_> = (0..self.states.len())
            .map(|n| {
                if n == 0 {
                    leds
                } else {
                    OFF
                }
            }).collect();
        self.set_state(state);
    }

    /// Set the led state for the child thread.
    fn set_state(&self, leds: Vec<RGB8>) {
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

// Currently, the rp2040 driver for the WS2812 LEDs is poll based (even though
// it uses PIO for writing).  We will mitigate this (about 460us to update 4
// leds), but just keeping state here, and having a slower low-priority thread
// actually update.  It will poll basically when things are idle.  This isn't
// good from a power perspective.
// TODO: Can we make this const static (not with the current way this is implemented).

type InfoPair = (Mutex<LedInfo>, Condvar);

struct LedInfo {
    // The values of the LEDs.
    leds: Option<Vec<RGB8>>,
}

fn led_thread(mut all_leds: LedSet, info: Arc<InfoPair>, stats: Arc<Stats>) -> ! {
    let limit = all_leds.len();
    loop {
        let info = get_info(&*info);
        stats.start("led");
        all_leds.update(&info);
        stats.stop("led");
    }
}

// Helper to get the state, waiting for it to be present.
#[inline(never)]
fn get_info(info: &InfoPair) -> Vec<RGB8> {
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
