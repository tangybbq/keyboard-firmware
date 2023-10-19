//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

use arraydeque::ArrayDeque;
use bbq_steno::Stroke;
use usbd_human_interface_device::page::Keyboard;
use usb_device::prelude::UsbDeviceState;

pub use layout::LayoutMode;

pub mod serialize;
pub mod modifiers;
pub mod usb_typer;
pub mod layout;

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

use log::*;

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

/// An event is something that happens in a handler to indicate some action
/// likely needs to be performed on it.
pub enum Event {
    /// Events from the Matrix layer indicating changes in key actions.
    Matrix(KeyEvent),

    /// Events from the inner layer indicating changes in key actions.
    InterKey(KeyEvent),

    /// Indication of a "raw" steno stroke from the steno layer.  This is
    /// untranslated and should just be typed.
    RawSteno(Stroke),

    /// Change in USB status.
    UsbState(UsbDeviceState),

    /// Indicates that the inner channel has determined we are secondary.
    BecomeState(InterState),

    /// Got heartbeat from secondary
    Heartbeat,

    /// Major mode indication change.
    Mode(LayoutMode),
}

pub struct EventQueue(ArrayDeque<Event, 256>);

impl EventQueue {
    pub fn new() -> Self {
        EventQueue(ArrayDeque::new())
    }

    pub fn push(&mut self, event: Event) {
        if let Err(_) = self.0.push_back(event) {
            warn!("Internal event queue overflow");
        }
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.0.pop_front()
    }
}

/// State of inter communication.
#[derive(Eq, PartialEq, Clone, Copy)]
pub enum InterState {
    Idle,
    Primary,
    Secondary,
}
