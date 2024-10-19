//! Control of the LEDs.

use super::LedGroup;

pub struct PwmLed {
    /// Number of LEDs managed.  There should be 3x this many LEDs in the device.
    count: usize,
    /// Underlying device.
    dev: *const zephyr::raw::device,
}

unsafe impl Send for PwmLed {}

impl LedGroup for PwmLed {
    fn len(&self) -> usize {
        self.count
    }

    fn update(&mut self, values: &[rgb::RGB8]) {
        for (i, value) in values.iter().enumerate() {
            let base = 3 * i;
            unsafe {
                let value = (((values[0].r as u32) * 100) >> 8) as u8;
                pwm_set_brightness(self.dev, (base + 0) as u32, value);
                let value = (((values[0].g as u32) * 100) >> 8) as u8;
                pwm_set_brightness(self.dev, (base + 1) as u32, value);
                let value = (((values[0].b as u32) * 100) >> 8) as u8;
                pwm_set_brightness(self.dev, (base + 2) as u32, value);
            }
        }
    }
}

impl PwmLed {
    pub fn get_instance() -> Option<PwmLed> {
        let leds = unsafe { get_pwm() };
        let count = leds.count / 3;
        if count > 0 {
            Some(PwmLed {
                count: count as usize,
                dev: leds.dev,
            })
        } else {
            None
        }
    }
}

#[repr(C)]
#[allow(non_camel_case_types)]
struct pwm_led_info {
    dev: *const zephyr::raw::device,
    count: u32,
}

extern "C" {
    fn get_pwm() -> pwm_led_info;
    fn pwm_set_brightness(dev: *const zephyr::raw::device,
                          index: u32,
                          value: u8)
        -> core::ffi::c_int;
}
