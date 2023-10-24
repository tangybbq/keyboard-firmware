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
//!
//! Unlike how something like qmk handles the combinations, we handle them at
//! the scancode layer, before there is any intepretation made. This does
//! somewhat limit what we can see as combos (and also means the combos apply to
//! physical keys, not dependent on the layer). With some help with the layer
//! code, this should avoid keys getting stuck with weird combinations of combo
//! keys and layers.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;
use crate::Mods;
use crate::log::warn;
use usbd_human_interface_device::page::Keyboard;

use crate::{KeyEvent, EventQueue, Event, KeyAction};

pub struct QwertyManager {
    down: BTreeSet<Mapping>,

    // The combo mapper.
    combo: ComboHandler,
}

struct ComboHandler {
    // Cached bitmap of keys that are parts of combos. Avoids a longer search
    // for keys that will never be part of one.
    combos: u64,

    // When we do send a down-event for a combo, indicate the keys pressed here,
    // as we will need to take care, upon release, to make sure these keys are
    // processed in the same layer they were pressed in.
    comboed: u64,

    // Potentially pending key event. A down even will be placed here if it
    // might participate in a combo.
    pending: Option<u8>,

    // When there is a pending key event, how long has it been since we've seen
    // it?
    pending_age: usize,

    // For each combo that is pressed down, record the keys contained in it, and
    // some information about what layer it was in to be able to process the
    // release properly.
    down: BTreeMap<[u8; 2], u8>,

    // Key events ready to be handled. This will hide keys that are parts of
    // combos, giving the non-combo events, as well as the synthesized events
    // from the combos.
    ready: VecDeque<KeyEvent>,
}

impl Default for ComboHandler {
    fn default() -> Self {
        // Collect all of the keys that are parts of combos. Avoids the need to
        // look up keys that aren't part of a combo.
        let mut combos = 0;
        for [a, b] in &COMBOS {
            combos |= 1 << a;
            combos |= 1 << b;
        }

        ComboHandler {
            combos,
            comboed: 0,
            pending: None,
            pending_age: 0,
            down: BTreeMap::new(),
            ready: VecDeque::new(),
        }
    }
}

impl ComboHandler {
    /// Deal with a single incoming key event. Events will be queued to 'ready',
    /// with the sanitized view of the events there.
    pub fn handle(&mut self, event: KeyEvent) {
        // A release event also will cause anything pending to be removed.
        if event.is_release() {
            self.push_pending();
        }

        match event {
            KeyEvent::Press(key) => {
                if self.possible_combo(key) {
                    if let Some(prior_key) = self.pending {
                        // There is a key, see if both of these make for a combo.
                        let keys = if prior_key < key {
                            [prior_key, key]
                        } else {
                            [key, prior_key]
                        };
                        if let Some(combo) = COMBOS.iter().position(|x| *x == keys) {
                            let combo = (combo + NKEYS) as u8;
                            // We have a combo. Enqueue that up, and neither of
                            // the pending keys.
                            self.ready.push_back(KeyEvent::Press(combo));
                            self.down.insert(keys, combo);
                        } else {
                            // Not a valid combo, press both keys, in the order
                            // we saw them in.
                            self.ready.push_back(KeyEvent::Press(prior_key));
                            self.ready.push_back(KeyEvent::Press(key));
                        }
                        // In either case, we've exhausted the pending key.
                        self.pending = None;

                        // Set the flags indicating both of these keys are down,
                        // and part of a combo.
                        self.comboed |= (1 << prior_key) | (1 << key);
                    } else {
                        // We have a possible key from a combo. Hold it for a
                        // little bit, and see if we get the other key.
                        self.pending = Some(key);
                        self.pending_age = 0;
                    }
                } else {
                    // This key can't be part of a combo, so just queue it up.
                    self.push_pending();
                    self.ready.push_back(event);
                }
            }
            KeyEvent::Release(key) => {
                if self.part_of_pressed(key) {
                    // Key is released, so indicate that.
                    self.comboed &= !(1 << key);

                    // Try to find a combo, where the key just released was part of it.
                    if let Some([a, b]) = self.down.keys().find(|&&[a,b]| a == key || b == key) {
                        let other = if *a == key { *b } else { *a };
                        if self.part_of_pressed(other) {
                            // The other key is still pressed, so nothing to do here.
                        } else {
                            if let Some(combo) = self.down.remove(&[*a, *b]) {
                                // Both have been released, so release the combo.
                                self.ready.push_back(KeyEvent::Release(combo));
                            } else {
                                // This is really an assertion failure.
                                warn!("Combo vanished from map");
                            }
                        }
                    } else {
                        // This shouldn't ever happen.
                        warn!("Key missing from combo map");
                    }
                } else {
                    // Not part of a pressed combo, just send a normal release.
                    self.ready.push_back(event);
                }
            }
        }
    }

    /// Called as part of the tick handler. Ages potentially pressed keys, so
    /// they will be sent in a timely manner if not accompanied by their
    /// companion.  May cause an event to be queue.
    pub fn tick(&mut self) {
        if self.pending.is_none() {
            return;
        }

        self.pending_age += 1;

        if self.pending_age >= 50 {
            self.push_pending();
        }
    }

    /// Potentially retrieve the next event.
    pub fn next(&mut self) -> Option<KeyEvent> {
        self.ready.pop_front()
    }

    // Move the pending event into the ready as just a press.
    fn push_pending(&mut self) {
        if let Some(key) = self.pending {
            self.ready.push_back(KeyEvent::Press(key));
            self.pending = None;
        }
    }

    // Is this code potentially in a combo?
    fn possible_combo(&self, key: u8) -> bool {
        (self.combos & (1 << key)) != 0
    }

    // Is this key part of a combo that was pressed?
    fn part_of_pressed(&self, key: u8) -> bool {
        (self.comboed & (1 << key)) != 0
    }
}

/*
struct ComboInfo {
    keys: [u8; 2],
}
*/

impl Default for QwertyManager {
    fn default() -> Self {
        QwertyManager {
            down: BTreeSet::new(),
            combo: ComboHandler::default(),
        }
    }
}

impl QwertyManager {
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        // Skip out of bound events.
        if event.key() as usize >= NKEYS {
            return;
        }

        self.combo.handle(event);
        self.process_keys(events);
    }

    pub fn tick(&mut self, events: &mut EventQueue) {
        self.combo.tick();
        self.process_keys(events);
    }

    fn process_keys(&mut self, events: &mut EventQueue) {
        while let Some(event) = self.combo.next() {
            // Skip out of bound events.
            if event.key() as usize >= ROOT_MAP.len() {
                // info!("Extra event: {}", event);
                continue;
            }

            let code = ROOT_MAP[event.key() as usize];
            if code.is_empty() {
                // Skip dead keys.
                continue;
            }

            // info!("Event: {}", event);
            if event.is_press() {
                self.down.insert(code);
                self.show(events, Some(code));
            } else {
                self.down.remove(&code);
                self.show(events, None);
            }
        }
    }

    fn show(&self, events: &mut EventQueue, code: Option<Mapping>) {
        let mut keys: Vec<Keyboard> = Vec::new();

        // We first need to collect the modifiers from any keys that are
        // pressed. Keys that have built-in modifiers are handled a bit
        // specially, we want that modifier to only apply when that key itself
        // is pressed. Modifiers that are held by themselves should persist
        // until they are released. We use the special 'code' above to know what
        // the new key being pressed is, and treat it's modifiers specially.

        // Modifiers that have been included in the key set.
        let mut sent = Mods::empty();

        // Go through every key, and add modifiers that are just modifier presses.
        for m in &self.down {
            if m.is_mod() {
                if !sent.contains(m.mods) {
                    push_mods(&mut sent, &mut keys, m.mods);
                }
            }
        }

        // Always add the modifiers from the latest key, if we pressed something.
        if let Some(code) = code {
            push_mods(&mut sent, &mut keys, code.mods);
        }

        // Now push the rest of the non-modifier keys.
        for m in &self.down {
            if m.has_nonmmod() {
                keys.push(m.key);
            }
        }

        events.push(Event::Key(KeyAction::KeySet(keys)));
    }
}

// Push keys for any modifiers mentioned here. The 'sent' tracks those that have
// already been pushed, so we don\t push redundant mods.
fn push_mods(sent: &mut Mods, keys: &mut Vec<Keyboard>, mods: Mods) {
    for (m, k) in &[
        (Mods::SHIFT, Keyboard::LeftShift),
        (Mods::CONTROL, Keyboard::LeftControl),
        (Mods::ALT, Keyboard::LeftAlt),
        (Mods::GUI, Keyboard::LeftGUI),
    ] {
        if mods.contains(*m) && !sent.contains(*m) {
            *sent |= *m;
            keys.push(*k);
        }
    }
}

// Number of keys on the main keyboard. Codes after this will be synthesized
// from pairs of keys on the main keyboard.
const NKEYS: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Mapping {
    key: Keyboard,
    mods: Mods,
}

impl Mapping {
    fn is_empty(&self) -> bool {
        self.key == Keyboard::NoEventIndicated && self.mods.is_empty()
    }

    // A modifier is indicate by the no key key, but with modifiers.
    fn is_mod(&self) -> bool {
        self.key == Keyboard::NoEventIndicated && !self.mods.is_empty()
    }

    // Does this press contain a non-modifier key?
    fn has_nonmmod(&self) -> bool {
        self.key != Keyboard::NoEventIndicated
    }
}

// Basic qwerty map for the proto3
static ROOT_MAP: [Mapping; NKEYS + 19] = [
    // 0
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 4
    Mapping { key: Keyboard::Q, mods: Mods::empty() },
    Mapping { key: Keyboard::A, mods: Mods::empty() },
    Mapping { key: Keyboard::Z, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 8
    Mapping { key: Keyboard::W, mods: Mods::empty() },
    Mapping { key: Keyboard::S, mods: Mods::empty() },
    Mapping { key: Keyboard::X, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 12
    Mapping { key: Keyboard::E, mods: Mods::empty() },
    Mapping { key: Keyboard::D, mods: Mods::empty() },
    Mapping { key: Keyboard::C, mods: Mods::empty() },
    Mapping { key: Keyboard::LeftBrace, mods: Mods::empty() },

    // 16
    Mapping { key: Keyboard::R, mods: Mods::empty() },
    Mapping { key: Keyboard::F, mods: Mods::empty() },
    Mapping { key: Keyboard::V, mods: Mods::empty() },
    Mapping { key: Keyboard::Tab, mods: Mods::empty() },

    // 20
    Mapping { key: Keyboard::T, mods: Mods::empty() },
    Mapping { key: Keyboard::G, mods: Mods::empty() },
    Mapping { key: Keyboard::B, mods: Mods::empty() },
    Mapping { key: Keyboard::DeleteBackspace, mods: Mods::empty() },

    // 24
    Mapping { key: Keyboard::Grave, mods: Mods::empty() },
    Mapping { key: Keyboard::Apostrophe, mods: Mods::empty() },
    Mapping { key: Keyboard::Equal, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 28
    Mapping { key: Keyboard::P, mods: Mods::empty() },
    Mapping { key: Keyboard::Semicolon, mods: Mods::empty() },
    Mapping { key: Keyboard::ForwardSlash, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 32
    Mapping { key: Keyboard::O, mods: Mods::empty() },
    Mapping { key: Keyboard::L, mods: Mods::empty() },
    Mapping { key: Keyboard::Dot, mods: Mods::empty() },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() },

    // 36
    Mapping { key: Keyboard::I, mods: Mods::empty() },
    Mapping { key: Keyboard::K, mods: Mods::empty() },
    Mapping { key: Keyboard::Comma, mods: Mods::empty() },
    Mapping { key: Keyboard::RightBrace, mods: Mods::empty() },

    // 40
    Mapping { key: Keyboard::U, mods: Mods::empty() },
    Mapping { key: Keyboard::J, mods: Mods::empty() },
    Mapping { key: Keyboard::M, mods: Mods::empty() },
    Mapping { key: Keyboard::ReturnEnter, mods: Mods::empty() },

    // 44
    Mapping { key: Keyboard::Y, mods: Mods::empty() },
    Mapping { key: Keyboard::H, mods: Mods::empty() },
    Mapping { key: Keyboard::N, mods: Mods::empty() },
    Mapping { key: Keyboard::Space, mods: Mods::empty() },

    // Left hand upper combos
    // 48
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL },

    // Right hand upper combos
    // 52
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT },
    Mapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI },
    Mapping { key: Keyboard::Backslash, mods: Mods::empty() },

    // Left hand lower combos
    // 57
    Mapping { key: Keyboard::Keyboard1, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard2, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard3, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard4, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard5, mods: Mods::SHIFT },

    // Right hand lower combos
    // 62
    Mapping { key: Keyboard::Keyboard6, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard7, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard8, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard9, mods: Mods::SHIFT },
    Mapping { key: Keyboard::Keyboard0, mods: Mods::SHIFT },

    // Thumb pairs "#A", "AO", "#U", "EU"
    // TODO: These are all layer shifts, wait for that to be implemented.
];

// Combination keys. Each of these pairs will register as the entry for its
// index in this list, starting at NKEY. Each pair should have the lowest
// scancode first.
static COMBOS: [[u8; 2]; 23] = [
    // Pairs with the top and middle row and the main fingers.
    [4, 5],
    [8, 9],
    [12, 13],
    [16, 17],
    [40, 41],
    [36, 37],
    [32, 33],
    [28, 29],
    [24, 25],

    // Pairs of the middle and lower row, main fingers.
    [5, 6],
    [9, 10],
    [13, 14],
    [17, 18],
    [21, 22],
    [45, 46],
    [41, 42],
    [37, 38],
    [33, 34],
    [29, 30],
    // [25, 26], // TODO: Should this map to something?

    // Pairs from the thumb keys.
    [15, 19],
    [19, 23],
    [39, 43],
    [43, 47],
];
