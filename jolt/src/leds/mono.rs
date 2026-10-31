//! Single-color PWM LEDs, one per indicator slot.
//!
//! The chosen `bbq,mono-leds` is a `pwm-leds` node whose children, `led-0` to `led-3`, are the
//! board's LEDs in slot order.  The names are fixed because the devicetree bindings give no way
//! to walk a node's children, so a board with some other number of LEDs needs this changed.
//! Each LED shows a [`Shade`]'s brightness; the PWM peripheral holds its duty cycle, so nothing
//! has to be rewritten.

use zephyr::{device::led::Led, printkln};

use super::{LedGroup, Shade};

pub struct MonoLeds {
    leds: [Led; 4],
}

impl LedGroup for MonoLeds {
    fn len(&self) -> usize {
        self.leds.len()
    }

    fn update(&mut self, values: &[Shade]) {
        for (led, shade) in self.leds.iter_mut().zip(values) {
            if let Err(err) = led.set_brightness(shade.brightness) {
                printkln!("LED brightness failed: {:?}", err);
            }
        }
    }
}

impl MonoLeds {
    /// Find the chosen `bbq,mono-leds`.
    pub fn get_instance() -> Option<Self> {
        use zephyr::devicetree::chosen::bbq_mono_leds as node;

        let leds = [
            node::led_0::get_instance()?,
            node::led_1::get_instance()?,
            node::led_2::get_instance()?,
            node::led_3::get_instance()?,
        ];
        if !leds.iter().all(|led| led.is_ready()) {
            printkln!("Mono LEDs are not ready");
            return None;
        }
        Some(Self { leds })
    }
}
