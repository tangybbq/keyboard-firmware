//! Control of the LEDs.

#![allow(unused_variables)]
#![allow(dead_code)]

extern crate alloc;

use heapless::Vec;
use smart_leds::RGB8;

pub mod led_strip;
pub mod manager;
/*
mod pwm;
*/

/// Max size of a set of LEDs.
const MAX_SET_SIZE: usize = 1;

/// Maximum number of LEDs in a group.
const MAX_GROUP_SIZE: usize = 4;

/// Maximum number of leds total.
pub(crate) const MAX_LEDS: usize = MAX_SET_SIZE * MAX_GROUP_SIZE;

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
    all: Vec<&'static mut dyn LedGroup, MAX_GROUP_SIZE>,
}

impl LedSet {
    /// Construct an LedSet consisting of a single group.
    pub fn new<const M: usize>(groups: [&'static mut dyn LedGroup; M]) -> Self {
        let mut all = Vec::new();
        for elt in groups {
            all.push(elt).ok().unwrap();
        }
        Self { all }
    }

    /// Get the total number of LEDs represented by this set.
    pub fn len(&self) -> usize {
        self.all.iter().map(|e| e.len()).sum()
    }

    /// Update all of the LEDs.  The length of values must equal the return of Self::len()
    pub fn update(&mut self, values: &[RGB8]) {
        let mut base = 0;

        for group in &mut self.all {
            let len = group.len();

            group.update(&values[base..base + len]);
            base += len;
        }

        assert_eq!(base, values.len());
    }

    /*
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
    */
}
