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
pub static OFF_INDICATOR: Indication = Indication(&[
    Step { color: OFF,                count: 10000 },
]);

/// Steno mode
pub static STENO_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 0, 32), count: 10000 },
]);

/// Artsey mode
pub static ARTSEY_INDICATOR: Indication = Indication(&[
    Step { color: RGB8::new(0, 16, 0), count: 10000 },
]);

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
            steps: INIT_INDICATOR.0,
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

    /// Set a global indicator. This will override any other status being
    /// displayed, and usually indicates either an error, or an initial
    /// condition. It also usually indicates that the keyboard can't be used
    /// yet.
    pub fn set_global(&mut self, indicator: &Indication) {
        self.steps = indicator.0;
        self.count = 0;
        self.phase = 0;
    }
}
