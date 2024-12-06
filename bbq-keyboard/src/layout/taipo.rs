//! Taipo keyboard support.
//!
//! The Taipo keyboard layout is a little bit similar to Artsey, in that each
//! half of the keyboard is complete. However, it makes use of 2 thumb keys for
//! each half, resulting in each half having 10 keys.
//!
//! The two halves are completely symmetrical, and the intent is to be able to
//! freely type between the two halves, allowing, for example, rollover between
//! the halves.  As such, we have to maintain the state of the two halves
//! separately.
//!
//! Modifiers:
//!
//! The Taipo layout describes four modifier combos for each side, one for each
//! of the modifiers.  They are described as "one shot".  They way they are
//! implemented here, is that pressing the modifier sends the modifier
//! immediately.  When a non-modifier key is pressed, the modifiers will then
//! all be released.  The two thumb keys together is defined as a "null" key,
//! which will release any pressed modifiers without pressing any keys.  This
//! allows for modifiers to be pressed and released.
//!
//! In addition, modifiers can be made sticky by pressing them twice (it
//! actually makes all of the modifiers sticky, there isn't support for
//! releasing some modifiers).  The modifiers will remain pressed until the two
//! thumb keys are pressed together.  This is useful for some types of GUI
//! manipulation, such as holding down alt while pressing tab or arrow keys.

// TODO: Fn key support. The function key causes the next stroke or two, if they
// are numbers, to send function keys.

use arraydeque::ArrayDeque;
use usbd_human_interface_device::page::Keyboard;

// use crate::log::info;

use crate::{EventQueue, KeyEvent, Side, Mods, KeyAction, Event};

pub struct TaipoManager {
    /// Managing state for each side.
    sides: [SideManager; 2],
    /// Key events passed through.
    keys: TaipoEvents,

    /// Modifiers that are down.
    oneshot: Mods,

    /// Does the HID have a non-modifier key down?
    down: bool,
}

impl Default for TaipoManager {
    fn default() -> Self {
        TaipoManager {
            sides: [Default::default(), Default::default()],
            keys: TaipoEvents::new(),
            oneshot: Mods::empty(),
            down: false,
        }
    }
}

impl TaipoManager {
    /// Poll doesn't do anything.
    pub fn poll(&mut self) {
    }

    /// Tick is needed to track time.
    pub fn tick(&mut self, events: &mut dyn EventQueue) {
        self.sides[0].tick(&mut self.keys);
        self.sides[1].tick(&mut self.keys);

        // After polling, handle any events.
        while let Some(tevent) = self.keys.pop_front() {
            // info!("ev: p:{}, code:{:x}", tevent.is_press, tevent.code);
            if !tevent.is_press {
                // If a key is actually pressed, release it. This shouldn't
                // really need to be conditional.
                if self.down {
                    events.push(Event::Key(KeyAction::KeyRelease));
                    self.down = false;
                }
                continue;
            }

            // Look up the code to see if we have an action.
            match TAIPO_ACTIONS.iter().find(|e| e.code == tevent.code) {
                Some(Entry { action: Action::Simple(k), .. }) => {
                    self.release_nonmod(events);
                    events.push(Event::Key(KeyAction::KeyPress(*k, self.oneshot)));
                    self.down = true;
                    self.oneshot = Mods::empty();
                }
                Some(Entry { action: Action::Shifted(k), .. }) => {
                    self.release_nonmod(events);
                    events.push(Event::Key(KeyAction::KeyPress(*k, self.oneshot | Mods::SHIFT)));
                    self.down = true;
                    self.oneshot = Mods::empty();
                }
                Some(Entry { action: Action::OneShot(m), .. }) => {
                    let new_mods = self.oneshot | *m;

                    // If this modification adds any new modifiers, send a new
                    // event.
                    if new_mods != self.oneshot {
                        self.release_nonmod(events);
                        events.push(Event::Key(KeyAction::ModOnly(new_mods)))
                    }
                    self.oneshot |= *m;
                }
                Some(Entry { action: Action::Release, .. }) => {
                    if !self.oneshot.is_empty() {
                        events.push(Event::Key(KeyAction::KeyRelease));
                        self.oneshot = Mods::empty();
                    }
                }
                None => (),
            }
        }
        let _ = events;
    }

    /// Release any non-modifier keys.  Because of the alternation, which could
    /// be for the same key, we simply don't do any rollover, releasing any
    /// pressed non-modifier keys when a new key needs to be pressed.
    fn release_nonmod(&mut self, events: &mut dyn EventQueue) {
        if self.down {
            if self.oneshot.is_empty() {
                events.push(Event::Key(KeyAction::KeyRelease));
            } else {
                events.push(Event::Key(KeyAction::ModOnly(self.oneshot)));
            }
            self.down = false;
        }
    }

    pub fn handle_event(&mut self, event: KeyEvent, events: &mut dyn EventQueue) {
        let (is_press, code) = match event {
            KeyEvent::Press(code) => (true, code),
            KeyEvent::Release(code) => (false, code),
        };
        let (side, tcode) = if let Some(Some((side, tcode))) = SCAN_MAP.get(code as usize) {
            (side, tcode)
        } else {
            // Dead keys can just return.
            return;
        };
        /*
        let text_side = match side {
            Side::Left => "left",
            Side::Right => "right",
        };
        info!("taipo: p:{}, code:{}, side:{}, tcode:{:x}",
              is_press, code, text_side, tcode);
        */
        if is_press {
            self.sides[side.index()].press(*tcode);
        } else {
            self.sides[side.index()].release(*tcode, &mut self.keys);
        }
        let _ = events;
    }
}

/// For each side, this tracks the state of keys pressed on that side.
#[derive(Default)]
struct SideManager {
    /// Keys that are currently pressed.
    pressed: u16,
    /// Keys that have been seen.
    seen: u16,
    /// How many ticks since the last key pressed went down.
    age: u32,
    /// Set when we determined a key was pressed, and sent a code. No more
    /// changes will happen.
    down: bool,
}

// Manage the key presses and releases per-side.  We consider keys that come
// down within a given time interval to be pressed together.  This is more
// strict than what is done for steno.  However, we want to be able to handle
// rollover even just beyond the left-right alternating.

impl SideManager {
    fn press(&mut self, tcode: u16) {
        // info!("smpress: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
        // As long as we aren't in 'down' state, capture that this is part of
        // the key we want to send.
        if !self.down {
            self.seen |= tcode;
            self.age = 0;
        }
        self.pressed |= tcode;
        // info!("Usmpress: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
    }

    fn release(&mut self, tcode: u16, keys: &mut TaipoEvents) {
        // info!("smrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
        self.pressed &= !tcode;
        // If everything is released, and the timer hasn't expired, we need to
        // send down, and then release.
        if self.pressed == 0 {
            if !self.down {
                let _ = keys.push_back(TaipoEvent { is_press: true, code: self.seen });
                // info!("taipo: press {:x}", self.seen);
            }
            let _ = keys.push_back(TaipoEvent { is_press: false, code: self.seen });
            // info!("taipo: release {:x}", self.seen);
            *self = Default::default();
        }
        // info!("Usmrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);

    }

    fn tick(&mut self, keys: &mut TaipoEvents) {
        // If we already sent, or just if nothing has been pressed.
        if self.down || self.seen == 0 {
            return;
        }
        self.age = self.age.saturating_add(1);
        if self.age >= 50 {
            let _ = keys.push_back(TaipoEvent { is_press: true, code: self.seen });
            // info!("taipo: tpress {:x}", self.seen);
            self.down = true;
        }
    }
}

#[cfg(test)]
mod test_side_manager {
    use super::{SideManager, TaipoEvent, TaipoEvents};

    struct Tester {
        events: TaipoEvents,
        manager: SideManager,
    }

    impl Tester {
        fn new() -> Tester {
            Tester {
                events: TaipoEvents::new(),
                manager: SideManager::default(),
            }
        }

        fn press(&mut self, keys: u16) {
            self.manager.press(keys);
        }

        fn release(&mut self, keys: u16) {
            self.manager.release(keys, &mut self.events);
        }

        fn spin(&mut self, ticks: usize) {
            for _ in 0..ticks {
                self.manager.tick(&mut self.events);
            }
        }

        fn events(&mut self, events: &[TaipoEvent]) {
            // Ensure the events match.
            let mut gotten = Vec::new();
            while let Some(ev) = self.events.pop_front() {
                gotten.push(ev);
            }
            assert_eq!(&gotten, events);
        }
    }

    /// Test the basics of the side.  Simulate two keys being pressed, and that
    /// the event is sent when the timer expires.
    #[test]
    fn test_side_manager_basic() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.press(2);
        tester.spin(52);
        tester.events(&[TaipoEvent { is_press: true, code: 3 }]);
        tester.release(2);
        tester.events(&[]);
        tester.release(1);
        tester.events(&[TaipoEvent { is_press: false, code: 3 }]);
    }

    /// Test rollover.  Once a set of keys has been pressed, and sent, other
    /// keys can come in, which will be considered part of a new chord.  The
    /// rollover only works with different keys.
    #[test]
    fn test_rollover() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(51);
        tester.events(&[TaipoEvent { is_press: true, code: 1 }]);
        tester.press(2);
        tester.spin(51);
        tester.events(&[TaipoEvent { is_press: false, code: 1 },
                        TaipoEvent { is_press: true, code: 2 }]);
        tester.release(1);
        tester.events(&[]);
        tester.release(2);
        tester.events(&[TaipoEvent { is_press: false, code: 2 }]);
    }
}

/// A single press or release indicated by Taipo.
#[derive(Debug, Eq, PartialEq)]
struct TaipoEvent {
    is_press: bool,
    code: u16,
}

/// A queue of events recorded.
type TaipoEvents = ArrayDeque<TaipoEvent, 8>;

/// Mapping between scan codes, and Taipo codes.  Taipo codes are a 10 number,
/// with the top two bits as the two thumb keys, then the top row, and bottom
/// row, with bit order represented by the view from the right side.
#[cfg(feature = "proto3")]
static SCAN_MAP: [Option<(Side, u16)>; 48] = [
    // 0
    None,
    None,
    None,
    None,
    Some((Side::Left, 0x010)),

    // 5
    Some((Side::Left, 0x001)),
    None,
    None,
    Some((Side::Left, 0x020)),
    Some((Side::Left, 0x002)),

    // 10
    None,
    None,
    Some((Side::Left, 0x040)),
    Some((Side::Left, 0x004)),
    None,

    // 15
    None,
    Some((Side::Left, 0x080)),
    Some((Side::Left, 0x008)),
    None,
    Some((Side::Left, 0x100)),

    // 20
    None,
    None,
    None,
    Some((Side::Left, 0x200)),
    None,

    // 25
    None,
    None,
    None,
    Some((Side::Right, 0x010)),
    Some((Side::Right, 0x001)),

    // 30
    None,
    None,
    Some((Side::Right, 0x020)),
    Some((Side::Right, 0x002)),
    None,

    // 35
    None,
    Some((Side::Right, 0x040)),
    Some((Side::Right, 0x004)),
    None,
    None,

    // 40
    Some((Side::Right, 0x080)),
    Some((Side::Right, 0x008)),
    None,
    Some((Side::Right, 0x100)),
    None,

    // 45
    None,
    None,
    Some((Side::Right, 0x200)),
];

#[cfg(feature = "proto2")]
static SCAN_MAP: [Option<(Side, u16)>; 30] = [
    // 0
    Some((Side::Left, 0x200)),
    Some((Side::Left, 0x100)),
    None,
    None,
    None,

    // 5
    Some((Side::Left, 0x008)),
    Some((Side::Left, 0x080)),
    Some((Side::Left, 0x004)),
    Some((Side::Left, 0x040)),
    Some((Side::Left, 0x020)),

    // 10
    Some((Side::Left, 0x002)),
    Some((Side::Left, 0x010)),
    Some((Side::Left, 0x001)),
    None,
    None,

    // 15
    Some((Side::Right, 0x200)),
    Some((Side::Right, 0x100)),
    None,
    None,
    None,

    // 20
    Some((Side::Right, 0x008)),
    Some((Side::Right, 0x080)),
    Some((Side::Right, 0x004)),
    Some((Side::Right, 0x040)),
    Some((Side::Right, 0x020)),

    // 25
    Some((Side::Right, 0x002)),
    Some((Side::Right, 0x010)),
    Some((Side::Right, 0x001)),
    None,
    None,
];

/// An Action is what should happen when particular key or combo is pressed.
/// Taipo does not have anything that acts as a shift key, as all keys are
/// pressed together (like steno).
enum Action {
    Simple(Keyboard),
    Shifted(Keyboard),
    OneShot(Mods),
    Release,
}

/// The mapping between each key and its Action.
struct Entry {
    code: u16,
    action: Action,
}

static TAIPO_ACTIONS: [Entry; 126] = [
    // The thumb keys by themselves.
    Entry { code: 0x100, action: Action::Simple(Keyboard::Space), },
    Entry { code: 0x200, action: Action::Simple(Keyboard::DeleteBackspace), },

    // The thumb keys together releases any modifiers.
    Entry { code: 0x300, action: Action::Release, },

    // Tab and variants.
    Entry { code: 0x0e0, action: Action::Simple(Keyboard::Tab), },
    Entry { code: 0x1e0, action: Action::Simple(Keyboard::DeleteForward), },
    Entry { code: 0x2e0, action: Action::Simple(Keyboard::Insert), },

    // Enter and variants
    Entry { code: 0x00e, action: Action::Simple(Keyboard::ReturnEnter), },
    Entry { code: 0x10e, action: Action::Simple(Keyboard::Escape), },

    // The single letters, with shift, and the punctuation below these.
    Entry { code: 0x001, action: Action::Simple(Keyboard::A), },
    Entry { code: 0x101, action: Action::Shifted(Keyboard::A), },
    Entry { code: 0x201, action: Action::Shifted(Keyboard::Comma), },

    Entry { code: 0x002, action: Action::Simple(Keyboard::O), },
    Entry { code: 0x102, action: Action::Shifted(Keyboard::O), },
    Entry { code: 0x202, action: Action::Shifted(Keyboard::LeftBrace), },

    Entry { code: 0x004, action: Action::Simple(Keyboard::T), },
    Entry { code: 0x104, action: Action::Shifted(Keyboard::T), },
    Entry { code: 0x204, action: Action::Simple(Keyboard::LeftBrace), },

    Entry { code: 0x008, action: Action::Simple(Keyboard::E), },
    Entry { code: 0x108, action: Action::Shifted(Keyboard::E), },
    Entry { code: 0x208, action: Action::Shifted(Keyboard::Keyboard9), },

    Entry { code: 0x010, action: Action::Simple(Keyboard::R), },
    Entry { code: 0x110, action: Action::Shifted(Keyboard::R), },
    Entry { code: 0x210, action: Action::Shifted(Keyboard::Dot), },

    Entry { code: 0x020, action: Action::Simple(Keyboard::S), },
    Entry { code: 0x120, action: Action::Shifted(Keyboard::S), },
    Entry { code: 0x220, action: Action::Shifted(Keyboard::RightBrace), },

    Entry { code: 0x040, action: Action::Simple(Keyboard::N), },
    Entry { code: 0x140, action: Action::Shifted(Keyboard::N), },
    Entry { code: 0x240, action: Action::Simple(Keyboard::RightBrace), },

    Entry { code: 0x080, action: Action::Simple(Keyboard::I), },
    Entry { code: 0x180, action: Action::Shifted(Keyboard::I), },
    Entry { code: 0x280, action: Action::Shifted(Keyboard::Keyboard0), },

    // Paired letters, shifted, and number/symbol.
    Entry { code: 0x0c0, action: Action::Simple(Keyboard::Y), },
    Entry { code: 0x1c0, action: Action::Shifted(Keyboard::Y), },
    Entry { code: 0x2c0, action: Action::Simple(Keyboard::Keyboard5), },

    Entry { code: 0x00c, action: Action::Simple(Keyboard::H), },
    Entry { code: 0x10c, action: Action::Shifted(Keyboard::H), },
    Entry { code: 0x20c, action: Action::Simple(Keyboard::Keyboard0), },

    Entry { code: 0x006, action: Action::Simple(Keyboard::U), },
    Entry { code: 0x106, action: Action::Shifted(Keyboard::U), },
    Entry { code: 0x206, action: Action::Simple(Keyboard::Keyboard2), },

    Entry { code: 0x009, action: Action::Simple(Keyboard::D), },
    Entry { code: 0x109, action: Action::Shifted(Keyboard::D), },
    Entry { code: 0x209, action: Action::Shifted(Keyboard::Keyboard2), },

    Entry { code: 0x0a0, action: Action::Simple(Keyboard::F), },
    Entry { code: 0x1a0, action: Action::Shifted(Keyboard::F), },
    Entry { code: 0x2a0, action: Action::Simple(Keyboard::Keyboard6), },

    Entry { code: 0x00a, action: Action::Simple(Keyboard::C), },
    Entry { code: 0x10a, action: Action::Shifted(Keyboard::C), },
    Entry { code: 0x20a, action: Action::Simple(Keyboard::Keyboard1), },

    Entry { code: 0x082, action: Action::Simple(Keyboard::K), },
    Entry { code: 0x182, action: Action::Shifted(Keyboard::K), },
    Entry { code: 0x282, action: Action::Shifted(Keyboard::Equal), },

    Entry { code: 0x041, action: Action::Simple(Keyboard::J), },
    Entry { code: 0x141, action: Action::Shifted(Keyboard::J), },
    Entry { code: 0x241, action: Action::Simple(Keyboard::Equal), },

    Entry { code: 0x081, action: Action::Simple(Keyboard::W), },
    Entry { code: 0x181, action: Action::Shifted(Keyboard::W), },
    Entry { code: 0x281, action: Action::Shifted(Keyboard::Keyboard7), },

    Entry { code: 0x030, action: Action::Simple(Keyboard::B), },
    Entry { code: 0x130, action: Action::Shifted(Keyboard::B), },
    Entry { code: 0x230, action: Action::Simple(Keyboard::Keyboard9), },

    Entry { code: 0x003, action: Action::Simple(Keyboard::L), },
    Entry { code: 0x103, action: Action::Shifted(Keyboard::L), },
    Entry { code: 0x203, action: Action::Simple(Keyboard::Keyboard4), },

    Entry { code: 0x060, action: Action::Simple(Keyboard::P), },
    Entry { code: 0x160, action: Action::Shifted(Keyboard::P), },
    Entry { code: 0x260, action: Action::Simple(Keyboard::Keyboard7), },

    Entry { code: 0x090, action: Action::Simple(Keyboard::G), },
    Entry { code: 0x190, action: Action::Shifted(Keyboard::G), },
    Entry { code: 0x290, action: Action::Shifted(Keyboard::Keyboard3), },

    Entry { code: 0x050, action: Action::Simple(Keyboard::Z), },
    Entry { code: 0x150, action: Action::Shifted(Keyboard::Z), },
    Entry { code: 0x250, action: Action::Simple(Keyboard::Keyboard8), },

    Entry { code: 0x005, action: Action::Simple(Keyboard::Q), },
    Entry { code: 0x105, action: Action::Shifted(Keyboard::Q), },
    Entry { code: 0x205, action: Action::Simple(Keyboard::Keyboard3), },

    Entry { code: 0x014, action: Action::Simple(Keyboard::X), },
    Entry { code: 0x114, action: Action::Shifted(Keyboard::X), },
    Entry { code: 0x214, action: Action::Shifted(Keyboard::Keyboard6), },

    Entry { code: 0x028, action: Action::Simple(Keyboard::V), },
    Entry { code: 0x128, action: Action::Shifted(Keyboard::V), },
    Entry { code: 0x228, action: Action::Shifted(Keyboard::Keyboard8), },

    Entry { code: 0x018, action: Action::Simple(Keyboard::M), },
    Entry { code: 0x118, action: Action::Shifted(Keyboard::M), },
    Entry { code: 0x218, action: Action::Shifted(Keyboard::Keyboard4), },

    // Punctuation only keys.
    Entry { code: 0x024, action: Action::Simple(Keyboard::ForwardSlash), },
    Entry { code: 0x124, action: Action::Simple(Keyboard::Backslash), },
    Entry { code: 0x224, action: Action::Shifted(Keyboard::Backslash), },

    Entry { code: 0x042, action: Action::Simple(Keyboard::Minus), },
    Entry { code: 0x142, action: Action::Shifted(Keyboard::Minus), },
    Entry { code: 0x242, action: Action::Shifted(Keyboard::Keyboard5), },

    Entry { code: 0x012, action: Action::Simple(Keyboard::Semicolon), },
    Entry { code: 0x112, action: Action::Shifted(Keyboard::Semicolon), },

    Entry { code: 0x084, action: Action::Shifted(Keyboard::ForwardSlash), },
    Entry { code: 0x184, action: Action::Shifted(Keyboard::Keyboard1), },

    Entry { code: 0x048, action: Action::Simple(Keyboard::Comma), },
    Entry { code: 0x148, action: Action::Simple(Keyboard::Dot), },
    Entry { code: 0x248, action: Action::Shifted(Keyboard::Grave), },

    // These aren't quite as per the chart, but the chart doesn't appear to be a
    // US layout.
    Entry { code: 0x021, action: Action::Simple(Keyboard::Apostrophe), },
    Entry { code: 0x121, action: Action::Shifted(Keyboard::Apostrophe), },
    Entry { code: 0x221, action: Action::Simple(Keyboard::Grave), },

    // The one shot keys.
    Entry { code: 0x088, action: Action::OneShot(Mods::SHIFT), },
    Entry { code: 0x188, action: Action::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x288, action: Action::Simple(Keyboard::PageDown), },

    Entry { code: 0x011, action: Action::OneShot(Mods::GUI), },
    Entry { code: 0x111, action: Action::Simple(Keyboard::RightArrow), },
    Entry { code: 0x211, action: Action::Simple(Keyboard::PageUp), },

    Entry { code: 0x044, action: Action::OneShot(Mods::CONTROL), },
    Entry { code: 0x144, action: Action::Simple(Keyboard::DownArrow), },
    Entry { code: 0x244, action: Action::Simple(Keyboard::End), },

    Entry { code: 0x022, action: Action::OneShot(Mods::ALT), },
    Entry { code: 0x122, action: Action::Simple(Keyboard::UpArrow), },
    Entry { code: 0x222, action: Action::Simple(Keyboard::Home), },

    // Map the function keys to the numbers, but with both thumbs pressed. F11
    // is 'v' and F12 is 'w'.
    Entry { code: 0x30c, action: Action::Simple(Keyboard::F10), },
    Entry { code: 0x30a, action: Action::Simple(Keyboard::F1), },
    Entry { code: 0x306, action: Action::Simple(Keyboard::F2), },
    Entry { code: 0x305, action: Action::Simple(Keyboard::F3), },
    Entry { code: 0x303, action: Action::Simple(Keyboard::F4), },
    Entry { code: 0x3c0, action: Action::Simple(Keyboard::F5), },
    Entry { code: 0x3a0, action: Action::Simple(Keyboard::F6), },
    Entry { code: 0x360, action: Action::Simple(Keyboard::F7), },
    Entry { code: 0x350, action: Action::Simple(Keyboard::F8), },
    Entry { code: 0x330, action: Action::Simple(Keyboard::F9), },
    Entry { code: 0x328, action: Action::Simple(Keyboard::F11), },
    Entry { code: 0x381, action: Action::Simple(Keyboard::F12), },
];
