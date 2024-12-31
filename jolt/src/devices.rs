//! Device management
//!
//! Management of the various devices used in the keyboards.  Some are just direct types from
//! Zephyr, and others are wrapped.

pub mod usb;

pub mod leds {
    use bbq_keyboard::RGB8;
    use zephyr::raw::led_rgb;

    // Wrap the Zephyr rgb indicator.
    #[derive(Copy, Clone)]
    pub struct LedRgb(pub led_rgb);

    // TODO: There might be an additional field depend on configs.
    impl Default for LedRgb {
        fn default() -> Self {
            LedRgb::new(0, 0, 0)
        }
    }

    impl LedRgb {
        pub const fn new(r: u8, g: u8, b: u8) -> LedRgb {
            LedRgb(led_rgb { r, g, b })
        }

        pub fn to_rgb8(self) -> RGB8 {
            RGB8::new(self.0.r, self.0.g, self.0.b)
        }
    }
}
