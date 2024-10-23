//! Dictionary operations.

extern crate alloc;

use alloc::{rc::Rc, string::ToString, vec::Vec};

use bbq_steno::{dict::{Joined, Joiner, Lookup}, memdict::MemDict, Stroke};
use bbq_steno_macros::stroke;
use crate::log::info;

use crate::Timable;

pub struct Dict {
    // The translation engine.
    lookup: Lookup,

    // The "joining" engine.
    joiner: Joiner,

    // Are we in "raw" mode.
    raw: bool,
}

impl Dict {
    pub fn new() -> Self {
        let xlat = unsafe {
            // MemDict::from_raw_ptr(0x10200000 as *const u8)
            // With the 8MB devices, move the dictionary down to 1MB, as the
            // dictionaries seem to be about 6.5MB.
            MemDict::from_raw_ptr(0x10200000 as *const u8)
        };
        let xlat: Vec<_> = xlat.into_iter().map(|d| Rc::new(d) as bbq_steno::dict::Dict).collect();
        info!("Found {} steno dictionaries", xlat.len());
        let lookup = Lookup::new(xlat);
        let joiner = Joiner::new();
        Dict {
            lookup,
            joiner,
            raw: false,
        }
    }

    pub fn handle_stroke(&mut self, stroke: Stroke, timer: &dyn Timable) -> Vec<Joined> {
        let mut result = Vec::new();

        // Special check for the raw mode stroke.  Use it to toggle raw mode.
        if stroke == stroke!("RA*U") {
            self.raw = !self.raw;
            return result;
        }

        // If we are in raw mode, just type out the converted stroke.
        if self.raw {
            let mut text = stroke.to_string();
            text.push(' ');
            result.push(Joined::Type {
                remove: 0,
                append: text,
            });
            return result;
        }

        // The xlat is always present as it will just do nothing if there
        // are no dictionaries present.
        let start = timer.get_ticks();
        let action = self.lookup.add(stroke);
        self.joiner.add(action);
        let stop = timer.get_ticks();
        while let Some(action) = self.joiner.pop(0) {
            info!("Key: {:?} {}us", action,
            stop - start);
            result.push(action);
        }
        result
    }
}
