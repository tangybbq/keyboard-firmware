//! Definitions for the proto3 board.

pub const NCOLS: usize = 6;
pub const NROWS: usize = 4;
pub const NKEYS: usize = NCOLS * NROWS;

macro_rules! cols {
    ($pins:expr) => {
        crate::board::col_pins!($pins, gpio2, gpio3, gpio4, gpio5, gpio6, gpio7)
    };
}
pub(crate) use cols;

macro_rules! rows {
    ($pins:expr) => {
        crate::board::row_pins!($pins, adc3, adc2, adc1, adc0)
    };
}
pub(crate) use rows;

/// Side select GPIO pin.
macro_rules! side_pin {
    ($pins:expr) => {
        $pins.sck.into_pull_down_input()
    };
}
pub(crate) use side_pin;
