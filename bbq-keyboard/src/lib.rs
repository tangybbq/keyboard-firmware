//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

use usbd_human_interface_device::page::Keyboard;

pub mod serialize;
pub mod modifiers;

#[cfg(test)]
mod testlog;

#[cfg(test)]
mod log {
    pub use log::warn;
    pub use log::info;
}

#[cfg(not(test))]
mod log {
    pub use defmt::info;
    pub use defmt::warn;
}

/// Which side of the keyboard are we.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn is_left(&self) -> bool {
        match *self {
            Side::Left => true,
            Side::Right => false,
        }
    }
}

/// Key events indicate keys going up or down.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum KeyEvent {
    Press(u8),
    Release(u8),
}

impl KeyEvent {
    pub fn key(&self) -> u8 {
        match self {
            KeyEvent::Press(k) => *k,
            KeyEvent::Release(k) => *k,
        }
    }

    pub fn is_press(&self) -> bool {
        match self {
            KeyEvent::Press(_) => true,
            KeyEvent::Release(_) => false,
        }
    }
}

/// Indicates keypress that should be sent to the host.
#[derive(Clone)]
pub enum KeyAction {
    KeyPress(Keyboard),
    ShiftedKeyPress(Keyboard),
    KeyRelease,
}
