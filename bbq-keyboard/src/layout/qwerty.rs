//! Qwerty mode
//!
//! Qwerty mode tries to be as much like a regular keyboard as possible, at
//! least with what we can do with only 40% of the keys.  To make this usable,
//! we use a few techniques to act like we have more keys:
//!
//! - Layers. Some of the keys might act as shifts that, while they are pressed,
//!   cause remaining keys to be interpreted differently.
//! - Combo keys.  Some pairs of keys, when pressed closely enough together, can
//!   be treated as a key themselves.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use usbd_human_interface_device::page::Keyboard;

use crate::{KeyEvent, EventQueue, Event, KeyAction};

pub struct QwertyManager {
    down: BTreeSet<Keyboard>
}

impl Default for QwertyManager {
    fn default() -> Self {
        QwertyManager {
            down: BTreeSet::new(),
        }
    }
}

impl QwertyManager {
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        // Skip out of bound events, or those that are dead.
        if event.key() as usize >= ROOT_MAP.len() {
            return;
        }
        let code = ROOT_MAP[event.key() as usize];
        if code == Keyboard::NoEventIndicated {
            return;
        }

        if event.is_press() {
            self.down.insert(code);
            self.show(events);
        } else {
            self.down.remove(&code);
            self.show(events);
        }
    }

    fn show(&self, events: &mut EventQueue) {
        let keys: Vec<Keyboard> = self.down.iter().cloned().collect();
        events.push(Event::Key(KeyAction::KeySet(keys)));
    }
}

// Basic qwerty map for the proto3
static ROOT_MAP: [Keyboard; 48] = [
    // 0
    Keyboard::NoEventIndicated,
    Keyboard::NoEventIndicated,
    Keyboard::NoEventIndicated,
    Keyboard::NoEventIndicated,

    // 4
    Keyboard::Q,
    Keyboard::A,
    Keyboard::Z,
    Keyboard::NoEventIndicated,

    // 8
    Keyboard::W,
    Keyboard::S,
    Keyboard::X,
    Keyboard::NoEventIndicated,

    // 12
    Keyboard::E,
    Keyboard::D,
    Keyboard::C,
    Keyboard::LeftBrace,

    // 16
    Keyboard::R,
    Keyboard::F,
    Keyboard::V,
    Keyboard::Tab,

    // 20
    Keyboard::T,
    Keyboard::G,
    Keyboard::B,
    Keyboard::DeleteBackspace,

    // 24
    Keyboard::Grave,
    Keyboard::Apostrophe,
    Keyboard::Equal,
    Keyboard::NoEventIndicated,

    // 28
    Keyboard::P,
    Keyboard::Semicolon,
    Keyboard::ForwardSlash,
    Keyboard::NoEventIndicated,

    // 32
    Keyboard::O,
    Keyboard::L,
    Keyboard::Dot,
    Keyboard::NoEventIndicated,

    // 36
    Keyboard::I,
    Keyboard::K,
    Keyboard::Comma,
    Keyboard::RightBrace,

    // 40
    Keyboard::U,
    Keyboard::J,
    Keyboard::M,
    Keyboard::ReturnEnter,

    // 44
    Keyboard::Y,
    Keyboard::H,
    Keyboard::N,
    Keyboard::Space,
];
