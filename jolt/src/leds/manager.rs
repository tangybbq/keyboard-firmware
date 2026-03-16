//! LED indication management.

extern crate alloc;

use alloc::vec::Vec;

use bbq_steno::dict::State;
use smart_leds::RGB8;

use super::LedSet;

const OFF: RGB8 = RGB8::new(0, 0, 0);

pub struct Indication(&'static [Step]);

struct Step {
    color: RGB8,
    count: usize,
}

pub static INIT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(8, 0, 0), count: 1 },
    Step { color: RGB8::new(0, 8, 0), count: 1 },
    Step { color: RGB8::new(0, 0, 8), count: 1 },
    Step { color: OFF, count: 3 },
]);

pub static UNDEF_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(1, 1, 1), count: 1 },
    Step { color: OFF, count: 30 },
]);

pub static OFF_INDICATOR: Indication = Indication(&[Step {
    color: OFF,
    count: 10,
}]);

pub static STENO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 0, 24),
    count: 100,
}]);

pub static STENO_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 0, 24), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static STENO_RAW_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 0),
    count: 100,
}]);

pub static STENO_RAW_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(16, 8, 0), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static STENO_DIRECT_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 0),
    count: 100,
}]);

pub static STENO_DIRECT_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(16, 8, 0), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static NKRO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(32, 0, 32),
    count: 100,
}]);

pub static NKRO_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(32, 0, 32), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static ARTSEY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 0, 0),
    count: 100,
}]);

pub static ARTSEY_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(16, 0, 0), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static TAIPO_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(16, 8, 24),
    count: 100,
}]);

pub static TAIPO_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(16, 8, 24), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static QWERTY_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 16, 0),
    count: 100,
}]);

pub static QWERTY_SELECT_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 16, 0), count: 1 },
    Step { color: OFF, count: 1 },
]);

pub static ARTSEY_NAV_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(20, 20, 0),
    count: 100,
}]);

pub static STENO_NOSPACE_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(20, 0, 0),
    count: 100,
}]);

pub static STENO_NOSPACE_CAP_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(20, 20, 0),
    count: 100,
}]);

pub static STENO_CAP_INDICATOR: Indication = Indication(&[Step {
    color: RGB8::new(0, 20, 0),
    count: 100,
}]);

pub fn get_steno_state(state: &State) -> &'static Indication {
    let space = state.space || state.force_space || state.stitch;
    if !state.cap && space {
        &OFF_INDICATOR
    } else if !state.cap && !space {
        &STENO_NOSPACE_INDICATOR
    } else if state.cap && space {
        &STENO_CAP_INDICATOR
    } else {
        &STENO_NOSPACE_CAP_INDICATOR
    }
}

pub struct LedManager {
    leds: LedSet,
    states: Vec<LedState>,
}

struct LedState {
    base: &'static [Step],
    global: Option<&'static [Step]>,
    oneshot: Option<&'static [Step]>,
    count: usize,
    phase: usize,
    last_color: RGB8,
}

impl LedManager {
    pub fn new(leds: LedSet) -> Self {
        let len = leds.len();
        let states = (0..len)
            .map(|index| {
                if index == 0 {
                    LedState::new(UNDEF_INDICATOR.0, Some(INIT_INDICATOR.0))
                } else {
                    LedState::new(UNDEF_INDICATOR.0, None)
                }
            })
            .collect();

        Self { leds, states }
    }

    pub fn clear_global(&mut self, index: usize) {
        if let Some(state) = self.states.get_mut(index) {
            state.clear_global();
        }
    }

    pub fn set_base(&mut self, index: usize, indicator: &Indication) {
        if let Some(state) = self.states.get_mut(index) {
            state.set_base(indicator);
        }
    }

    /// Update the LEDs based on the active state machines.
    pub fn tick(&mut self) {
        let colors: Vec<_> = self.states.iter_mut().map(|state| state.tick()).collect();
        self.leds.update(&colors);
    }
}

impl LedState {
    fn new(base: &'static [Step], global: Option<&'static [Step]>) -> Self {
        Self {
            base,
            global,
            oneshot: None,
            count: 0,
            phase: 0,
            last_color: OFF,
        }
    }

    fn tick(&mut self) -> RGB8 {
        let mut steps = self.base;
        if let Some(global) = self.global {
            steps = global;
        }
        if let Some(oneshot) = self.oneshot {
            steps = oneshot;
        }

        if self.count == 0 {
            if self.phase >= steps.len() {
                self.phase = 0;

                if self.oneshot.is_some() {
                    self.oneshot = None;
                    self.phase = 1000;
                    return self.tick();
                }
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
