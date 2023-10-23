//! Steno key handling.

use crate::{EventQueue, Event, KeyEvent};
use crate::modifiers::Modifiers;

pub use bbq_steno::Stroke;
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
}

impl RawStenoHandler {
    pub fn new() -> Self {
        RawStenoHandler {
            seen: Stroke::empty(),
            down: Stroke::empty(),
            modifier: Modifiers::new(),
        }
    }

    // For now, we don't do anything with the tick, but it will be needed when
    // trying to implement the hold modes.
    pub fn tick(&mut self) {}
    pub fn poll(&mut self) {}

    // Handle a single event.
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        let key = event.key();
        if key as usize >= LEFT_KEYS.len() {
            return;
        }
        if let Some(st) = LEFT_KEYS[key as usize] {
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

static LEFT_KEYS: &[Option<Stroke>] = &[
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
