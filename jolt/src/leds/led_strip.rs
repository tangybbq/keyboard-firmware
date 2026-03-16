//! Stub for LED strip support.

use super::LedGroup;

pub struct LedStripGroup;

impl LedGroup for LedStripGroup {
    fn len(&self) -> usize {
        0
    }

    fn update(&mut self, _values: &[smart_leds::RGB8]) {}
}

impl LedStripGroup {
    pub fn get_instance() -> Option<Self> {
        None
    }
}
