//! Interface to devices.

extern crate alloc;

use core::ffi::{c_int, CStr};

use alloc::vec::Vec;
use bitflags::bitflags;

use crate::{Error, Result, EVENT_QUEUE};

use bbq_keyboard::{UsbDeviceState, Event};

#[allow(non_camel_case_types)]
type gpio_pin_t = u8;
#[allow(non_camel_case_types)]
type gpio_dt_flags_t = u16;

#[allow(non_camel_case_types)]
#[repr(C)]
struct struct_device {
    // Not the real structure, just a placeholder.
    _placeholder: u32,
}

#[allow(non_camel_case_types)]
#[repr(C)]
struct gpio_dt_spec {
    port: *const struct_device,
    pin: gpio_pin_t,
    dt_flags: gpio_dt_flags_t,
}

// Our higher level representation of a single GPIO.
// In our case, the pins are defined statically in the C code, so the static
// reference is appropriate.
pub struct Pin {
    spec: &'static gpio_dt_spec,
}

// A Pin matrix.
pub struct PinMatrix {
    pub rows: Vec<Pin>,
    pub cols: Vec<Pin>,
}

extern "C" {
    static n_matrix_cols: u32;
    static n_matrix_rows: u32;
    static matrix_rows: [*const gpio_dt_spec; 16];
    static matrix_cols: [*const gpio_dt_spec; 16];

    static matrix_reverse: u32;
    static matrix_translate: *const i8;

    fn c_get_side_select() -> *const gpio_dt_spec;

    fn sys_gpio_pin_configure(port: *const struct_device,
                              pin: gpio_pin_t,
                              flags:u32) -> c_int;
    fn sys_gpio_pin_set(port: *const struct_device, pin: gpio_pin_t, value: c_int) -> c_int;
    fn sys_gpio_pin_get(port: *const struct_device, pin: gpio_pin_t) -> c_int;
}

impl Pin {
    pub fn pin_configure(&self, flags: GpioFlags) -> Result<()> {
        if unsafe {
            sys_gpio_pin_configure(self.spec.port, self.spec.pin,
                                   self.spec.dt_flags as u32 | flags.bits())
        } != 0 {
            return Err(Error::GPIO);
        }
        Ok(())
    }

    pub fn pin_get(&self) -> Result<bool> {
        match unsafe {sys_gpio_pin_get(self.spec.port, self.spec.pin)} {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::GPIO),
        }
    }

    // TODO: This really should be `mut`, but that isn't how the C API is written.
    pub fn pin_set(&self, value: bool) -> Result<()> {
        match unsafe {sys_gpio_pin_set(self.spec.port,
                                       self.spec.pin,
                                       if value { 1 } else { 0 })}
        {
            0 => Ok(()),
            _ => Err(Error::GPIO),
        }
    }
}

impl PinMatrix {
    /// Get the Pin matrix from the C defines.
    pub fn get() -> PinMatrix {
        let rows: Vec<_> =
            unsafe {&matrix_rows[..n_matrix_rows as usize]}
        .iter()
            .map(|g| Pin { spec: unsafe {&**g} })
            .collect();
        let cols: Vec<_> =
            unsafe {&matrix_cols[..n_matrix_cols as usize]}
            .iter()
            .map(|g| Pin { spec: unsafe {&**g} })
            .collect();
        PinMatrix {rows, cols}
    }
}

pub fn get_matrix_reverse() -> bool {
    unsafe { matrix_reverse != 0 }
}

pub fn get_matrix_translate() -> Option<&'static str> {
    if unsafe { matrix_translate.is_null() } {
        None
    } else {
        Some(unsafe { CStr::from_ptr(matrix_translate).to_str().unwrap() })
    }
}

pub fn get_side_select() -> Option<Pin> {
    let raw = unsafe {c_get_side_select()};
    if raw.is_null() {
        None
    } else {
        Some(Pin {
            spec: unsafe {&*raw},
        })
    }
}

bitflags! {
    /// The GpioFlags taken from include/zephyr/drivers/gpio.h
    pub struct GpioFlags: u32 {
        const GPIO_INPUT = 1 << 16;
        const GPIO_OUTPUT = 1 << 17;
        const GPIO_OUTPUT_INIT_LOW = 1 << 18;
        const GPIO_OUTPUT_INIT_HIGH = 1 << 19;
        const GPIO_OUTPUT_INIT_LOGICAL = 1 << 20;
        const GPIO_PULL_UP = 1 << 4;
        const GPIO_PULL_DOWN = 1 << 5;

        // Some of these are zero, which the bitflags docs suggests might
        // confuse it.
        const GPIO_SINGLE_ENDED = 1 << 1;
        const GPIO_PUSH_PULL = 0 << 1;
        const GPIO_LINE_OPEN_DRAIN = 1 << 2;
        const GPIO_LINE_OPEN_SOURCE = 0 << 2;

        const GPIO_OPEN_DRAIN = Self::GPIO_SINGLE_ENDED.bits() | Self::GPIO_LINE_OPEN_DRAIN.bits();
        const GPIO_OPEN_SOURCE = Self::GPIO_SINGLE_ENDED.bits() | Self::GPIO_LINE_OPEN_SOURCE.bits();

        const GPIO_OUTPUT_LOW = Self::GPIO_OUTPUT.bits() | Self::GPIO_OUTPUT_INIT_LOW.bits();
        const GPIO_OUTPUT_HIGH = Self::GPIO_OUTPUT.bits() | Self::GPIO_OUTPUT_INIT_HIGH.bits();
        const GPIO_OUTPUT_INACTIVE =
            (Self::GPIO_OUTPUT.bits() |
             Self::GPIO_OUTPUT_INIT_LOW.bits() |
             Self::GPIO_OUTPUT_INIT_LOGICAL.bits());
        const GPIO_OUTPUT_ACTIVE =
            (Self::GPIO_OUTPUT.bits() |
             Self::GPIO_OUTPUT_INIT_HIGH.bits() |
             Self::GPIO_OUTPUT_INIT_LOGICAL.bits());

        // TODO: Add the interrupt signals
    }
}

/// The USB HID interface is primarily handled with C code, with just a simple
/// API to report.  Therefore this struct is very simple.
///
/// The first entry just queries if there is space in the USB stack for a single
/// report.
pub fn hid_is_accepting() -> bool {
    (unsafe {is_hid_accepting()}) != 0
}

/// Send a single report via HID.  The modifiers are bits, and there can be up
/// to 6 scancodes in the report.  If `hid_is_accepting()` didn't return true,
/// this might block.
pub fn hid_send_keyboard_report(mods: u8, keys: &[u8]) {
    if keys.len() > 6 {
        panic!("USB HID boot keyboard doesn't support more than 6 keys");
    }

    let mut report = [0u8; 8];
    report[0] = mods;
    for (i, key) in keys.iter().enumerate() {
        report[i+2] = *key;
    }
    unsafe {hid_report(report.as_ptr())};
}

extern "C" {
    fn is_hid_accepting() -> c_int;
    fn hid_report(report: *const u8);
}

/// Report on a USB status change.  This is a C callback, possibly from IRQ context.
#[no_mangle]
pub extern "C" fn rust_usb_status(state: u32) {
    let devstate = match state {
        0 => UsbDeviceState::Configured,
        1 => UsbDeviceState::Suspend,
        _ => return,
    };

    EVENT_QUEUE.push(Event::UsbState(devstate));
}

pub mod leds {
    use core::ffi::c_int;

    use super::struct_device;
    use super::Result;
    use super::Error;

    // The RGB API is straightforward.
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct LedRgb {
        #[cfg(CONFIG_LED_STRIP_RGB_SCRATCH)]
        scratch: u8,
        pub r: u8,
        pub g: u8,
        pub b: u8,
    }

    impl LedRgb {
        pub const fn new(r: u8, g: u8, b: u8) -> LedRgb {
            LedRgb { r, g, b }
        }
    }

    extern "C" {
        static strip_length: u32;
        static strip: *const struct_device;

        fn sys_led_strip_update_rgb(dev: *const struct_device,
                                    pixels: *const LedRgb,
                                    num_pixels: usize) -> c_int;
    }

    pub struct LedStrip {
        device: *const struct_device,
        #[allow(dead_code)]
        pixels: usize,
    }

    impl LedStrip {
        pub fn get() -> LedStrip {
            LedStrip {
                device: unsafe { strip },
                pixels: unsafe { strip_length } as usize,
            }
        }

        #[allow(dead_code)]
        pub fn pixel_count(&self) -> usize {
            self.pixels
        }

        pub fn update(&self, pixels: &[LedRgb]) -> Result<()> {
            match unsafe { sys_led_strip_update_rgb(self.device,
                                                    pixels.as_ptr(),
                                                    pixels.len()) }
            {
                0 => Ok(()),
                _ => Err(Error::LED),
            }
        }
    }
}
