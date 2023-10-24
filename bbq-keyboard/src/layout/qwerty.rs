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
        match event {
            KeyEvent::Press(4) => {
                self.down.insert(Keyboard::Q);
                self.show(events);
            }
            KeyEvent::Release(4) => {
                self.down.remove(&Keyboard::Q);
                self.show(events);
            }
            _ => (),
        }
    }

    fn show(&self, events: &mut EventQueue) {
        let keys: Vec<Keyboard> = self.down.iter().cloned().collect();
        events.push(Event::Key(KeyAction::KeySet(keys)));
    }
}
