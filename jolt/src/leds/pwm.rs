//! Control of the LEDs.

use zephyr::device::led::Leds;

use super::LedGroup;

pub struct PwmLed {
    /// Underlying device.
    leds: Leds,
}

unsafe impl Send for PwmLed {}

impl LedGroup for PwmLed {
    fn len(&self) -> usize {
        self.leds.len() / 3
    }

    fn update(&mut self, values: &[rgb::RGB8]) {
        for (i, value) in values.iter().enumerate() {
            let base = 3 * i;
            unsafe {
                let value = (((values[0].r as u32) * 100) >> 8) as u8;
                self.leds.set_brightness(base + 0, value).unwrap();
                let value = (((values[0].g as u32) * 100) >> 8) as u8;
                self.leds.set_brightness(base + 1, value).unwrap();
                let value = (((values[0].b as u32) * 100) >> 8) as u8;
                self.leds.set_brightness(base + 2, value).unwrap();
            }
        }
    }
}

impl PwmLed {
    pub fn get_instance() -> Option<PwmLed> {
        // TODO: Use a chosen and be conditional.
        let leds = zephyr::devicetree::pwm_leds::get_instance()?;
        // Require at least 3 LEDs.
        if leds.len() >= 3 {
            Some(PwmLed { leds })
        } else {
            None
        }
    }
}
