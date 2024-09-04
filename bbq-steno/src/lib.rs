//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

pub mod dict;
pub mod memdict;
pub mod stroke;
pub mod typer;

pub use stroke::Stroke;

#[cfg(test)]
mod testlog;

#[cfg(test)]
mod log {
    #[allow(unused_imports)]
    pub use log::warn;
}

#[cfg(not(test))]
mod log {
    // pub use defmt::warn;
}

#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! println {
    ($msg:expr) => { {} };
    ($msg:expr, $($_arg:expr),+) => { {} };
}
