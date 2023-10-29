//! Steno key handling.

use crate::{EventQueue, Event, KeyEvent, Timable};
use crate::modifiers::Modifiers;

pub use bbq_steno::Stroke;
use bbq_steno::dict::Translator;
use bbq_steno::memdict::MemDict;
use bbq_steno::stroke::EMPTY_STROKE;
use bbq_steno_macros::stroke;
use defmt::info;

pub struct RawStenoHandler {
    // Keys that have been currently seen.
    seen: Stroke,
    // Keys that are still pressed.
    down: Stroke,

    // Modifier.
    modifier: Modifiers,

    // The translation dictionary.
    xlat: Option<Translator<MemDict>>,
}

impl RawStenoHandler {
    pub fn new() -> Self {
        // For experimenting, setup a translation.
        let xlat = unsafe {
            MemDict::from_raw_ptr(0x10200000 as *const u8)
        };
        let xlat = xlat.map(|d| Translator::new(d));
        RawStenoHandler {
            seen: Stroke::empty(),
            down: Stroke::empty(),
            modifier: Modifiers::new(),
            xlat,
        }
    }

    // For now, we don't do anything with the tick, but it will be needed when
    // trying to implement the hold modes.
    pub fn tick(&mut self) {}
    pub fn poll(&mut self) {}

    // Handle a single event.
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue, timer: &dyn Timable) {
        let key = event.key();
        if key as usize >= STENO_KEYS.len() {
            return;
        }
        if let Some(st) = STENO_KEYS[key as usize] {
            if event.is_press() {
                self.seen = self.seen.merge(st);
                self.down = self.down.merge(st);
            } else {
                self.down = self.down.mask(st);
            }
        }

        if let Some(stroke) = self.get_stroke() {
            // For testing, Show emily's conversion.
            if let Some(text) = self.modifier.lookup(stroke) {
                info!("Mod: {}", text.as_str());
            }
            events.push(Event::RawSteno(stroke));

            // Run the translator.
            if let Some(xlat) = self.xlat.as_mut() {
                let start = timer.get_ticks();
                xlat.add(stroke);
                let stop = timer.get_ticks();
                info!("translator timing: {}", stop - start);
                while let Some(action) = xlat.next_action() {
                    info!("Key: delete {}, type {}", action.remove, action.text.len());
                }
            }
        }
    }

    // Handle the case of all keys up, and a steno stroke being available.
    fn get_stroke(&mut self) -> Option<Stroke> {
        if !self.seen.is_empty() && self.down.is_empty() {
            let result = self.seen;
            self.seen = EMPTY_STROKE;
            Some(result)
        } else {
            None
        }
    }
}

#[cfg(feature = "proto2")]
static STENO_KEYS: &[Option<Stroke>] = &[
    // Left side
    Some(stroke!("O")),
    Some(stroke!("A")),
    Some(stroke!("#")),
    Some(stroke!("^")),
    Some(stroke!("^")),
    Some(stroke!("R")),
    Some(stroke!("H")),
    Some(stroke!("W")),
    Some(stroke!("P")),
    Some(stroke!("T")),
    Some(stroke!("K")),
    Some(stroke!("*")),
    Some(stroke!("S")),
    Some(Stroke::empty()),
    Some(Stroke::empty()),

    // Right side
    Some(stroke!("E")),
    Some(stroke!("U")),
    Some(stroke!("#")),
    Some(stroke!("+")),
    Some(stroke!("+")),
    Some(stroke!("-R")),
    Some(stroke!("-F")),
    Some(stroke!("-B")),
    Some(stroke!("-P")),
    Some(stroke!("-L")),
    Some(stroke!("-G")),
    Some(stroke!("-T")),
    Some(stroke!("-S")),
    Some(stroke!("-D")),
    Some(stroke!("-Z")),
];

#[cfg(feature = "proto3")]
static STENO_KEYS: &[Option<Stroke>] = &[
    // Left
    Some(Stroke::empty()),
    Some(Stroke::empty()),
    Some(Stroke::empty()),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("*")),
    Some(stroke!("S")),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("T")),
    Some(stroke!("K")),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("P")),
    Some(stroke!("W")),
    Some(stroke!("#")),

    Some(stroke!("#")),
    Some(stroke!("H")),
    Some(stroke!("R")),
    Some(stroke!("A")),

    Some(stroke!("#")),
    Some(stroke!("^")),
    Some(stroke!("^")),
    Some(stroke!("O")),

    // Right
    Some(stroke!("#")), // What should this be?
    Some(stroke!("-D")),
    Some(stroke!("-Z")),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("-T")),
    Some(stroke!("-S")),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("-L")),
    Some(stroke!("-G")),
    Some(Stroke::empty()),

    Some(stroke!("#")),
    Some(stroke!("-P")),
    Some(stroke!("-B")),
    Some(stroke!("#")),

    Some(stroke!("#")),
    Some(stroke!("-F")),
    Some(stroke!("-R")),
    Some(stroke!("U")),

    Some(stroke!("#")),
    Some(stroke!("+")),
    Some(stroke!("+")),
    Some(stroke!("E")),

];
