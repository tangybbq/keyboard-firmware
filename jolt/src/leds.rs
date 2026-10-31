//! LED support.

extern crate alloc;

use alloc::{boxed::Box, vec::Vec};
use smart_leds::RGB8;

pub mod manager;

#[cfg(dt = "chosen::bbq_led_strip")]
mod led_strip;
#[cfg(dt = "chosen::bbq_mono_leds")]
mod mono;
#[cfg(dt = "chosen::bbq_pwm_leds")]
mod pwm;

/// What one LED shows, for each kind of LED that might be showing it.
///
/// The manager decides what each LED shows without knowing what kind of LED it is, so it gives
/// every kind its own form of the same thing, and each group takes the one it can display.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shade {
    /// On an RGB LED.
    pub color: RGB8,
    /// On a single-color LED, as a brightness in percent, `0..=100`.
    pub brightness: u8,
}

/// Represents a driver for 1 or more LEDs.
pub trait LedGroup: Send + 'static {
    /// How many LED units are in this grouping.
    fn len(&self) -> usize;

    /// Set the group of LEDs to the given value.
    fn update(&mut self, values: &[Shade]);

    /// Whether the LEDs have to be rewritten now and then with what they already show.
    ///
    /// A driver whose hardware holds its state (PWM, say) doesn't; a ws2812 that took a bad bit
    /// holds it until it is written again.
    fn needs_refresh(&self) -> bool {
        false
    }
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

    /// Whether any group needs [`LedGroup::needs_refresh`].
    pub fn needs_refresh(&self) -> bool {
        self.all.iter().any(|group| group.needs_refresh())
    }

    /// Update all of the LEDs.
    pub fn update(&mut self, values: &[Shade]) {
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

        #[cfg(dt = "chosen::bbq_mono_leds")]
        if let Some(leds) = mono::MonoLeds::get_instance() {
            all.push(Box::new(leds));
        }

        Self { all }
    }
}
