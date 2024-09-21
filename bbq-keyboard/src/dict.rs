//! Dictionary operations.

extern crate alloc;

use alloc::{vec::Vec, rc::Rc, string::ToString};

use bbq_steno::{memdict::MemDict, dict::{Translator, TypeAction}, Stroke};
use bbq_steno_macros::stroke;
use crate::log::info;

use crate::Timable;

pub struct Dict {
    // The translation dictionary.
    xlat: Option<Translator>,

    // Are we in "raw" mode.
    raw: bool,
}

impl Dict {
    pub fn new() -> Self {
        let xlat = unsafe {
            // MemDict::from_raw_ptr(0x10200000 as *const u8)
            // With the 8MB devices, move the dictionary down to 1MB, as the
            // dictionaries seem to be about 6.5MB.
            MemDict::from_raw_ptr(0x10100000 as *const u8)
        };
        let xlat = xlat.map(|d| Translator::new(Rc::new(d)));
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
                text: text,
            });
            return result;
        }

        // Otherwise, process through the dictionary.
        if let Some(xlat) = self.xlat.as_mut() {
            let start = timer.get_ticks();
            xlat.add(stroke);
            let stop = timer.get_ticks();
            while let Some(action) = xlat.next_action() {
                info!("Key: delete {}, type {} {}us", action.remove, action.text.len(),
                stop - start);
                result.push(action);
            }
        }
        result
    }
}
