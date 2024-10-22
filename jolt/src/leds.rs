//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

extern crate alloc;

use alloc::vec::Vec;
use alloc::boxed::Box;
use log::info;
use rgb::RGB8;

pub mod manager;
mod pwm;
mod led_strip;

/// Represents a driver for 1 or more RGB LEDs.
pub trait LedGroup: Send + 'static {
    /// How many RGB LED units are in this grouping.
    fn len(&self) -> usize;

    /// Set the group of LEDs to the given value.
    ///
    /// Sets the leds in this group.  `values.len()` must be equal to what `self.len()` returns.
    fn update(&mut self, values: &[RGB8]);
}

/// Management of a bunch of leds.
pub struct LedSet {
    all: Vec<Box<dyn LedGroup>>,
}

impl LedSet {
    /// Get the total number of LEDs represented by this set.
    pub fn len(&self) -> usize {
        self.all.iter().map(|e| e.len()).sum()
    }

    /// Update all of the LEDs.  The length of values must equal the return of Self::len()
    pub fn update(&mut self, values: &[RGB8]) {
        let mut base = 0;

        for group in &mut self.all {
            let len = group.len();

            group.update(&values[base..base+len]);
            base += len;
        }

        assert_eq!(base, values.len());
    }

    /// Get all instances of LED sets from underlying drivers.
    pub fn get_all() -> LedSet {
        let mut all = Vec::new();
        if let Some(led) = pwm::PwmLed::get_instance() {
            all.push(Box::new(led) as Box<dyn LedGroup>);
        }

        if let Some(leds) = led_strip::LedStripGroup::get_instance() {
            all.push(Box::new(leds) as Box<dyn LedGroup>);
        }

        let result = LedSet { all };
        info!("{} leds found", result.len());
        result
    }
}
