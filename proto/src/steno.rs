//! Steno key handling.

use crate::{EventQueue, Event};

use bbq_keyboard::KeyEvent;

pub use bbq_steno::Stroke;
use bbq_steno::stroke::EMPTY_STROKE;

pub struct RawStenoHandler {
    // Keys that have been currently seen.
    seen: Stroke,
    // Keys that are still pressed.
    down: Stroke,
}

impl RawStenoHandler {
    pub fn new() -> Self {
        RawStenoHandler { seen: Stroke::empty(), down: Stroke::empty() }
    }

    // For now, we don't do anything with the tick, but it will be needed when
    // trying to implement the hold modes.
    pub fn tick(&mut self) {}
    pub fn poll(&mut self) {}

    // Handle a single event.
    pub(crate) fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        let key = event.key();
        if let Some(st) = LEFT_KEYS[key as usize] {
            if event.is_press() {
                self.seen = self.seen.merge(st);
                self.down = self.down.merge(st);
            } else {
                self.down = self.down.mask(st);
            }
        }

        if let Some(stroke) = self.get_stroke() {
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
    Some(Stroke::from_raw(0x1 << 13)), // 'O'
    Some(Stroke::from_raw(0x1 << 14)), // 'A'
    Some(Stroke::from_raw(0x1000000)), // '#'
    Some(Stroke::from_raw( 0x800000)), // '^'
    Some(Stroke::from_raw( 0x800000)), // '^'
    Some(Stroke::from_raw(0x1 << 15)), // 'R'
    Some(Stroke::from_raw(0x1 << 16)), // 'H'
    Some(Stroke::from_raw(0x1 << 17)), // 'W'
    Some(Stroke::from_raw(0x1 << 18)), // 'P'
    Some(Stroke::from_raw(0x1 << 20)), // 'T'
    Some(Stroke::from_raw(0x1 << 19)), // 'K'
    Some(Stroke::from_raw(0x1 << 12)), // '*'
    Some(Stroke::from_raw(0x1 << 21)), // 'S'
    Some(Stroke::from_raw(        0)), // ''
    Some(Stroke::from_raw(        0)), // ''

    // Right side
    Some(Stroke::from_raw(0x1 << 11)), // 'E'
    Some(Stroke::from_raw(0x1 << 10)), // 'U'
    Some(Stroke::from_raw(0x1000000)), // '#'
    Some(Stroke::from_raw( 0x400000)), // '+'
    Some(Stroke::from_raw( 0x400000)), // '+'
    Some(Stroke::from_raw(0x1 <<  8)), // '-R'
    Some(Stroke::from_raw(0x1 <<  9)), // '-F'
    Some(Stroke::from_raw(0x1 <<  6)), // '-B'
    Some(Stroke::from_raw(0x1 <<  7)), // '-P'
    Some(Stroke::from_raw(0x1 <<  5)), // '-L'
    Some(Stroke::from_raw(0x1 <<  4)), // '-G'
    Some(Stroke::from_raw(0x1 <<  3)), // '-T'
    Some(Stroke::from_raw(0x1 <<  2)), // '-S'
    Some(Stroke::from_raw(0x1 <<  1)), // '-D'
    Some(Stroke::from_raw(0x1 <<  0)), // '-Z'
];
/*
static RIGHT_KEYS: &[Option<Stroke>] = &[
    Some(Stroke::from_raw(0x1000000)),
    Some(Stroke::from_raw(0x2000)),
    Some(Stroke::from_raw(0x4000)),
    Some(Stroke::from_raw(0x1 << 0)),
    Some(Stroke::from_raw(0x1 << 1)),
    Some(Stroke::from_raw(0x1 << 2)),
    Some(Stroke::from_raw(0x1 << 3)),
    Some(Stroke::from_raw(0x1 << 4)),
    Some(Stroke::from_raw(0x1 << 5)),
    Some(Stroke::from_raw(0x1 << 6)),
    Some(Stroke::from_raw(0x1 << 7)),
    Some(Stroke::from_raw(0x1 << 8)),
    Some(Stroke::from_raw(0x1 << 9)),
    Some(Stroke::from_raw(0x1 << 10)),
    Some(Stroke::from_raw(0x8000000)),
    Some(Stroke::from_raw(0x8000000)),
    /*
    Some(Stroke::from_text_const("#")),
    Some(Stroke::from_text_const("U")),
    Some(Stroke::from_text_const("E")),
    Some(Stroke::from_text_const("-D")),
    Some(Stroke::from_text_const("-Z")),
    Some(Stroke::from_text_const("-S")),
    Some(Stroke::from_text_const("-T")),
    Some(Stroke::from_text_const("-G")),
    Some(Stroke::from_text_const("-L")),
    Some(Stroke::from_text_const("-P")),
    Some(Stroke::from_text_const("-B")),
    Some(Stroke::from_text_const("-F")),
    Some(Stroke::from_text_const("-R")),
    Some(Stroke::from_text_const("^")),
    Some(Stroke::from_text_const("^")),
    */
];
*/
