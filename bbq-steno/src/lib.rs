//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

pub mod stroke;
pub mod memdict;
pub mod dict;

pub use stroke::Stroke;

#[cfg(test)]
mod testlog;

#[cfg(test)]
mod log {
    pub use log::warn;
}

#[cfg(not(test))]
mod log {
    pub use defmt::warn;
}
