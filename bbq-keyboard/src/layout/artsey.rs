//! Artsey keyboard support.

// I have made some additions to Artsey. I had added additional chord
// punctuation to the lower-inner key. I have also added a sticky-modifier mode,
// with alternate versions of the modifier keys that are intended for the case
// where they are held down and then there is a mouse click. These are pressed
// immediately, and released _before_ the next other type of key stroke, or
// after the explicit release is sent.
//
// Left view:  Right is symmetrical.
//
// XXXX - Sticky Shift.
// ---X
//
// X--X - Sticky Control.
// ---X
//
// X-X- - Sticky Gui.
// --X-
//
// XX-- - Sticky Alt.
// -X--
//
// --XX - Sticky Release.
// --XX

use usbd_human_interface_device::page::Keyboard;

// use crate::log::info;

use crate::{KeyEvent, EventQueue, KeyAction, Event, Mods, MinorMode};

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

    // The key that got us into a hold mode.
    hold_mode: u8,

    // When in hold mode, was a key pressed?
    hold_sent: bool,

    // Cached mapping table.
    mapping: &'static[Entry],

    // Was the last key seen on the left or right side. The hold maps differ
    // between sides, so we need this to decide which map to use.
    is_right: bool,

    // Are we in nav mode?
    nav: bool,

    // Any stick modifiers that have been sent.
    sticky: Mods,
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
//
// The hold keys are a little bit tricky, with several scenarios that can
// happen. On the one hand, it'd be nice if just holding down one of them would
// send that plain key, with repeats. However, this probably can't be done
// ambiguously with "hold", so we might have to just live with not being able to
// repeat one of 'A', 'E', 'S' or 'O'. This is in line with how qmk handles
// this. As such, we will consider one of these keys to have been "held" if it
// was held for a certain threshold of time. We will clear that key from the
// held mask, and indicate our special mode through other fields. When the
// special key is released, we'll undo the mode. If at this time, we discover
// that there wasn't actually anything done with the hold key, we'll then send
// what that key would send, otherwise these keys won't actually be useful.

/// The first key that is on the right side of the keyboard. TODO: Can this come
/// from the upper layers?
#[cfg(feature = "proto2")]
const FIRST_RIGHT_KEY: u8 = 16;

#[cfg(feature = "proto3")]
const FIRST_RIGHT_KEY: u8 = 24;

static LEFT_HOLD_KEYS: [HoldEntry; 4] = [
    HoldEntry { code: 0x80, mapping: &LEFT_BRACKET_MAP },
    HoldEntry { code: 0x10, mapping: &NUMBER_MAP },
    HoldEntry { code: 0x08, mapping: &LEFT_PUNCT_MAP },
    HoldEntry { code: 0x01, mapping: &NORMAL },
];

static RIGHT_HOLD_KEYS: [HoldEntry; 4] = [
    HoldEntry { code: 0x80, mapping: &RIGHT_BRACKET_MAP },
    HoldEntry { code: 0x10, mapping: &NUMBER_MAP },
    HoldEntry { code: 0x08, mapping: &RIGHT_PUNCT_MAP },
    HoldEntry { code: 0x01, mapping: &NORMAL },
];

struct HoldEntry {
    code: u8,
    mapping: &'static [Entry],
}

// Mapping of proto2 keys to artsey bits.  Codes past this will result in zero.
#[cfg(feature = "proto2")]
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

#[cfg(feature = "proto3")]
static KEY_TO_ARTSEY: [u8; 43] = [
    0x00, //  0
    0x00, //  1
    0x00, //  2
    0x00, //  3
    0x00, //  4
    0x10, //  5 - left S
    0x01, //  6 - left O
    0x00,
    0x00,
    0x20, //  9 - left T
    0x02, // 10 - left I
    0x00,
    0x00,
    0x40, // 13 - left R
    0x04, // 14 - left Y
    0x00,
    0x00,
    0x80, // 17 - left E
    0x08, // 18 - left A
    0x00, // 19
    0x00, // 20
    0x00, // 21
    0x00, // 22
    0x00, // 23
    0x00, // 24
    0x00, // 25
    0x00, // 26
    0x00, // 27
    0x00, // 28
    0x10, // 29 - right S
    0x01, // 30 - right O
    0x00, // 31
    0x00, // 32
    0x20, // 33 - right T
    0x02, // 34 - right I
    0x00, // 35
    0x00, // 36
    0x40, // 37 - right R
    0x04, // 38 - right Y
    0x00, // 39
    0x00, // 40
    0x80, // 41 - right A
    0x08, // 42 - right E
];

enum Value {
    Simple(Keyboard),
    Shifted(Keyboard),
    OneShot(Mods),
    Lock(Mods),
    Nav,
    Sticky(Mods),
    Unstick,
    None,
}

struct Entry {
    code: u8,
    value: Value,
}

// Normal Artsey mode map.
static NORMAL: [Entry; 51] = [
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
    Entry { code: 0x86, value: Value::Simple(Keyboard::Apostrophe), },
    Entry { code: 0xe1, value: Value::Simple(Keyboard::Tab), },
    Entry { code: 0x84, value: Value::Simple(Keyboard::Dot), },
    Entry { code: 0x18, value: Value::OneShot(Mods::CONTROL), },
    Entry { code: 0x82, value: Value::Simple(Keyboard::Comma), },
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
    Entry { code: 0x4a, value: Value::Nav, },

    // My additions.
    Entry { code: 0x8f, value: Value::Shifted(Keyboard::Minus), },
    Entry { code: 0x98, value: Value::Sticky(Mods::CONTROL), },
    Entry { code: 0x54, value: Value::Sticky(Mods::GUI), },
    Entry { code: 0x32, value: Value::Sticky(Mods::ALT), },
    Entry { code: 0xf8, value: Value::Sticky(Mods::SHIFT), },
    Entry { code: 0xcc, value: Value::Unstick, },
];

// The number Artsey mapping.
static NUMBER_MAP: [Entry; 10] = [
    Entry { code: 0x80, value: Value::Simple(Keyboard::Keyboard1), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::Keyboard2), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::Keyboard3), },
    Entry { code: 0x08, value: Value::Simple(Keyboard::Keyboard4), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::Keyboard5), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::Keyboard6), },
    Entry { code: 0xc0, value: Value::Simple(Keyboard::Keyboard7), },
    Entry { code: 0x60, value: Value::Simple(Keyboard::Keyboard8), },
    Entry { code: 0x0c, value: Value::Simple(Keyboard::Keyboard9), },
    Entry { code: 0x06, value: Value::Simple(Keyboard::Keyboard0), },
];

// The bracket Artsey mapping, for the right side of the keyboard
static RIGHT_BRACKET_MAP: [Entry; 6] = [
    Entry { code: 0x40, value: Value::Shifted(Keyboard::Keyboard9), },
    Entry { code: 0x20, value: Value::Shifted(Keyboard::Keyboard0), },
    Entry { code: 0x10, value: Value::Shifted(Keyboard::LeftBrace), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::LeftBrace), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::RightBrace), },
    Entry { code: 0x01, value: Value::Shifted(Keyboard::RightBrace), },
];

// The bracket Artsey mapping, for the left side of the keyboard
// The standard Artsey swaps the curly braces, even though they are positioned
// vertically. I have not done this, because this doesn't make sense to me.
static LEFT_BRACKET_MAP: [Entry; 6] = [
    Entry { code: 0x20, value: Value::Shifted(Keyboard::Keyboard9), },
    Entry { code: 0x40, value: Value::Shifted(Keyboard::Keyboard0), },
    Entry { code: 0x10, value: Value::Shifted(Keyboard::LeftBrace), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::LeftBrace), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::RightBrace), },
    Entry { code: 0x01, value: Value::Shifted(Keyboard::RightBrace), },
];

static LEFT_PUNCT_MAP: [Entry; 11] = [
    Entry { code: 0x80, value: Value::Shifted(Keyboard::Keyboard1), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::Backslash), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::Semicolon), },
    Entry { code: 0x10, value: Value::Simple(Keyboard::Grave), },
    Entry { code: 0x04, value: Value::Shifted(Keyboard::ForwardSlash), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::Minus), },
    Entry { code: 0x01, value: Value::Simple(Keyboard::Equal), },

    // My additions. These the top ones should probably distinguish left/right
    Entry { code: 0x60, value: Value::Shifted(Keyboard::Dot), },
    Entry { code: 0x30, value: Value::Shifted(Keyboard::Comma), },
    Entry { code: 0x06, value: Value::Shifted(Keyboard::Semicolon), },
    Entry { code: 0x03, value: Value::Shifted(Keyboard::Keyboard3), },
];

static RIGHT_PUNCT_MAP: [Entry; 11] = [
    Entry { code: 0x80, value: Value::Shifted(Keyboard::Keyboard1), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::Backslash), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::Semicolon), },
    Entry { code: 0x10, value: Value::Simple(Keyboard::Grave), },
    Entry { code: 0x04, value: Value::Shifted(Keyboard::ForwardSlash), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::Minus), },
    Entry { code: 0x01, value: Value::Simple(Keyboard::Equal), },

    // My additions.
    Entry { code: 0x60, value: Value::Shifted(Keyboard::Comma), },
    Entry { code: 0x30, value: Value::Shifted(Keyboard::Dot), },
    Entry { code: 0x06, value: Value::Shifted(Keyboard::Semicolon), },
    Entry { code: 0x03, value: Value::Shifted(Keyboard::Keyboard3), },
];

// Nav is the 8 nav buttons, the 4 one shot modifiers, shift lock, and the one
// nav toggle key.
static RIGHT_NAV_MAP: [Entry; 14] = [
    Entry { code: 0x80, value: Value::Simple(Keyboard::Home), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::UpArrow), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::End), },
    Entry { code: 0x10, value: Value::Simple(Keyboard::PageUp), },
    Entry { code: 0x08, value: Value::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::DownArrow), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::RightArrow), },
    Entry { code: 0x01, value: Value::Simple(Keyboard::PageDown), },

    Entry { code: 0x18, value: Value::OneShot(Mods::CONTROL), },
    Entry { code: 0x14, value: Value::OneShot(Mods::GUI), },
    Entry { code: 0x12, value: Value::OneShot(Mods::ALT), },
    Entry { code: 0x78, value: Value::OneShot(Mods::SHIFT), },
    Entry { code: 0x44, value: Value::Lock(Mods::SHIFT), },

    Entry { code: 0x4a, value: Value::Nav, },
];

// Nav is the 8 nav buttons, the 4 one shot modifiers, shift lock, and the one
// nav toggle key.
static LEFT_NAV_MAP: [Entry; 14] = [
    Entry { code: 0x80, value: Value::Simple(Keyboard::End), },
    Entry { code: 0x40, value: Value::Simple(Keyboard::UpArrow), },
    Entry { code: 0x20, value: Value::Simple(Keyboard::Home), },
    Entry { code: 0x10, value: Value::Simple(Keyboard::PageUp), },
    Entry { code: 0x08, value: Value::Simple(Keyboard::RightArrow), },
    Entry { code: 0x04, value: Value::Simple(Keyboard::DownArrow), },
    Entry { code: 0x02, value: Value::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x01, value: Value::Simple(Keyboard::PageDown), },

    Entry { code: 0x18, value: Value::OneShot(Mods::CONTROL), },
    Entry { code: 0x14, value: Value::OneShot(Mods::GUI), },
    Entry { code: 0x12, value: Value::OneShot(Mods::ALT), },
    Entry { code: 0x78, value: Value::OneShot(Mods::SHIFT), },
    Entry { code: 0x44, value: Value::Lock(Mods::SHIFT), },

    Entry { code: 0x4a, value: Value::Nav, },
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
            hold_mode: 0x00,
            hold_sent: false,
            mapping: &NORMAL,
            is_right: false,
            nav: false,
            sticky: Mods::empty(),
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

        if self.seen != 0 && self.age >= 50 {
            // If we have a 'seen' value, and suffient age, and we aren't in a
            // special mode, then activate the special mode.
            if self.hold_mode == 0 {
                let hold = if self.is_right { &RIGHT_HOLD_KEYS } else { &LEFT_HOLD_KEYS };
                if let Some(HoldEntry { mapping, .. }) = hold.iter().find(|k| k.code == self.seen) {
                    self.hold_mode = self.seen;
                    self.mapping = mapping;
                    // Pretend this key isn't actually held down.
                    self.seen = 0;
                    self.pressed = 0;
                    // And note that we haven't sent any of these keys yet.
                    self.hold_sent = false;
                }
            }

            if self.seen != 0 {
                self.handle_down(events);
            }
        }
    }

    fn handle_down(&mut self, events: &mut EventQueue) {
        let base_mods = self.locked | self.oneshot;

        match self.mapping.iter().find(|e| e.code == self.seen) {
            Some(Entry { value: Value::Simple(k), .. }) => {
                self.sticky = Mods::empty();
                self.down = true;
                events.push(Event::Key(KeyAction::KeyPress(*k, base_mods)));
                self.oneshot = Mods::empty();
                self.hold_sent = true;
                // info!("Simple: {}", *k as u8);
            }
            Some(Entry { value: Value::Shifted(k), .. }) => {
                self.sticky = Mods::empty();
                self.down = true;
                events.push(Event::Key(KeyAction::KeyPress(*k, base_mods | Mods::SHIFT)));
                self.oneshot = Mods::empty();
                self.hold_sent = true;
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
            Some(Entry { value: Value::Sticky(k), .. }) => {
                self.sticky |= *k;
                events.push(Event::Key(KeyAction::ModOnly(self.sticky)));
            }
            Some(Entry { value: Value::Unstick, .. }) => {
                // Release, if any are pressed.
                if !self.sticky.is_empty() {
                    events.push(Event::Key(KeyAction::KeyRelease));
                }
                self.sticky = Mods::empty();
            }
            Some(Entry { value: Value::Nav, .. }) => {
                // Toggle nav mode.
                self.nav = !self.nav;
                self.set_normal();
                let ind = if self.nav {
                    MinorMode::ArtseyNav
                } else {
                    MinorMode::ArtseyMain
                };
                events.push(Event::Indicator(ind));
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
                // The right side for the proto2.  This will be board specific.
                self.is_right = k >= FIRST_RIGHT_KEY;

                let code = to_artsey(k);
                self.pressed |= code;
                self.seen |= code;
                self.age = 0;
            }
            KeyEvent::Release(k) => {
                let code = to_artsey(k);
                self.pressed &= !code;

                // If the hold mode key is released, back out of the mode. TODO:
                // Is this funny if the hold is released before the other key?
                if code == self.hold_mode {
                    // If we entered hold mode, but didn't send anything, put
                    // the hold key down as a seen key so that it can still be
                    // just typed.
                    if !self.hold_sent {
                        self.seen = self.hold_mode;
                    }

                    self.hold_mode = 0x00;
                    self.set_normal();
                }

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

    fn set_normal(&mut self) {
        if self.nav {
            if self.is_right {
                self.mapping = &RIGHT_NAV_MAP;
            } else {
                self.mapping = &LEFT_NAV_MAP;
            }
        } else {
            self.mapping = &NORMAL;
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
