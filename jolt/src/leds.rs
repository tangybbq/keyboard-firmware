//! LED support.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use smart_leds::RGB8;

pub mod manager;

#[cfg(dt = "chosen::bbq_led_strip")]
mod led_strip;
#[cfg(dt = "chosen::bbq_pwm_leds")]
mod pwm;

/// Represents a driver for 1 or more RGB LEDs.
pub trait LedGroup: Send + 'static {
    /// How many RGB LED units are in this grouping.
    fn len(&self) -> usize;

    /// Set the group of LEDs to the given value.
    fn update(&mut self, values: &[RGB8]);
}

/// Management of a bunch of LEDs.
pub struct LedSet {
    all: Vec<Box<dyn LedGroup>>,
}

impl LedSet {
    /// Get the total number of LEDs represented by this set.
    pub fn len(&self) -> usize {
        self.all.iter().map(|group| group.len()).sum()
    }

    /// Update all of the LEDs.
    pub fn update(&mut self, values: &[RGB8]) {
        let mut base = 0;

        for group in &mut self.all {
            let len = group.len();
            group.update(&values[base..base + len]);
            base += len;
        }

        assert_eq!(base, values.len());
    }

    /// Discover all LED backends enabled by the current chosen nodes.
    pub fn get_all() -> Self {
        #[allow(unused_mut)]
        let mut all: Vec<Box<dyn LedGroup>> = Vec::new();

        #[cfg(dt = "chosen::bbq_pwm_leds")]
        if let Some(led) = pwm::PwmLed::get_instance() {
            all.push(Box::new(led));
        }

        #[cfg(dt = "chosen::bbq_led_strip")]
        if let Some(leds) = led_strip::LedStripGroup::get_instance() {
            all.push(Box::new(leds));
        }

        Self { all }
    }
}
