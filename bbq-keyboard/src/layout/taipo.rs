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

use crate::log::info;

use crate::{EventQueue, KeyEvent, Side};

pub struct TaipoManager {
    sides: [SideManager; 2],
}

impl Default for TaipoManager {
    fn default() -> Self {
        TaipoManager {
            sides: [Default::default(), Default::default()],
        }
    }
}

impl TaipoManager {
    /// Poll doesn't do anything.
    pub fn poll(&mut self) {
    }

    /// Tick is needed to track time.
    pub fn tick(&mut self, events: &mut dyn EventQueue) {
        self.sides[0].tick();
        self.sides[1].tick();
        let _ = events;
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
            self.sides[side.index()].release(*tcode);
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

// A few notes about this algorithm.  It is unclear what to do if a key comes
// down before others are release, but the timeout has passed, and we have sent
// the code.  I have decided to just ignore these keys, rather than send
// spurious events.

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

    fn release(&mut self, tcode: u16) {
        // info!("smrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
        self.pressed &= !tcode;
        // If everything is released, and the timer hasn't expired, we need to
        // send down, and then release.
        if self.pressed == 0 {
            if !self.down {
                info!("taipo: press {:x}", self.seen);
            }
            info!("taipo: release {:x}", self.seen);
            *self = Default::default();
        }
        // info!("Usmrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);

    }

    fn tick(&mut self) {
        // If we already sent, or just if nothing has been pressed.
        if self.down || self.seen == 0 {
            return;
        }
        self.age = self.age.saturating_add(1);
        if self.age >= 50 {
            info!("taipo: tpress {:x}", self.seen);
            self.down = true;
        }
    }
}

/// Mapping between scan codes, and Taipo codes.  Taipo codes are a 10 number,
/// with the top two bits as the two thumb keys, then the top row, and bottom
/// row, with bit order represented by the view from the right side.
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
