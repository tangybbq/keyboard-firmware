//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

extern crate alloc;

use alloc::vec::Vec;

use bbq_steno::Stroke;
use usbd_human_interface_device::page::Keyboard;
use usb_device::prelude::UsbDeviceState;
use bitflags::bitflags;

pub use layout::LayoutMode;

pub mod dict;
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

impl defmt::Format for KeyEvent {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            KeyEvent::Press(k) => defmt::write!(fmt, "KeyEvent::Press({})", k),
            KeyEvent::Release(k) => defmt::write!(fmt, "KeyEvent::Release({})", k),
        }
    }
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

    pub fn is_release(&self) -> bool {
        match self {
            KeyEvent::Press(_) => false,
            KeyEvent::Release(_) => true,
        }
    }
}

/// Indicates keypress that should be sent to the host.
#[derive(Clone, Debug)]
pub enum KeyAction {
    KeyPress(Keyboard, Mods),
    ModOnly(Mods),
    KeyRelease,
    KeySet(Vec<Keyboard>),
}

bitflags! {
    /// A modifier map. This indicates what modifiers should be held down when
    /// this keypress is sent.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
    pub struct Mods: u8 {
        const SHIFT = 0b0000_0001;
        const CONTROL = 0b0000_0010;
        const ALT = 0b0000_0100;
        const GUI = 0b0000_1000;
    }
}

/// An event is something that happens in a handler to indicate some action
/// likely needs to be performed on it.
#[derive(Debug)]
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

    /// We are doing a mode select, with the given mode being the next mode.
    ModeSelect(LayoutMode),

    /// A keypress (from a typical keyboard mode)
    Key(KeyAction),

    /// Set indicator to given mode.
    Indicator(MinorMode),
}

/// A generalized event queue.  TODO: Handle the error better.  For now, we
/// don't do anything with the error, so might as well.
pub trait EventQueue {
    // Attempt to push to the queue.  Events will be discarded if the queue is full.
    fn push(&mut self, val: Event);
    // This is not currently supported, but could be with async-trait.
    // async fn send(&mut self, val: Event) -> Result<(), ()>;
}

/// State of inter communication.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum InterState {
    Idle,
    Primary,
    Secondary,
}

impl defmt::Format for InterState {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            InterState::Idle => defmt::write!(fmt, "idle"),
            InterState::Primary => defmt::write!(fmt, "primary"),
            InterState::Secondary => defmt::write!(fmt, "secondary"),
        }
    }
}

#[derive(Debug)]
pub enum MinorMode {
    // To start with, just distinguish artsy main from artsy nav mode.
    ArtseyMain,
    ArtseyNav,
}

/// Something we can use to get time.
pub trait Timable {
    fn get_ticks(&self) -> u64;
}
