//! Control of the LEDs.

use core::iter::once;

use smart_leds::{SmartLedsWrite, RGB8};

const OFF: RGB8 = RGB8::new(0, 0, 0);
// const INIT: RGB8 = RGB8::new(8, 8, 0);

struct Step {
    color: RGB8,
    count: usize,
}

static INIT_INDICATOR: &[Step] = &[
    Step { color: RGB8::new(8, 0, 0), count: 100 },
    Step { color: RGB8::new(0, 8, 0), count: 100 },
    Step { color: RGB8::new(0, 0, 8), count: 100 },
    Step { color: OFF,                count: 300 },
];

pub struct LedManager<'a, L: SmartLedsWrite<Color = RGB8>> {
    leds: &'a mut L,

    steps: &'static [Step],
    count: usize,
    phase: usize,
}

impl<'a, L: SmartLedsWrite<Color = RGB8>> LedManager<'a, L> {
    pub fn new(leds: &'a mut L) -> Self {
        LedManager {
            leds,
            steps: INIT_INDICATOR,
            count: 0,
            phase: 0,
        }
    }

    pub fn tick(&mut self) {
        if self.count == 0 {
            if self.phase >= self.steps.len() {
                self.phase = 0;
            }

            let _ = self.leds.write(once(self.steps[self.phase].color));
            // let _ = self.leds.write(once(if self.phase { INIT } else { OFF }));
            self.count = self.steps[self.phase].count;
            self.phase += 1;
        } else {
            self.count -= 1;
        }
    }
}
