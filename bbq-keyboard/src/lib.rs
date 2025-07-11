//! BBQ keyboard series

#![cfg_attr(not(any(feature = "std", test)), no_std)]
// #![deny(missing_docs)]

#[cfg(not(any(feature = "std", test)))]
extern crate core as std;

extern crate alloc;

use alloc::vec::Vec;

use enumset::EnumSetType;
use minicbor::{Decode, Encode};
pub use smart_leds::RGB8;
pub use usbd_human_interface_device::page::Keyboard;
use bitflags::bitflags;

pub use layout::LayoutMode;

pub mod dict;
pub mod boardinfo;
pub mod keys;
pub mod ser2;
pub mod serialize;
pub mod modifiers;
pub mod usb_typer;
pub mod layout;

#[cfg(feature = "std")]
use clap::ValueEnum;

#[cfg(test)]
mod testlog;

#[cfg(not(any(feature = "defmt", feature = "log")))]
compile_error!("One of defmt or log must be selected");

#[cfg(not(feature = "defmt"))]
mod log {
    pub use log::warn;
    pub use log::info;
}

#[cfg(feature = "defmt")]
mod log {
    pub use defmt::info;
    pub use defmt::warn;
}

/// Which side of the keyboard are we.
#[derive(Eq, PartialEq, Clone, Copy, Debug, Encode, Decode)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg_attr(feature = "std", derive(ValueEnum))]
#[cbor(index_only)]
pub enum Side {
    #[n(0)]
    Left,
    #[n(1)]
    Right,
}

impl Side {
    pub fn is_left(&self) -> bool {
        match *self {
            Side::Left => true,
            Side::Right => false,
        }
    }

    /// Return an index of the side, with "left" being zero.
    pub fn index(&self) -> usize {
        match *self {
            Side::Left => 0,
            Side::Right => 1,
        }
    }
}

/// Key events indicate keys going up or down.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum KeyEvent {
    Press(u8),
    Release(u8),
}

#[cfg(feature = "defmt")]
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

    // We use 255 for invalid keys.
    pub fn is_valid(&self) -> bool {
        match self {
            KeyEvent::Press(k) => *k < 255,
            KeyEvent::Release(k) => *k < 255,
        }
    }
}

/// Indicates keypress that should be sent to the host.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum KeyAction {
    KeyPress(Keyboard, Mods),
    ModOnly(Mods),
    KeyRelease,
    KeySet(Vec<Keyboard>),
    Stall,
}

bitflags! {
    /// A modifier map. This indicates what modifiers should be held down when
    /// this keypress is sent.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
    pub struct Mods: u8 {
        const CONTROL = 0b0000_0001;
        const SHIFT = 0b0000_0010;
        const ALT = 0b0000_0100;
        const GUI = 0b0000_1000;
    }
}

// Grumble.
#[cfg(feature = "defmt")]
impl defmt::Format for Mods {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "Mods:{=u8:02x}", self.bits());
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

    /// Change in USB status.
    UsbState(UsbDeviceState),

    /// Indicates that the inner channel has determined we are secondary.
    BecomeState(InterState),

    /// Got heartbeat from secondary
    Heartbeat,

    /// Message back from the layout code that steno raw mode is enabled.
    RawMode(bool),

    /// Message received from the primary side to set out LEDs.
    RecvLed(RGB8),

    /// Led value to be sent to the other side.
    SendLed(RGB8),

    /// Update the steno mode state.
    StenoState(bbq_steno::dict::State),

    /// Tick.  Happens every 1 ms.
    Tick,
}

/// Instead of the usb-device crate's UsbDeviceState, add our own, as the one in
/// the crate is missing some important events.
#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum UsbDeviceState {
    Default,
    Addressed,
    Configured,
    Suspend,
    Resume,
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

#[cfg(feature = "defmt")]
impl defmt::Format for InterState {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            InterState::Idle => defmt::write!(fmt, "idle"),
            InterState::Primary => defmt::write!(fmt, "primary"),
            InterState::Secondary => defmt::write!(fmt, "secondary"),
        }
    }
}

#[derive(EnumSetType, Debug)]
pub enum MinorMode {
    // To start with, just distinguish artsy main from artsy nav mode.
    ArtseyNav,
}

/// Something we can use to get time.
pub trait Timable {
    fn get_ticks(&self) -> u64;
}
