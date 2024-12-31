//! Led strip support

// Currently, the rp2040 driver for the WS2812 LEDs is poll based (even though
// it uses PIO for writing).  We will mitigate this (about 460us to update 4
// leds), but just keeping state here, and having a slower low-priority thread
// actually update.  It will poll basically when things are idle.  This isn't
// good from a power perspective.
// TODO: Can we make this const static (not with the current way this is implemented).

extern crate alloc;

use alloc::vec::Vec;

use super::LedGroup;
use zephyr::{device::led_strip::LedStrip, raw::led_rgb};

pub struct LedStripGroup {
    // Underlying device.
    strip: LedStrip,
}

impl LedGroup for LedStripGroup {
    fn len(&self) -> usize {
        self.strip.chain_len()
    }

    // Note that this does an allocation/free.
    fn update(&mut self, values: &[rgb::RGB8]) {
        let leds: Vec<_> = values
            .iter()
            .map(|led| led_rgb {
                r: led.r,
                g: led.g,
                b: led.b,
            })
            .collect();
        unsafe { self.strip.update(&leds).unwrap() }
    }
}

impl LedStripGroup {
    #[cfg(dt = "chosen::bbq_led_strip")]
    pub fn get_instance() -> Option<LedStripGroup> {
        let leds = zephyr::devicetree::chosen::bbq_led_strip::get_instance()?;
        if leds.chain_len() >= 1 {
            Some(LedStripGroup { strip: leds })
        } else {
            None
        }
    }

    #[cfg(not(dt = "chosen::bbq_led_strip"))]
    pub fn get_instance() -> Option<LedStripGroup> {
        None
    }
}
