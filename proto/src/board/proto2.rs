//! Proto2 definitions.

pub const NCOLS: usize = 5;
pub const NROWS: usize = 3;
pub const NKEYS: usize = NCOLS * NROWS;

macro_rules! cols {
    ($pins:expr) => {
        crate::board::col_pins!($pins, gpio2, gpio3, gpio4, gpio5, gpio6)
    };
}
pub(crate) use cols;

macro_rules! rows {
    ($pins:expr) => {
        crate::board::row_pins!($pins, gpio7, adc0, sck)
    };
}
pub(crate) use rows;

/// Side select GPIO pin.
macro_rules! side_pin {
    ($pins:expr) => {
        $pins.adc1.into_pull_down_input()
    };
}
pub(crate) use side_pin;
