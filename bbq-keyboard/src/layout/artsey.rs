//! Artsey keyboard support.

use usbd_human_interface_device::page::Keyboard;

// use crate::log::info;

use crate::{KeyEvent, EventQueue, KeyAction, Event, Mods};

pub struct ArtseyManager {
    // Keys that are currently down.
    pressed: u8,

    // Time since we last saw keys go down.
    age: u32,

    // Has a keydown been sent (regular, not locking).
    down: bool,

    // Keys that have been pressed.
    seen: u8,

    // The modifiers that apply to the next actual to-send keystroke.
    oneshot: Mods,

    // Modifiers that are locked down.
    locked: Mods,
}

// The Artsey keyboard consists of a full keyboard layout implemented on 8 keys.
// We represent the keys as seen on the right side keyboard, and the left is a
// mirror image of this.  For the most part, the Artsey keyboard works like
// steno, in that keys are pressed as chords and processed upon release.
// However, the corner keys can also be held, which causes them to be treated
// differently.  We will consider corner keys to be held in this manner if a
// single corner key is held for a short period of time before other keys are
// pressed.  This hold mode will remain in effect until that key is released.
// That key will then not be considered in the other modes, which will then use
// the normal mapping.
//
// In addition the above, there are a set of one-shot modifiers which affect the
// key sent immediately after them.  Artsey also defines a shift lock key (and a
// caps lock), but doesn't define a way to lock down the other modifiers.  This
// implementation will allow the other modifiers to be effectively held down so
// they can be used to modify mouse clicks.  They will combine, and pressing any
// of the modifiers that is already down will release all of them.  This will
// need to be used to determine if it is useful.
//
// The 8 keys of the artsey mode are represented as a u8, with the bits as
// follows:
//    80 40 20 10
//    08 04 02 01
// The left side is the same, but flipped left to right.

// To start with, implement just the plain mapping, not the nav, mouse, holds,
// or modifiers.

// Mapping of proto2 keys to artsey bits.  Codes past this will result in zero.
static KEY_TO_ARTSEY: [u8; 28] = [
    0x00, //  0
    0x00, //  1
    0x00, //  2
    0x00, //  3
    0x00, //  4
    0x08, //  5 - left E
    0x80, //  6 - left A
    0x04, //  7 - left Y
    0x40, //  8 - left R
    0x20, //  9 - left T
    0x02, // 10 - left I
    0x10, // 11 - left S
    0x01, // 12 - left O
    0x00, // 13
    0x00, // 14
    0x00, // 15
    0x00, // 16
    0x00, // 17
    0x00, // 18
    0x00, // 19
    0x08, // 20 - right E
    0x80, // 21 - right A
    0x04, // 22 - right Y
    0x40, // 23 - right R
    0x20, // 24 - right T
    0x02, // 25 - right I
    0x10, // 26 - right S
    0x01, // 27 - right O
];

enum Value {
    Simple(Keyboard),
    Shifted(Keyboard),
    OneShot(Mods),
    Lock(Mods),
    None,
}

struct Entry {
    code: u8,
    value: Value,
}

// Normal Artsey mode map.
static NORMAL: [Entry; 44] = [
    Entry { code: 0x80, value: Value::Simple(Keyboard::A), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::R), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::T), },
    Entry { code: 0x10, value: Value::Simple(Keyboard::S), },
    Entry { code: 0x08, value: Value::Simple(Keyboard::E), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::Y), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::I), },
    Entry { code: 0x01, value: Value::Simple(Keyboard::O), },
    Entry { code: 0x09, value: Value::Simple(Keyboard::B), },
    Entry { code: 0x0c, value: Value::Simple(Keyboard::C), },
    Entry { code: 0xe0, value: Value::Simple(Keyboard::D), },
    Entry { code: 0xc0, value: Value::Simple(Keyboard::F), },
    Entry { code: 0x60, value: Value::Simple(Keyboard::G), },
    Entry { code: 0x0a, value: Value::Simple(Keyboard::H), },
    Entry { code: 0x30, value: Value::Simple(Keyboard::J), },
    Entry { code: 0x05, value: Value::Simple(Keyboard::K), },
    Entry { code: 0x0e, value: Value::Simple(Keyboard::L), },
    Entry { code: 0x07, value: Value::Simple(Keyboard::M), },
    Entry { code: 0x03, value: Value::Simple(Keyboard::N), },
    Entry { code: 0x0b, value: Value::Simple(Keyboard::P), },
    Entry { code: 0xb0, value: Value::Simple(Keyboard::Q), },
    Entry { code: 0x06, value: Value::Simple(Keyboard::U), },
    Entry { code: 0x50, value: Value::Simple(Keyboard::V), },
    Entry { code: 0x90, value: Value::Simple(Keyboard::W), },
    Entry { code: 0x70, value: Value::Simple(Keyboard::X), },
    Entry { code: 0xf0, value: Value::Simple(Keyboard::Z), },

    Entry { code: 0x88, value: Value::Simple(Keyboard::ReturnEnter), },
    Entry { code: 0xc1, value: Value::Simple(Keyboard::Escape), },
    Entry { code: 0x86, value: Value::Simple(Keyboard::Grave), },
    Entry { code: 0xe1, value: Value::Simple(Keyboard::Tab), },
    Entry { code: 0x84, value: Value::Simple(Keyboard::Dot), },
    Entry { code: 0x18, value: Value::OneShot(Mods::CONTROL), },
    Entry { code: 0x82, value: Value::Simple(Keyboard::Apostrophe), },
    Entry { code: 0x14, value: Value::OneShot(Mods::GUI), },
    Entry { code: 0x81, value: Value::Simple(Keyboard::ForwardSlash), },
    Entry { code: 0x12, value: Value::OneShot(Mods::ALT), },
    Entry { code: 0x22, value: Value::Shifted(Keyboard::Keyboard1), },
    Entry { code: 0x78, value: Value::OneShot(Mods::SHIFT), },
    Entry { code: 0x0f, value: Value::Simple(Keyboard::Space), },
    Entry { code: 0x44, value: Value::Lock(Mods::SHIFT), },
    Entry { code: 0x48, value: Value::Simple(Keyboard::DeleteBackspace), },
    Entry { code: 0x87, value: Value::Simple(Keyboard::CapsLock), },
    Entry { code: 0x42, value: Value::Simple(Keyboard::DeleteForward), },
    Entry { code: 0x66, value: Value::None, },

];

impl Default for ArtseyManager {
    fn default() -> Self {
        ArtseyManager {
            seen: 0,
            age: 0,
            down: false,
            pressed: 0,
            oneshot: Mods::empty(),
            locked: Mods::empty(),
        }
    }
}

impl ArtseyManager {
    /// Poll doesn't do anything.
    pub fn poll(&mut self) {
    }

    /// Tick is needed to track time for determining time.
    pub fn tick(&mut self, events: &mut EventQueue) {
        // If we've seen keys, bump the age, and then when they have been down
        // sufficiently long to be considered together, process them as a send
        // event.
        if self.pressed != 0 {
            self.age = self.age.saturating_add(1);
        }

        if self.seen != 0 && self.age >= 100 {
            self.handle_down(events);
        }
    }

    fn handle_down(&mut self, events: &mut EventQueue) {
        let base_mods = self.locked | self.oneshot;

        match NORMAL.iter().find(|e| e.code == self.seen) {
            Some(Entry { value: Value::Simple(k), .. }) => {
                self.down = true;
                events.push(Event::Key(KeyAction::KeyPress(*k, base_mods)));
                self.oneshot = Mods::empty();
                // info!("Simple: {}", *k as u8);
            }
            Some(Entry { value: Value::Shifted(k), .. }) => {
                self.down = true;
                events.push(Event::Key(KeyAction::KeyPress(*k, base_mods | Mods::SHIFT)));
                self.oneshot = Mods::empty();
                // info!("Shifted: {}", *k as u8);
            }
            Some(Entry { value: Value::OneShot(k), .. }) => {
                // Oneshot modifiers are kept until the next keypress goes
                // through.
                self.oneshot |= *k;
            }
            Some(Entry { value: Value::Lock(k), .. }) => {
                // Locked modifiers are a toggle of modifiers sent with
                // everything form now on.
                self.locked ^= *k;
            }
            Some(Entry { value: Value::None, .. }) => (),
            None => (),
        }
        self.seen = 0;
    }

    /// Handle a single key event.
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        // info!("Artsey {}", event);
        match event {
            KeyEvent::Press(k) => {
                let code = to_artsey(k);
                self.pressed |= code;
                self.seen |= code;
                self.age = 0;
            }
            KeyEvent::Release(k) => {
                let code = to_artsey(k);
                self.pressed &= !code;

                // When we actually released one of our keys, and the result has
                // no keys pressed.
                if code != 0 && self.pressed == 0 {
                    // If we didn't actually do anything yet (due to age),
                    // actually send the key.
                    if !self.down {
                        self.handle_down(events);
                    }

                    // If something caused it to go down, release all of it.
                    if self.down {
                        self.down = false;
                        events.push(Event::Key(KeyAction::KeyRelease));
                        //info!("Release");
                    }
                }
            }
        }
    }
}

fn to_artsey(key: u8) -> u8 {
    let key = key as usize;
    if key < KEY_TO_ARTSEY.len() {
        KEY_TO_ARTSEY[key]
    } else {
        0
    }
}
