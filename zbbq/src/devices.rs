//! Interface to devices.

extern crate alloc;

use core::ffi::c_int;

use alloc::vec::Vec;
use bitflags::bitflags;

use crate::{Error, Result};

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
    static matrix_rows: [*const gpio_dt_spec; 3];
    static matrix_cols: [*const gpio_dt_spec; 5];

    static side_select: gpio_dt_spec;

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

pub fn get_side_select() -> Pin {
    Pin { spec: unsafe {&side_select}}
}

bitflags! {
    /// The GpioFlags taken from include/zephyr/drivers/gpio.h
    pub struct GpioFlags: u32 {
        const GPIO_INPUT = 1 << 16;
        const GPIO_OUTPUT = 1 << 17;
        const GPIO_OUTPUT_INIT_LOW = 1 << 18;
        const GPIO_OUTPUT_INIT_HIGH = 1 << 19;
        const GPIO_OUTPUT_INIT_LOGICAL = 1 << 20;

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
