//! Board-specific definitions

#[cfg(feature = "proto2")]
mod proto2;
#[cfg(feature = "proto3")]
mod proto3;

#[cfg(feature = "proto2")]
pub use proto2::*;
#[cfg(feature = "proto3")]
pub use proto3::*;

macro_rules! col_pins {
    ($pins:expr, $($pin:ident),*) => {
        [
            $($pins.$pin
              .into_push_pull_output_in_state(PinState::Low)
              .into_dyn_pin()),*
        ]
    };
}
pub(crate) use col_pins;

macro_rules! row_pins {
    ($pins:expr, $($pin:ident),*) => {
        [
            $($pins.$pin
              .into_pull_down_input()
              .into_dyn_pin()),*
        ]
    };
}
pub(crate) use row_pins;
