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

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use crate::Mods;
use crate::log::warn;
use usbd_human_interface_device::page::Keyboard;

use crate::{KeyEvent, EventQueue, Event, KeyAction};

pub struct QwertyManager {
    down: BTreeMap<u8, Mapping>,

    // The combo mapper.
    combo: ComboHandler,

    // Current layer.
    layer: Layout,
}

type Layout = &'static [Mapping];

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
    pending: Option<(u8, Layout)>,

    // When there is a pending key event, how long has it been since we've seen
    // it?
    pending_age: usize,

    // For each combo that is pressed down, record the keys contained in it, and
    // some information about what layer it was in to be able to process the
    // release properly.
    down: BTreeMap<[u8; 2], ComboInfo>,

    // Key events ready to be handled. This will hide keys that are parts of
    // combos, giving the non-combo events, as well as the synthesized events
    // from the combos.
    ready: VecDeque<LayeredEvent>,
}

struct LayeredEvent {
    key: KeyEvent,
    layer: Layout,
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
    pub fn handle(&mut self, event: KeyEvent, layer: Layout) {
        // A release event also will cause anything pending to be removed.
        if event.is_release() {
            // TODO: Better track the release layers.
            self.push_pending();
        }

        match event {
            KeyEvent::Press(key) => {
                if self.possible_combo(key) {
                    if let Some((prior_key, layer)) = self.pending {
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
                            self.ready.push_back(LayeredEvent { key: KeyEvent::Press(combo), layer });
                            self.down.insert(keys, ComboInfo {
                                code: combo,
                                layer,
                            });

                            // Set the flags indicating both of these keys are down,
                            // and part of a combo.
                            self.comboed |= (1 << prior_key) | (1 << key);
                            self.pending = None;
                        } else {
                            // Not a valid combo.  Press the older one.
                            self.ready.push_back(LayeredEvent { key: KeyEvent::Press(prior_key), layer });
                            // And make the new key into a pending key, resetting the age timer for
                            // the new press.
                            self.pending = Some((key, layer));
                            self.pending_age = 0;
                        }
                    } else {
                        // We have a possible key from a combo. Hold it for a
                        // little bit, and see if we get the other key.
                        self.pending = Some((key, layer));
                        self.pending_age = 0;
                    }
                } else {
                    // This key can't be part of a combo, so just queue it up.
                    self.push_pending();
                    self.ready.push_back(LayeredEvent { key: event, layer });
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
                                self.ready.push_back(LayeredEvent {
                                    key: KeyEvent::Release(combo.code),
                                    layer: combo.layer,
                                });
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
                    // TODO: We probably need to handle layer changes here.
                    self.ready.push_back(LayeredEvent { key: event, layer });
                }
            }
        }
    }

    /// Handle a keypress in NKRO mode.  This is just a simple layer with no
    /// switching or combo keys.
    pub fn handle_nkro(&mut self, event: KeyEvent) {
        self.ready.push_back(LayeredEvent { key: event, layer: &NKRO_MAP });
    }

    /// Called as part of the tick handler. Ages potentially pressed keys, so
    /// they will be sent in a timely manner if not accompanied by their
    /// companion.  May cause an event to be queue.
    pub fn tick(&mut self) {
        if self.pending.is_none() {
            return;
        }

        self.pending_age += 1;

        if self.pending_age >= 250 {
            self.push_pending();
        }
    }

    /// Potentially retrieve the next event.
    pub fn next(&mut self) -> Option<LayeredEvent> {
        self.ready.pop_front()
    }

    // Move the pending event into the ready as just a press.
    fn push_pending(&mut self) {
        if let Some((key, layer)) = self.pending {
            self.ready.push_back(LayeredEvent { key: KeyEvent::Press(key), layer });
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

struct ComboInfo {
    // The combo keycode relevant here.
    code: u8,
    // What layer map to use to interpret the key release.
    layer: Layout,
}

impl Default for QwertyManager {
    fn default() -> Self {
        QwertyManager {
            down: BTreeMap::new(),
            combo: ComboHandler::default(),
            layer: &ROOT_MAP,
        }
    }
}

impl QwertyManager {
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut dyn EventQueue, nkro: bool) {
        // Skip out of bound events.
        if event.key() as usize >= NKEYS {
            return;
        }

        if nkro {
            // For nkro, just push the event in the nkro layer.
            self.combo.handle_nkro(event);
        } else {
            self.combo.handle(event, self.layer);
        }
        self.process_keys(events);
    }

    pub fn tick(&mut self, events: &mut dyn EventQueue) {
        self.combo.tick();
        self.process_keys(events);
    }

    fn process_keys(&mut self, events: &mut dyn EventQueue) {
        while let Some(LayeredEvent { key: event, layer }) = self.combo.next() {
            // Skip out of bound events.
            if event.key() as usize >= layer.len() {
                // info!("Extra event: {}", event);
                continue;
            }

            // Get the mapping of a release event from the 'down' information, in case we have it.
            let code = if event.is_release() {
                self.down.remove(&event.key())
            } else {
                None
            };

            // If we don't have a mapping, look it up in the current layer.
            let code = code.unwrap_or_else(|| layer[event.key() as usize]);
            if code.is_empty() {
                // Skip dead keys.
                continue;
            }

            // Handle layer changes.
            match code {
                Mapping::LayerShift(nlayer) => {
                    if event.is_press() {
                        self.layer = nlayer;
                    } else {
                        self.layer = &ROOT_MAP;
                    }
                    continue;
                }
                _ => (),
            }

            // info!("Event: {}", event);
            if event.is_press() {
                self.down.insert(event.key(), code);
                self.show(events, Some(code));
            } else {
                self.show(events, None);
            }
        }
    }

    fn show(&self, events: &mut dyn EventQueue, code: Option<Mapping>) {
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
        for m in self.down.values() {
            if let Mapping::Key(m) = m {
                if m.is_mod() {
                    if !sent.contains(m.mods) {
                        push_mods(&mut sent, &mut keys, m.mods);
                    }
                }
            }
        }

        // Always add the modifiers from the latest key, if we pressed something.
        if let Some(Mapping::Key(code)) = code {
            push_mods(&mut sent, &mut keys, code.mods);
        }

        // Now push the rest of the non-modifier keys.
        for m in self.down.values() {
            if let Mapping::Key(m) = m {
                if m.has_nonmmod() {
                    keys.push(m.key);
                }
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
enum Mapping {
    // This key doesn't do anything.
    Dead,
    // A regular keypress.
    Key(KeyMapping),
    // A layer change that works like a shift key, keys while this is held are
    // interpreted in the new layer.
    LayerShift(Layout),
}

impl Mapping {
    fn is_empty(&self) -> bool {
        match self {
            Mapping::Dead => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct KeyMapping {
    key: Keyboard,
    mods: Mods,
}

impl KeyMapping {
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
static ROOT_MAP: [Mapping; NKEYS + 24] = [
    // 0
    Mapping::Key(KeyMapping { key: Keyboard::Grave, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Escape, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::empty() }),
    Mapping::Dead,

    // 4
    Mapping::Key(KeyMapping { key: Keyboard::Q, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::A, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Z, mods: Mods::empty() }),
    Mapping::Dead,

    // 8
    Mapping::Key(KeyMapping { key: Keyboard::W, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::S, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::X, mods: Mods::empty() }),
    Mapping::Dead,

    // 12
    Mapping::Key(KeyMapping { key: Keyboard::E, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::D, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::C, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::LeftBrace, mods: Mods::empty() }),

    // 16
    Mapping::Key(KeyMapping { key: Keyboard::R, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::F, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::V, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Tab, mods: Mods::empty() }),

    // 20
    Mapping::Key(KeyMapping { key: Keyboard::T, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::G, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::B, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::DeleteBackspace, mods: Mods::empty() }),

    // 24
    Mapping::Key(KeyMapping { key: Keyboard::Minus, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Apostrophe, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Equal, mods: Mods::empty() }),
    Mapping::Dead,

    // 28
    Mapping::Key(KeyMapping { key: Keyboard::P, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Semicolon, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::ForwardSlash, mods: Mods::empty() }),
    Mapping::Dead,

    // 32
    Mapping::Key(KeyMapping { key: Keyboard::O, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::L, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Dot, mods: Mods::empty() }),
    Mapping::Dead,

    // 36
    Mapping::Key(KeyMapping { key: Keyboard::I, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::K, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Comma, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::RightBrace, mods: Mods::empty() }),

    // 40
    Mapping::Key(KeyMapping { key: Keyboard::U, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::J, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::M, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::ReturnEnter, mods: Mods::empty() }),

    // 44
    Mapping::Key(KeyMapping { key: Keyboard::Y, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::H, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::N, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Space, mods: Mods::empty() }),

    // Left hand upper combos
    // 48
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),

    // Right hand upper combos
    // 52
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Key(KeyMapping { key: Keyboard::Backslash, mods: Mods::empty() }),

    // Left hand lower combos
    // 57
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard1, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard2, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard3, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard4, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard5, mods: Mods::SHIFT }),

    // Right hand lower combos
    // 62
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard6, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard7, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard8, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard9, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard0, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::CapsLock, mods: Mods::empty() }),

    // Thumb pairs "#A", "AO", "#U", "EU"
    // TODO: These are all layer shifts, wait for that to be implemented.
    Mapping::LayerShift(&FN_MAP),
    Mapping::LayerShift(&NUM_MAP),
    Mapping::Dead,
    Mapping::LayerShift(&NAV_MAP),
];

static NUM_MAP: [Mapping; NKEYS + 24] = [
    // 0
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 4
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard1, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::A, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,

    // 8
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard2, mods: Mods::empty() }),
    // This is the wrong place for 'E', but if you can get used to it, it avoids
    // the conflict due to '3' being where the 'e' i.
    Mapping::Key(KeyMapping { key: Keyboard::E, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::X, mods: Mods::empty() }),
    Mapping::Dead,

    // 12
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard3, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::D, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::C, mods: Mods::empty() }),
    Mapping::Dead,

    // 16
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard4, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::F, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,

    // 20
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard5, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Key(KeyMapping { key: Keyboard::B, mods: Mods::empty() }),
    Mapping::Dead,

    // 24
    Mapping::Key(KeyMapping { key: Keyboard::Minus, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 28
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard0, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Semicolon, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::ForwardSlash, mods: Mods::empty() }),
    Mapping::Dead,

    // 32
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard9, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Key(KeyMapping { key: Keyboard::Dot, mods: Mods::empty() }),
    Mapping::Dead,

    // 36
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard8, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Key(KeyMapping { key: Keyboard::Comma, mods: Mods::empty() }),
    Mapping::Dead,

    // 40
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard7, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 44
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard6, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Key(KeyMapping { key: Keyboard::Space, mods: Mods::empty() }),

    // Left hand upper combos
    // 48
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),

    // Right hand upper combos
    // 52
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Dead,

    // Left hand lower combos
    // 57
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Right hand lower combos
    // 62
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Thumb pairs "#A", "AO", "#U", "EU"
    // TODO: These are all layer shifts, wait for that to be implemented.
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
];

static FN_MAP: [Mapping; NKEYS + 24] = [
    // 0
    Mapping::Key(KeyMapping { key: Keyboard::F12, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 4
    Mapping::Key(KeyMapping { key: Keyboard::F1, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 8
    Mapping::Key(KeyMapping { key: Keyboard::F2, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 12
    Mapping::Key(KeyMapping { key: Keyboard::F3, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 16
    Mapping::Key(KeyMapping { key: Keyboard::F4, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 20
    Mapping::Key(KeyMapping { key: Keyboard::F5, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 24
    Mapping::Key(KeyMapping { key: Keyboard::F11, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 28
    Mapping::Key(KeyMapping { key: Keyboard::F10, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 32
    Mapping::Key(KeyMapping { key: Keyboard::F9, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 36
    Mapping::Key(KeyMapping { key: Keyboard::F8, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 40
    Mapping::Key(KeyMapping { key: Keyboard::F7, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 44
    Mapping::Key(KeyMapping { key: Keyboard::F6, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Left hand upper combos
    // 48
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),

    // Right hand upper combos
    // 52
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Dead,

    // Left hand lower combos
    // 57
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Right hand lower combos
    // 62
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Thumb pairs "#A", "AO", "#U", "EU"
    // TODO: These are all layer shifts, wait for that to be implemented.
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
];

static NAV_MAP: [Mapping; NKEYS + 24] = [
    // 0
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 4
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 8
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 12
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 16
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 20
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 24
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 28
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // 32
    Mapping::Key(KeyMapping { key: Keyboard::End, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::RightArrow, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::ScrollLock, mods: Mods::empty() }),
    Mapping::Dead,

    // 36
    Mapping::Key(KeyMapping { key: Keyboard::PageUp, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::UpArrow, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::DeleteForward, mods: Mods::empty() }),
    Mapping::Dead,

    // 40
    Mapping::Key(KeyMapping { key: Keyboard::PageDown, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::DownArrow, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::DeleteBackspace, mods: Mods::empty() }),
    Mapping::Dead,

    // 44
    Mapping::Key(KeyMapping { key: Keyboard::Home, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::LeftArrow, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,

    // Left hand upper combos
    // 48
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),

    // Right hand upper combos
    // 52
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::CONTROL }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::SHIFT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::ALT }),
    Mapping::Key(KeyMapping { key: Keyboard::NoEventIndicated, mods: Mods::GUI }),
    Mapping::Dead,
    Mapping::Dead,

    // Left hand lower combos
    // 57
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Right hand lower combos
    // 62
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,

    // Thumb pairs "#A", "AO", "#U", "EU"
    // TODO: These are all layer shifts, wait for that to be implemented.
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
    Mapping::Dead,
];

static NKRO_MAP: [Mapping; NKEYS] = [
    // 0
    Mapping::Key(KeyMapping { key: Keyboard::Grave, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Z, mods: Mods::empty() }),
    Mapping::Dead,
    Mapping::Dead,

    // 4
    Mapping::Key(KeyMapping { key: Keyboard::Q, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::A, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard1, mods: Mods::empty() }),
    Mapping::Dead,

    // 8
    Mapping::Key(KeyMapping { key: Keyboard::W, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::S, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard2, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Z, mods: Mods::empty() }),

    // 12
    Mapping::Key(KeyMapping { key: Keyboard::E, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::D, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard3, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::X, mods: Mods::empty() }),

    // 16
    Mapping::Key(KeyMapping { key: Keyboard::R, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::F, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard4, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::C, mods: Mods::empty() }),

    // 20
    Mapping::Key(KeyMapping { key: Keyboard::T, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::G, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard5, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::V, mods: Mods::empty() }),

    // 24
    Mapping::Key(KeyMapping { key: Keyboard::LeftBrace, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Apostrophe, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Minus, mods: Mods::empty() }),
    Mapping::Dead,

    // 28
    Mapping::Key(KeyMapping { key: Keyboard::P, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Semicolon, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard0, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::ForwardSlash, mods: Mods::empty() }),

    // 32
    Mapping::Key(KeyMapping { key: Keyboard::O, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::L, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard9, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Dot, mods: Mods::empty() }),

    // 36
    Mapping::Key(KeyMapping { key: Keyboard::I, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::K, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard8, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Comma, mods: Mods::empty() }),

    // 40
    Mapping::Key(KeyMapping { key: Keyboard::U, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::J, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard7, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::M, mods: Mods::empty() }),

    // 44
    Mapping::Key(KeyMapping { key: Keyboard::Y, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::H, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::Keyboard6, mods: Mods::empty() }),
    Mapping::Key(KeyMapping { key: Keyboard::N, mods: Mods::empty() }),
];

// Combination keys. Each of these pairs will register as the entry for its
// index in this list, starting at NKEY. Each pair should have the lowest
// scancode first.
static COMBOS: [[u8; 2]; 24] = [
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
    [25, 26],

    // Pairs from the thumb keys.
    [15, 19],
    [19, 23],
    [39, 43],
    [43, 47],
];
