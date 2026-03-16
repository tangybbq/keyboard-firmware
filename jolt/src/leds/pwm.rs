//! PWM-backed RGB LED groups.

use zephyr::device::led::Leds;

use super::LedGroup;

pub struct PwmLed {
    leds: Leds,
}

impl LedGroup for PwmLed {
    fn len(&self) -> usize {
        self.leds.len() / 3
    }

    fn update(&mut self, values: &[smart_leds::RGB8]) {
        for (index, value) in values.iter().enumerate() {
            let base = 3 * index;
            unsafe {
                self.leds
                    .set_brightness(base, scale(value.r))
                    .unwrap();
                self.leds
                    .set_brightness(base + 1, scale(value.g))
                    .unwrap();
                self.leds
                    .set_brightness(base + 2, scale(value.b))
                    .unwrap();
            }
        }
    }
}

impl PwmLed {
    pub fn get_instance() -> Option<Self> {
        let leds = zephyr::devicetree::chosen::bbq_pwm_leds::get_instance()?;
        if leds.len() >= 3 {
            Some(Self { leds })
        } else {
            None
        }
    }
}

fn scale(value: u8) -> u8 {
    (((value as u32) * 100) >> 8) as u8
}
