//! Synthetic key event logs, with known ground truth.
//!
//! The third piece of `taipo-teacher.md`'s phase 0.  Turn a target string into
//! the key events a writer would produce typing it, with the sloppiness dialled
//! in deliberately: chords assembled finger by finger rather than struck, runs
//! on the same hand, chords that come apart, misfingerings, and the
//! backspace-and-retype that follows them.
//!
//! This exists because the analysis needs something to be right about.  A real
//! corpus says what happened but not what was meant; here both are known, so a
//! test can assert that the analysis found the three misfingerings that were
//! put there and nothing else.  Phase 4's Swift chord assembly is checked the
//! same way: this generates the log, the replay derives the events, both are
//! checked in, and the Swift side has to reproduce them.
//!
//! # Determinism
//!
//! Everything is periodic rather than random: "every third chord goes on the
//! same hand", not "a third of them do".  A golden file has to be reproducible,
//! and a test that says "this log contains exactly two dead chords" is worth
//! more than one that says "about two".  Realism is not the point; known
//! ground truth is.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::layout::export::char_for_key;
use crate::layout::dosh::DOSH_ACTIONS;
use crate::layout::taipo::{Action, Entry, TaipoVariant, CHORD_TIME, SCAN_MAP, TAIPO_ACTIONS};
use crate::replay::KeyLogEvent;
use crate::Side;

/// The longest string any table entry types, which bounds the greedy match.
const MAX_GRAM: usize = 8;

/// How many keys a deliberately dead chord may use.  Bounded so that it stays
/// a plausible mistake, and so that it still assembles inside the chord window
/// at any sensible spread.
const MAX_DEAD_KEYS: u32 = 4;

//////////////////////////////////////////////////////////////////////////////
// Style
//////////////////////////////////////////////////////////////////////////////

/// How the log's writer types.
///
/// The defaults describe a tidy writer: chords struck rather than assembled,
/// hands strictly alternating, and nothing going wrong.  Each knob makes the
/// log worse in one specific way.
#[derive(Clone, Debug)]
pub struct Style {
    /// Which chord table the writer's keyboard is using.
    pub variant: TaipoVariant,
    /// The time of the first key of the log.
    pub start_ms: u32,
    /// Milliseconds between consecutive keys of one chord.  Zero is a struck
    /// chord; tens of milliseconds is a chord being assembled.
    pub spread_ms: u32,
    /// How long a chord is held after its last key lands.
    pub hold_ms: u32,
    /// Milliseconds from a chord's release to the next chord's first key.
    ///
    /// Negative rolls the next chord into this one -- its first key lands
    /// before this one's last key comes up -- which is what alternating hands
    /// at speed actually looks like, and the only way to end a chord with the
    /// other hand.  A chord always keeps at least one millisecond to itself.
    pub gap_ms: i32,
    /// Every Nth chord goes on the same hand as the one before it, instead of
    /// alternating.  Zero never does.
    pub same_hand_every: u32,
    /// Every Nth multi-character gram is spelled out one letter at a time
    /// instead of chorded.  Zero never does.
    pub spell_every: u32,
    /// Every Nth chord is held past the chord window with half its keys down,
    /// so that the engine commits it in two pieces.  Zero never does.
    pub split_every: u32,
    /// Every Nth chord is typed wrong, then corrected with a backspace and
    /// retyped.  Zero never does.
    pub error_every: u32,
    /// The kinds of error to make, cycled through in order.
    pub errors: Vec<ErrorKind>,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            // Named rather than [`TaipoVariant::DEFAULT`], so that a test's claims about
            // what a chord types stay claims about a particular table.  A synthetic log
            // is bare key events -- the format has no `variant` marker -- so a caller
            // generating for the table the engine does not come up in has to put the
            // replay there itself, by tapping the toggle as a writer would.
            variant: TaipoVariant::Taipo,
            start_ms: 0,
            spread_ms: 0,
            hold_ms: 30,
            gap_ms: 60,
            same_hand_every: 0,
            spell_every: 0,
            split_every: 0,
            error_every: 0,
            errors: alloc::vec![
                ErrorKind::Misfingering,
                ErrorKind::WrongChord,
                ErrorKind::DeadChord
            ],
        }
    }
}

/// The ways a chord can be got wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// One key different from what was meant: an adjacent finger on the same
    /// row.  Usually types some other letter.
    Misfingering,
    /// The right fingers, one of them on the wrong row.
    ///
    /// The commonest real mistake by some way -- a third of the corrections in
    /// the first corpus -- and the one `Misfingering` cannot make, since that
    /// moves along a row and this moves across one.  Without it nothing here
    /// can generate the shape the analysis most needs to be tested against.
    RowSlip,
    /// A different chord entirely -- the next chord of the text, typed early,
    /// which is what recalling the wrong chord tends to look like.
    WrongChord,
    /// A chord the table has no entry for.  It types nothing at all, so there
    /// is nothing to backspace; the writer just types the right chord next.
    DeadChord,
}

//////////////////////////////////////////////////////////////////////////////
// What the generator meant
//////////////////////////////////////////////////////////////////////////////

/// Why a chord is in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A chord typing part of the target text.
    Text,
    /// A gram spelled out one letter at a time rather than chorded.
    Spelled,
    /// A chord deliberately typed wrong.
    Error(ErrorKind),
    /// The backspace correcting the chord before it.
    Correction,
    /// The chord retyped after a correction.
    Retype,
}

/// One chord the generator put in the log, and what it meant by it.
///
/// This is the ground truth: what a correct analysis of the log should say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// When the chord's first key goes down.
    pub press_ms: u32,
    /// The hand.
    pub side: Side,
    /// The chord actually typed.
    pub code: u16,
    /// The chord that was meant, which differs from `code` only for an error.
    pub intended: u16,
    /// Why it is here.
    pub purpose: Purpose,
    /// The text this chord contributes to the output, empty for an error, a
    /// correction, or a dead chord.
    pub types: String,
    /// Whether the chord was deliberately held apart, so that the engine will
    /// commit it as two chords rather than one.
    pub split: bool,
}

impl Planned {
    /// A chord with no time yet; `emit` fills that in as it lays the log out.
    fn new(side: Side, code: u16, intended: u16, purpose: Purpose) -> Planned {
        Planned {
            press_ms: 0,
            side,
            code,
            intended,
            purpose,
            types: String::new(),
            split: false,
        }
    }
}

/// A generated log, with what the generator meant by it.
#[derive(Clone, Debug)]
pub struct SynthLog {
    /// The key events, in time order.
    pub events: Vec<KeyLogEvent>,
    /// The chords the generator intended, in order.
    pub planned: Vec<Planned>,
    /// The target text, for reference.
    pub target: String,
}

impl SynthLog {
    /// The chords that carry the target text, skipping errors and corrections.
    pub fn text_chords(&self) -> impl Iterator<Item = &Planned> {
        self.planned
            .iter()
            .filter(|p| matches!(p.purpose, Purpose::Text | Purpose::Spelled | Purpose::Retype))
    }

    /// How many chords of each purpose the log holds.
    pub fn count(&self, purpose: Purpose) -> usize {
        self.planned.iter().filter(|p| p.purpose == purpose).count()
    }
}

//////////////////////////////////////////////////////////////////////////////
// Generation
//////////////////////////////////////////////////////////////////////////////

/// Generate the log for a target string.
///
/// Fails, naming the character, if the target has something the table cannot
/// type; a generator that quietly dropped it would produce a log that does not
/// say what it claims to.
pub fn synth(target: &str, style: &Style) -> Result<SynthLog, String> {
    let table = table_for(style.variant);
    let units = segment(target, table, style)?;
    let mut planned = plan(&units, table, style);
    check_spread(&planned, style)?;
    let events = emit(&mut planned, style);
    Ok(SynthLog {
        events,
        planned,
        target: target.to_string(),
    })
}

/// A chord whose keys are further apart than the chord window would not be
/// one chord at all, and the plan would be describing something the engine
/// never sees.  Say so rather than generating it.
fn check_spread(planned: &[Planned], style: &Style) -> Result<(), String> {
    for chord in planned {
        if chord.split {
            continue;
        }
        let keys = chord.code.count_ones();
        let spread = style.spread_ms * keys.saturating_sub(1);
        if keys > 1 && spread >= CHORD_TIME {
            return Err(format!(
                "a {keys}-key chord at spread_ms {} takes {spread}ms to assemble, \
                 which the {CHORD_TIME}ms chord window would split",
                style.spread_ms
            ));
        }
    }
    Ok(())
}

/// The chord table for a variant.
fn table_for(variant: TaipoVariant) -> &'static [Entry] {
    match variant {
        TaipoVariant::Taipo => TAIPO_ACTIONS,
        TaipoVariant::Dosh => DOSH_ACTIONS,
    }
}

/// What a table entry puts on the screen, if anything.
pub fn text_for(action: &Action) -> Option<String> {
    match action {
        Action::Simple(key) => char_for_key(*key, false).map(|c| c.to_string()),
        Action::Shifted(key) => char_for_key(*key, true).map(|c| c.to_string()),
        Action::Text(text) => Some(text.to_string()),
        Action::OneShot(_) | Action::Release | Action::Variant(_) => None,
    }
}

/// One thing the writer decided to type.
#[derive(Clone, Debug)]
struct Unit {
    code: u16,
    types: String,
    /// True when this came from breaking a multi-character gram up.
    spelled: bool,
}

/// Break the target into chords, longest match first.
///
/// This is not the cost model from `words/ngrams.py`; it is greedy, which is
/// enough to exercise the multi-character entries and to produce a log whose
/// segmentation is known.  Phase 3 brings the real one, and this can then use
/// it.
fn segment(target: &str, table: &'static [Entry], style: &Style) -> Result<Vec<Unit>, String> {
    // Longest text first, so that the greedy match prefers a gram.  Entries
    // are otherwise kept in table order, which decides ties.
    let mut by_text: Vec<(String, u16)> = table
        .iter()
        .filter_map(|entry| text_for(&entry.action).map(|text| (text, entry.code)))
        .collect();
    by_text.sort_by_key(|(text, _)| core::cmp::Reverse(text.len()));

    let chars: Vec<char> = target.chars().collect();
    let mut units = Vec::new();
    let mut at = 0;
    let mut grams = 0u32;
    while at < chars.len() {
        let rest: String = chars[at..chars.len().min(at + MAX_GRAM)].iter().collect();
        let found = by_text
            .iter()
            .find(|(text, _)| rest.starts_with(text.as_str()));
        let Some((text, code)) = found else {
            return Err(format!(
                "nothing in the table types {:?} (at offset {})",
                chars[at], at
            ));
        };
        let len = text.chars().count();
        if len > 1 {
            grams += 1;
            if every(style.spell_every, grams) {
                // Spell it out instead, one character at a time.
                for ch in text.chars() {
                    let single = ch.to_string();
                    let (_, code) = by_text
                        .iter()
                        .find(|(text, _)| *text == single)
                        .ok_or_else(|| format!("nothing in the table types {ch:?}"))?;
                    units.push(Unit {
                        code: *code,
                        types: single,
                        spelled: true,
                    });
                }
                at += len;
                continue;
            }
        }
        units.push(Unit {
            code: *code,
            types: text.clone(),
            spelled: false,
        });
        at += len;
    }
    Ok(units)
}

/// True on every Nth item, with zero meaning never.
fn every(period: u32, count: u32) -> bool {
    period != 0 && count % period == 0
}

/// Decide the hand for each chord, and insert the deliberate mistakes.
fn plan(units: &[Unit], table: &'static [Entry], style: &Style) -> Vec<Planned> {
    let mut out: Vec<Planned> = Vec::new();
    // Flipped before the first chord, so the log starts on the left.
    let mut side = Side::Right;
    let mut chords = 0u32;
    let mut errors = 0usize;

    for (num, unit) in units.iter().enumerate() {
        chords += 1;
        // The hands alternate, except when told to run on.
        if !every(style.same_hand_every, chords) {
            side = other(side);
        }
        let split = every(style.split_every, chords) && unit.code.count_ones() > 1;

        if every(style.error_every, chords) && !style.errors.is_empty() {
            let kind = style.errors[errors % style.errors.len()];
            errors += 1;
            let next = units.get(num + 1).map(|u| u.code);
            let wrong = wrong_code(unit.code, next, kind, table);
            out.push(Planned::new(side, wrong, unit.code, Purpose::Error(kind)));
            // A dead chord types nothing, so there is nothing to take back.
            // Everything else is corrected, on the other hand, which is where
            // a backspace belongs and is not always where it lands.
            if kind != ErrorKind::DeadChord {
                side = other(side);
                out.push(Planned::new(
                    side,
                    BACKSPACE,
                    BACKSPACE,
                    Purpose::Correction,
                ));
            }
            side = other(side);
            let mut retype = Planned::new(side, unit.code, unit.code, Purpose::Retype);
            retype.types = unit.types.clone();
            retype.split = split;
            out.push(retype);
            continue;
        }

        let purpose = if unit.spelled {
            Purpose::Spelled
        } else {
            Purpose::Text
        };
        let mut chord = Planned::new(side, unit.code, unit.code, purpose);
        chord.types = unit.types.clone();
        chord.split = split;
        out.push(chord);
    }
    out
}

/// The other hand.
fn other(side: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    }
}

/// The backspace chord, which is the same in both tables.
const BACKSPACE: u16 = 0x200;

/// The chord actually typed for a deliberate mistake.
fn wrong_code(
    intended: u16,
    next: Option<u16>,
    kind: ErrorKind,
    table: &'static [Entry],
) -> u16 {
    match kind {
        ErrorKind::Misfingering => misfinger(intended, table),
        ErrorKind::RowSlip => row_slip(intended, table),
        ErrorKind::WrongChord => match next {
            Some(next) if next != intended => next,
            _ => misfinger(intended, table),
        },
        ErrorKind::DeadChord => dead_code(intended, table),
    }
}

/// One key different: the lowest finger key of the chord, moved to the
/// adjacent finger on the same row.
///
/// Bits 0..3 are the bottom row, pinky to index, and 4..7 the top, so `bit ^ 1`
/// stays on the row and steps between pinky and ring, or between middle and
/// index.  It never reaches the ring-to-middle pair, which is a limitation of
/// the generator rather than of the layout.
fn misfinger(intended: u16, table: &'static [Entry]) -> u16 {
    for bit in 0..8 {
        if intended & (1 << bit) == 0 {
            continue;
        }
        let moved = (intended & !(1 << bit)) | (1 << (bit ^ 1));
        if moved != intended && moved.count_ones() == intended.count_ones() {
            return moved;
        }
    }
    // Nothing to move: add a key instead, which is still one key different.
    dead_code(intended, table)
}

/// The right fingers, one on the wrong row: the lowest finger key of the chord,
/// moved to the other row of the same column.
///
/// A finger's two keys are four bits apart, so the other row is one xor away.
/// The move is skipped where the destination is already held, since that would
/// drop a key rather than move one and the result would not be a row slip at
/// all.
fn row_slip(intended: u16, table: &'static [Entry]) -> u16 {
    for bit in 0..8 {
        if intended & (1 << bit) == 0 {
            continue;
        }
        let other = 1 << (bit ^ 4);
        if intended & other != 0 {
            continue;
        }
        return (intended & !(1 << bit)) | other;
    }
    // A chord of thumbs only has no row to slip on; fall back to something that
    // is at least wrong.
    dead_code(intended, table)
}

/// A chord the table has no entry for, as close to the intended one as
/// possible: the nearest unmapped chord by how many keys differ.
///
/// Nearest, rather than "one more finger down", because the space chord's
/// every one-key neighbour is a real chord, and the fallback of pressing the
/// whole hand is not a plausible mistake -- and is so wide that it would not
/// even assemble inside the chord window.
fn dead_code(intended: u16, table: &'static [Entry]) -> u16 {
    for distance in 1..=4 {
        for candidate in 1..0x400u16 {
            if (candidate ^ intended).count_ones() == distance
                && candidate.count_ones() <= MAX_DEAD_KEYS
                && !table.iter().any(|entry| entry.code == candidate)
            {
                return candidate;
            }
        }
    }
    // Neither table is anywhere near full, so this cannot happen.
    unreachable!("every chord near {intended:#05x} is mapped")
}

//////////////////////////////////////////////////////////////////////////////
// Emitting the key events
//////////////////////////////////////////////////////////////////////////////

/// Turn the plan into key events, filling each chord's press time in as it
/// goes so that the ground truth and the log cannot disagree about when
/// something happened.
fn emit(planned: &mut [Planned], style: &Style) -> Vec<KeyLogEvent> {
    let mut events: Vec<KeyLogEvent> = Vec::new();
    let mut cursor = style.start_ms;
    let sides: Vec<Side> = planned.iter().map(|chord| chord.side).collect();

    for (num, chord) in planned.iter_mut().enumerate() {
        let keys: Vec<u8> = (0..10)
            .filter(|bit| chord.code & (1 << bit) != 0)
            .map(|bit| scan_for(chord.side, bit))
            .collect();
        let first_ms = cursor;
        chord.press_ms = first_ms;

        // The keys land in bit order, `spread_ms` apart.  A split chord holds
        // the first half past the chord window before the rest arrives, which
        // is what makes the engine commit it as two.
        let half = keys.len() / 2;
        let mut last_ms = first_ms;
        for (num, key) in keys.iter().enumerate() {
            let key_ms = if num == 0 {
                first_ms
            } else if chord.split && num == half {
                last_ms + CHORD_TIME + 5
            } else {
                last_ms + style.spread_ms
            };
            events.push(KeyLogEvent {
                time_ms: key_ms,
                key: *key,
                press: true,
            });
            last_ms = key_ms;
        }

        let hold = style.hold_ms.max(1);
        let release_ms = last_ms + hold;
        for key in &keys {
            events.push(KeyLogEvent {
                time_ms: release_ms,
                key: *key,
                press: false,
            });
        }

        // The next chord starts after the gap, or early enough to roll into
        // this one when the gap is negative.  A negative gap only applies
        // between hands: two chords overlapping on the *same* hand are not two
        // chords, they are one, which the engine would be quite right to say
        // and which would make the plan a lie.  It never starts before this
        // chord's own first key.
        let rolls = sides.get(num + 1).is_some_and(|next| *next != chord.side);
        let gap = if rolls { style.gap_ms } else { style.gap_ms.max(1) };
        cursor = release_ms.saturating_add_signed(gap);
        cursor = cursor.max(first_ms + 1);
    }

    // Events in the same millisecond keep the order they were made in, which
    // is the order the fingers moved.
    events.sort_by_key(|event| event.time_ms);
    events
}

/// The key code for a chord bit on a hand, in the upper row position, which is
/// the position a log's key codes are recorded in.
fn scan_for(side: Side, bit: usize) -> u8 {
    let mask = 1u16 << bit;
    (0..SCAN_MAP.len() as u8)
        .find(|code| SCAN_MAP[*code as usize] == Some((side, mask)))
        .unwrap_or_else(|| panic!("no key for {side:?} bit {bit}"))
}
