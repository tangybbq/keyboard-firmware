//! Dictionary operations.

extern crate alloc;

use alloc::{rc::Rc, string::ToString, vec::Vec};

use bbq_steno::{memdict::MemDict, dict::{Translator, TypeAction}, Stroke};
use bbq_steno_macros::stroke;
use crate::log::info;

use crate::Timable;

pub struct Dict {
    // The translation dictionary.
    xlat: Translator,

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
        let xlat = Translator::new(xlat);
        Dict {
            xlat,
            raw: false,
        }
    }

    pub fn handle_stroke(&mut self, stroke: Stroke, timer: &dyn Timable) -> Vec<TypeAction> {
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
            result.push(TypeAction {
                remove: 0,
                text,
            });
            return result;
        }

        // The xlat is always present as it will just do nothing if there
        // are no dictionaries present.
        let start = timer.get_ticks();
        self.xlat.add(stroke);
        let stop = timer.get_ticks();
        while let Some(action) = self.xlat.next_action() {
            info!("Key: delete {}, type {} {}us", action.remove, action.text.len(),
            stop - start);
            result.push(action);
        }
        result
    }
}
