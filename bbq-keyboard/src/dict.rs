//! Dictionary operations.

extern crate alloc;

use alloc::{vec::Vec, rc::Rc};

use bbq_steno::{memdict::MemDict, dict::{Translator, TypeAction}, Stroke};
use defmt::info;

use crate::Timable;

pub struct Dict {
    // The translation dictionary.
    xlat: Option<Translator>,
}

impl Dict {
    pub fn new() -> Self {
        let xlat = unsafe {
            MemDict::from_raw_ptr(0x10200000 as *const u8)
        };
        let xlat = xlat.map(|d| Translator::new(Rc::new(d)));
        Dict {
            xlat,
        }
    }

    pub fn handle_stroke(&mut self, stroke: Stroke, timer: &dyn Timable) -> Vec<TypeAction> {
        let mut result = Vec::new();
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
