//! PWM-backed RGB LED groups.

use zephyr::device::led::Led;

use super::LedGroup;

pub struct PwmLed {
    red: Led,
    green: Led,
    blue: Led,
}

impl LedGroup for PwmLed {
    fn len(&self) -> usize {
        1
    }

    fn update(&mut self, values: &[smart_leds::RGB8]) {
        if let Some(value) = values.first() {
            self.red.set_brightness(scale(value.r)).unwrap();
            self.green.set_brightness(scale(value.g)).unwrap();
            self.blue.set_brightness(scale(value.b)).unwrap();
        }
    }
}

impl PwmLed {
    #[cfg(all(
        dt = "aliases::pwm_led0",
        dt = "aliases::pwm_led1",
        dt = "aliases::pwm_led2"
    ))]
    pub fn get_instance() -> Option<Self> {
        Some(Self {
            red: zephyr::devicetree::aliases::pwm_led0::get_instance()?,
            green: zephyr::devicetree::aliases::pwm_led1::get_instance()?,
            blue: zephyr::devicetree::aliases::pwm_led2::get_instance()?,
        })
    }

    #[cfg(not(all(
        dt = "aliases::pwm_led0",
        dt = "aliases::pwm_led1",
        dt = "aliases::pwm_led2"
    )))]
    pub fn get_instance() -> Option<Self> {
        None
    }
}

fn scale(value: u8) -> u8 {
    (((value as u32) * 100) >> 8) as u8
}
