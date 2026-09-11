//! Synthetic Orsy logs, with known ground truth.
//!
//! The Orsy counterpart of the parent module: a target text becomes the key
//! events a writer would produce typing it in Orsy, with the mistakes dialled
//! in deliberately, so that the replay and the Swift trainer's stroke engine
//! have something with a known answer to be checked against.  Everything is
//! periodic rather than random, for the same reason as in the parent: a
//! golden file has to be reproducible.
//!
//! The division of each word into strokes is [`Writer`]'s, the fewest strokes
//! that spell it, so the plan is also what a trainer would hint.  What each
//! stroke puts on the screen is worked out by running the plan through the
//! layout's own [`Output`] stage, spaces and undo included, so the ground
//! truth cannot disagree with the firmware about spacing.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bbq_orsy::chord::HAND_MASK;
use bbq_orsy::tables::{commands, Outer, Patterns, Second, Vowel};
use bbq_orsy::{Chord, Op, Ops, Output, Writer};

use super::scan_for;
use crate::layout::orsy::{OrsyManager, StrokeOutcome};
use crate::replay::KeyLogEvent;
use crate::Side;

/// How the log's writer types Orsy.
///
/// The defaults describe a tidy writer: strokes struck rather than assembled,
/// and nothing going wrong.
#[derive(Clone, Debug)]
pub struct OrsyStyle {
    /// The time of the first key of the log.
    pub start_ms: u32,
    /// Milliseconds between consecutive keys of one stroke, left hand first.
    /// Zero is a struck stroke.
    pub spread_ms: u32,
    /// How long a stroke is held after its last key lands.
    pub hold_ms: u32,
    /// Milliseconds from a stroke's release to the next stroke's first key.
    /// Orsy commits on the first release, so strokes never overlap; a gap
    /// of zero still leaves a millisecond.
    pub gap_ms: u32,
    /// Every Nth stroke is typed wrong, then corrected.  Zero never does.
    pub error_every: u32,
    /// The kinds of error to make, cycled through in order.
    pub errors: Vec<OrsyError>,
}

impl Default for OrsyStyle {
    fn default() -> OrsyStyle {
        OrsyStyle {
            start_ms: 0,
            spread_ms: 0,
            hold_ms: 30,
            gap_ms: 60,
            error_every: 0,
            errors: alloc::vec![
                OrsyError::WrongStroke,
                OrsyError::WrongSeries,
                OrsyError::DeadStroke,
            ],
        }
    }
}

/// The ways a stroke can be got wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrsyError {
    /// A different stroke entirely -- the next stroke of the text, typed
    /// early.  Taken back with the undo command.
    WrongStroke,
    /// One Series wrong: the coda (or, failing that, the onset or the vowel)
    /// replaced by another shape that still makes a syllable.  Taken back a
    /// character at a time, with backspaces played through the Dosh escape.
    WrongSeries,
    /// A chord that is neither a syllable nor a command.  It types nothing,
    /// so there is nothing to take back; the writer just types the right
    /// stroke next.
    DeadStroke,
}

/// Why a stroke is in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrsyPurpose {
    /// A stroke typing part of the target text.
    Text,
    /// A stroke deliberately typed wrong.
    Error(OrsyError),
    /// The undo command, taking back the stroke before it.
    Undo,
    /// A backspace through the Dosh escape, taking back one character.
    Backspace,
    /// The stroke retyped after a mistake.
    Retype,
}

/// One stroke the generator put in the log, and what it meant by it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedStroke {
    /// When the stroke's first key goes down.
    pub press_ms: u32,
    /// The stroke actually typed.
    pub chord: Chord,
    /// The stroke that was meant, which differs from `chord` only for an
    /// error.
    pub intended: Chord,
    /// Why it is here.
    pub purpose: OrsyPurpose,
    /// What the stroke put on the screen: the characters, a space before
    /// them included, or a `\u{8}` per character taken back.  Empty for a
    /// dead stroke.
    pub types: String,
}

/// A generated log, with what the generator meant by it.
#[derive(Clone, Debug)]
pub struct OrsyLog {
    /// The key events, in time order.
    pub events: Vec<KeyLogEvent>,
    /// The strokes the generator intended, in order.
    pub planned: Vec<PlannedStroke>,
    /// The target text, for reference.
    pub target: String,
}

impl OrsyLog {
    /// How many strokes of each purpose the log holds.
    pub fn count(&self, purpose: OrsyPurpose) -> usize {
        self.planned.iter().filter(|p| p.purpose == purpose).count()
    }

    /// What the screen shows once everything is typed and taken back.
    pub fn screen(&self) -> String {
        let mut out = String::new();
        for stroke in &self.planned {
            for ch in stroke.types.chars() {
                if ch == '\u{8}' {
                    out.pop();
                } else {
                    out.push(ch);
                }
            }
        }
        out
    }
}

/// The Dosh escape playing a backspace: the one-shot shape on the left, and
/// Dosh's backspace key on the right.
const ESCAPED_BACKSPACE: Chord = Chord::new(commands::DOSH_ONESHOT, 0x100);

/// The undo command.
const UNDO: Chord = Chord::new(commands::UNDO, 0);

/// Generate the log for a target text: lowercase words separated by single
/// spaces.  Fails, naming the word, if the rules cannot spell one.
pub fn synth_orsy(target: &str, style: &OrsyStyle) -> Result<OrsyLog, String> {
    let writer = Writer::new();
    let mut units = Vec::new();
    for word in target.split(' ') {
        let strokes = writer
            .write(word)
            .ok_or_else(|| format!("the rules cannot spell {word:?}"))?;
        units.extend(strokes);
    }
    let mut planned = plan(&units, style);
    screen(&mut planned);
    let events = emit(&mut planned, style);
    Ok(OrsyLog {
        events,
        planned,
        target: target.to_string(),
    })
}

/// True on every Nth item, with zero meaning never.
fn every(period: u32, count: u32) -> bool {
    period != 0 && count % period == 0
}

/// Lay out the strokes, and insert the deliberate mistakes.
fn plan(units: &[Chord], style: &OrsyStyle) -> Vec<PlannedStroke> {
    let mut out = Vec::new();
    let mut errors = 0usize;
    for (num, &chord) in units.iter().enumerate() {
        let count = num as u32 + 1;
        if every(style.error_every, count) && !style.errors.is_empty() {
            let kind = style.errors[errors % style.errors.len()];
            errors += 1;
            let next = units.get(num + 1).copied();
            let (kind, wrong) = wrong_stroke(chord, next, kind);
            out.push(PlannedStroke {
                press_ms: 0,
                chord: wrong,
                intended: chord,
                purpose: OrsyPurpose::Error(kind),
                types: String::new(),
            });
            match kind {
                OrsyError::WrongStroke => out.push(PlannedStroke {
                    press_ms: 0,
                    chord: UNDO,
                    intended: UNDO,
                    purpose: OrsyPurpose::Undo,
                    types: String::new(),
                }),
                // How many backspaces is decided once the screen is known;
                // one is planned here and `screen` adds the rest.
                OrsyError::WrongSeries => out.push(PlannedStroke {
                    press_ms: 0,
                    chord: ESCAPED_BACKSPACE,
                    intended: ESCAPED_BACKSPACE,
                    purpose: OrsyPurpose::Backspace,
                    types: String::new(),
                }),
                OrsyError::DeadStroke => (),
            }
            out.push(PlannedStroke {
                press_ms: 0,
                chord,
                intended: chord,
                purpose: OrsyPurpose::Retype,
                types: String::new(),
            });
            continue;
        }
        out.push(PlannedStroke {
            press_ms: 0,
            chord,
            intended: chord,
            purpose: OrsyPurpose::Text,
            types: String::new(),
        });
    }
    out
}

/// The stroke actually typed for a deliberate mistake, and the kind it
/// turned out to be: a kind that cannot be made for this stroke falls back
/// to a dead stroke, which always can.
fn wrong_stroke(intended: Chord, next: Option<Chord>, kind: OrsyError) -> (OrsyError, Chord) {
    let is_text = |c: Chord| matches!(OrsyManager::outcome(c), StrokeOutcome::Text(_));
    match kind {
        OrsyError::WrongStroke => match next {
            Some(next) if next != intended && is_text(next) => (kind, next),
            _ => wrong_stroke(intended, None, OrsyError::WrongSeries),
        },
        OrsyError::WrongSeries => match wrong_series(intended) {
            Some(wrong) => (kind, wrong),
            None => wrong_stroke(intended, None, OrsyError::DeadStroke),
        },
        OrsyError::DeadStroke => (kind, dead_near(intended)),
    }
}

/// One Series of the stroke replaced by another shape, keeping it a
/// syllable: the coda first, then the onset, then the vowel.
fn wrong_series(intended: Chord) -> Option<Chord> {
    let p = Patterns::of(intended)?;
    let is_text = |c: Chord| matches!(OrsyManager::outcome(c), StrokeOutcome::Text(_));
    if p.coda != Outer::Empty {
        for coda in Outer::ALL {
            if coda == p.coda {
                continue;
            }
            let wrong = Chord::new(intended.left, p.vowel.bits() | coda.bits());
            if is_text(wrong) {
                return Some(wrong);
            }
        }
    }
    if p.onset != Outer::Empty {
        for onset in Outer::ALL {
            if onset == p.onset {
                continue;
            }
            let wrong = Chord::new(onset.bits() | p.second.bits(), intended.right);
            if is_text(wrong) {
                return Some(wrong);
            }
        }
    }
    if p.vowel != Vowel::Empty {
        for vowel in Vowel::ALL {
            if vowel == p.vowel {
                continue;
            }
            let wrong = Chord::new(intended.left, vowel.bits() | p.coda.bits());
            if is_text(wrong) {
                return Some(wrong);
            }
        }
    }
    let _ = Second::Empty;
    None
}

/// The nearest chord to the intended one that is neither a syllable nor a
/// command, by how many keys differ.
fn dead_near(intended: Chord) -> Chord {
    for distance in 1..=4u32 {
        for left in 0u16..0x400 {
            for right in 0u16..0x400 {
                if (left | right) & !HAND_MASK != 0 || (left == 0 && right == 0) {
                    continue;
                }
                let chord = Chord::new(left, right);
                let differ = (left ^ intended.left).count_ones() + (right ^ intended.right).count_ones();
                if differ == distance && OrsyManager::outcome(chord) == StrokeOutcome::Dead {
                    return chord;
                }
            }
        }
    }
    unreachable!("every chord near {intended:?} is a syllable or a command")
}

/// Work out what each stroke puts on the screen, by running the plan through
/// the layout's output stage, and plan enough backspaces to take a wrong
/// stroke back a character at a time.
fn screen(planned: &mut Vec<PlannedStroke>) {
    let mut output = Output::new();
    let mut num = 0;
    while num < planned.len() {
        let mut ops = Ops::new();
        let stroke = &mut planned[num];
        match stroke.purpose {
            OrsyPurpose::Text | OrsyPurpose::Retype | OrsyPurpose::Error(_) => {
                match OrsyManager::outcome(stroke.chord) {
                    StrokeOutcome::Text(t) => output.stroke(&t, &mut ops),
                    StrokeOutcome::Dead => (),
                    other => panic!("planned a {other:?} as text"),
                }
            }
            OrsyPurpose::Undo => output.undo(&mut ops),
            OrsyPurpose::Backspace => {
                // The escape's backspace bypasses the output stage on the
                // keyboard, which is told about it afterwards; the same here.
                output.erase();
                ops = Ops::new();
                stroke.types = "\u{8}".to_string();
                num += 1;
                continue;
            }
        }
        stroke.types = ops
            .as_slice()
            .iter()
            .map(|op| match op {
                Op::Char(ch) => *ch,
                Op::Backspace => '\u{8}',
            })
            .collect();
        // A wrong Series is taken back one character at a time: as many
        // backspaces as it typed, the one already planned included.
        if stroke.purpose == OrsyPurpose::Error(OrsyError::WrongSeries) {
            let typed = stroke.types.chars().count();
            let backspace = planned[num + 1].clone();
            for _ in 1..typed {
                planned.insert(num + 1, backspace.clone());
            }
        }
        num += 1;
    }
}

/// Turn the plan into key events, filling each stroke's press time in as it
/// goes.  The keys land in bit order, left hand first, `spread_ms` apart,
/// and all come up together.
fn emit(planned: &mut [PlannedStroke], style: &OrsyStyle) -> Vec<KeyLogEvent> {
    let mut events = Vec::new();
    let mut cursor = style.start_ms;
    for stroke in planned.iter_mut() {
        let mut keys: Vec<u8> = Vec::new();
        for (side, code) in [(Side::Left, stroke.chord.left), (Side::Right, stroke.chord.right)] {
            for bit in 0..10 {
                if code & (1 << bit) != 0 {
                    keys.push(scan_for(side, bit));
                }
            }
        }
        stroke.press_ms = cursor;
        let mut key_ms = cursor;
        for (num, key) in keys.iter().enumerate() {
            if num != 0 {
                key_ms += style.spread_ms;
            }
            events.push(KeyLogEvent { time_ms: key_ms, key: *key, press: true });
        }
        let release_ms = key_ms + style.hold_ms.max(1);
        for key in &keys {
            events.push(KeyLogEvent { time_ms: release_ms, key: *key, press: false });
        }
        cursor = release_ms + style.gap_ms.max(1);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tidy log spells its target, one stroke per syllable.
    #[test]
    fn clean() {
        let log = synth_orsy("ten tennis", &OrsyStyle::default()).unwrap();
        assert_eq!(log.planned.len(), 3);
        assert_eq!(log.screen(), "ten tennis");
        assert!(log.planned.iter().all(|p| p.purpose == OrsyPurpose::Text));
        assert_eq!(log.planned[1].types, " ten");
        assert_eq!(log.events.len(), 2 * (4 + 3 + 4));
    }

    /// Each kind of mistake is made and taken back, and the screen still ends
    /// up right.
    #[test]
    fn mistakes_are_corrected() {
        let style = OrsyStyle {
            error_every: 2,
            ..OrsyStyle::default()
        };
        let log = synth_orsy("ten sit tin net sits tents", &OrsyStyle::default()).unwrap();
        assert_eq!(log.screen(), "ten sit tin net sits tents");
        let log = synth_orsy("ten sit tin net sits tents", &style).unwrap();
        assert_eq!(log.screen(), "ten sit tin net sits tents");
        // Eight strokes, so four errors, cycling through the three kinds.
        // The last stroke has no next stroke to type early, so its wrong
        // stroke falls back to a wrong Series.
        assert_eq!(log.count(OrsyPurpose::Error(OrsyError::WrongStroke)), 1);
        assert_eq!(log.count(OrsyPurpose::Error(OrsyError::WrongSeries)), 2);
        assert_eq!(log.count(OrsyPurpose::Error(OrsyError::DeadStroke)), 1);
        assert_eq!(log.count(OrsyPurpose::Undo), 1);
        assert_eq!(log.count(OrsyPurpose::Retype), 4);
        // A wrong Series gets as many backspaces as characters it typed, the
        // space before the word included.
        let typed: usize = log
            .planned
            .iter()
            .filter(|p| p.purpose == OrsyPurpose::Error(OrsyError::WrongSeries))
            .map(|p| p.types.chars().count())
            .sum();
        assert!(typed > 2);
        assert_eq!(log.count(OrsyPurpose::Backspace), typed);
        // Errors differ from what was meant; everything else is as meant.
        for p in &log.planned {
            match p.purpose {
                OrsyPurpose::Error(_) => assert_ne!(p.chord, p.intended),
                _ => assert_eq!(p.chord, p.intended),
            }
        }
    }

    /// A word the rules cannot spell is refused.
    #[test]
    fn unspellable() {
        assert!(synth_orsy("iraq", &OrsyStyle::default()).is_err());
    }
}
