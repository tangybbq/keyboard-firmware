//! The Orsy layout: chord accumulation and the mode handler.
//!
//! Orsy borrows steno's front and none of its machinery.  The whole board is
//! one chord, accumulated across both hands and committed on the first
//! release, the way [`RawStenoHandler`](super::steno::RawStenoHandler) does
//! it; Taipo's per-side rollover is wrong here, because both hands form one
//! chord.  Everything behind that is `bbq_orsy`: a stroke translates on its
//! own, by rule, and nothing later revises it, so the output stage is a few
//! flags and a count of characters per stroke for undo.  See `docs/orsy/`.
//!
//! The scan codes map to chord bits through Taipo's [`SCAN_MAP`], as the
//! layout uses the same nine keys per hand that Dosh does.  The upper pinky,
//! which the mesa3 does not have, is ignored on boards that do.
//!
//! # Commands
//!
//! Five whole strokes are commands rather than syllables, each on a key
//! combination that is unassigned within its own group, so no syllable can
//! produce them.  Three are handled here: undo, a space on its own, and
//! capitalise the next word.  The other two escape to Dosh and need the
//! layout manager, so they are returned to it as an [`Escape`]: holding the
//! one-shot shape on the left plays the right hand's chord through the Dosh
//! table for that stroke only, and the toggle switches the keyboard to Dosh
//! outright.  The toggle's shape is unmapped in Dosh too, so the same chord
//! there switches back; that end is in the Dosh table.

use bbq_orsy::chord::HAND_MASK;
use bbq_orsy::tables::{commands, punctuation};
use bbq_orsy::{translate, Chord, Op, Ops, Output, Translation};

use crate::usb_typer::key_for_char;
use crate::{KeyAction, KeyEvent, Keyboard, Mods, Side};

use super::taipo::SCAN_MAP;
use super::LayoutActions;

/// What a committed stroke turned out to be.
///
/// Reported through [`LayoutActions::orsy_stroke`] for every stroke, so that
/// a host replay learns what was written from the engine rather than from
/// the keys it typed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrokeOutcome {
    /// A syllable, spelled by the rules.
    Text(Translation),
    /// The undo command.
    Undo,
    /// The space command.
    Space,
    /// The capitalise-next command.
    CapNext,
    /// A punctuation mark that takes part in the spacing.
    Punct(&'static punctuation::Mark),
    /// The toggle: the keyboard is switching to Dosh.
    ToggleDosh,
    /// The one-shot: this right-hand chord is played through the Dosh table.
    Dosh(u16),
    /// Not a syllable and not a command.  Nothing is typed.
    Dead,
}

/// A stroke the layout manager has to act on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Escape {
    /// The toggle chord: switch to Dosh.
    ToggleDosh,
    /// The one-shot: play this right-hand chord through the Dosh table.
    Dosh(u16),
}

pub struct OrsyManager {
    /// The keys currently held.
    down: Chord,
    /// Whether keys are being pressed (true) or released (false).  The stroke
    /// is sent on the first release after a press; further releases only
    /// take keys away, and a press starts a new stroke from what is still
    /// held.
    pressing: bool,
    /// The output stage.
    output: Output,
}

impl Default for OrsyManager {
    fn default() -> Self {
        OrsyManager::new()
    }
}

impl OrsyManager {
    pub const fn new() -> Self {
        OrsyManager {
            down: Chord::new(0, 0),
            pressing: true,
            output: Output::new(),
        }
    }

    /// Handle a key event.  Returns the escape if the event completed a
    /// stroke that the layout manager has to act on.
    pub async fn handle_event<ACT: LayoutActions>(
        &mut self,
        event: KeyEvent,
        actions: &ACT,
    ) -> Option<Escape> {
        let Some(Some((side, bit))) = SCAN_MAP.get(event.key() as usize) else {
            return None;
        };
        if bit & HAND_MASK == 0 {
            // The upper pinky, which is not an Orsy key.
            return None;
        }
        let stroke = self.down;
        let hand = match side {
            Side::Left => &mut self.down.left,
            Side::Right => &mut self.down.right,
        };
        if event.is_press() {
            *hand |= bit;
        } else {
            *hand &= !bit;
        }
        match (event.is_press(), self.pressing) {
            (true, true) => (),
            // The first release sends everything that was held.
            (false, true) => {
                self.pressing = false;
                // A stroke of nothing can only be a release left over from
                // another mode.
                if !stroke.is_empty() {
                    return self.stroke(stroke, actions).await;
                }
            }
            // A press while releasing starts a new stroke from what is
            // still held.
            (true, false) => self.pressing = true,
            (false, false) => (),
        }
        None
    }

    /// Capitalise the next word, for the layout manager to ask for after
    /// sentence-ending punctuation played through the Dosh escape.
    pub fn cap_next(&mut self) {
        self.output.cap_next();
    }

    /// A character was erased by a backspace played through the Dosh
    /// escape, so the output stage's idea of the text keeps up.
    pub fn backspace(&mut self) {
        self.output.erase();
    }

    /// Forget the keys held.  For a mode change: the releases of whatever is
    /// down go to the other mode, and would otherwise leave keys stuck in a
    /// stroke that never ends.  The output stage is kept, so that a word
    /// begun before a Dosh run gets its space after it.
    pub fn reset(&mut self) {
        self.down = Chord::new(0, 0);
        self.pressing = true;
    }

    /// What a completed stroke is.
    pub fn outcome(chord: Chord) -> StrokeOutcome {
        // A mark is a right-handed stroke on its own: the word-end marker plus outer
        // keys, which no vowel form and no command uses.
        if chord.left == 0 {
            if let Some(mark) = punctuation::lookup(chord.right) {
                return StrokeOutcome::Punct(mark);
            }
        }
        match (chord.left, chord.right) {
            (commands::DOSH_TOGGLE, 0) => StrokeOutcome::ToggleDosh,
            (commands::DOSH_ONESHOT, right) if right != 0 => StrokeOutcome::Dosh(right),
            (commands::UNDO, 0) => StrokeOutcome::Undo,
            (0, commands::SPACE) => StrokeOutcome::Space,
            (0, commands::CAP_NEXT) => StrokeOutcome::CapNext,
            _ => match translate(chord) {
                Some(t) => StrokeOutcome::Text(t),
                None => StrokeOutcome::Dead,
            },
        }
    }

    /// Act on a completed stroke.
    async fn stroke<ACT: LayoutActions>(&mut self, chord: Chord, actions: &ACT) -> Option<Escape> {
        let outcome = Self::outcome(chord);
        actions.orsy_stroke(chord, outcome).await;

        let mut ops = Ops::new();
        match outcome {
            StrokeOutcome::ToggleDosh => return Some(Escape::ToggleDosh),
            StrokeOutcome::Dosh(right) => return Some(Escape::Dosh(right)),
            StrokeOutcome::Undo => self.output.undo(&mut ops),
            StrokeOutcome::Space => self.output.space(&mut ops),
            StrokeOutcome::CapNext => self.output.cap_next(),
            StrokeOutcome::Punct(mark) => {
                self.output.mark(mark.text, mark.capitalises, &mut ops)
            }
            StrokeOutcome::Text(t) => self.output.stroke(&t, &mut ops),
            // Not a syllable.  Nothing is typed, which is the error signal
            // there is.
            StrokeOutcome::Dead => (),
        }

        for op in ops.as_slice() {
            let (key, mods) = match op {
                Op::Backspace => (Keyboard::DeleteBackspace, Mods::empty()),
                Op::Char(ch) => match key_for_char(*ch) {
                    Some(key) => key,
                    // The placeholder glyphs the theory has for a free
                    // vowel combination struck alone.
                    None => continue,
                },
            };
            actions.send_key(KeyAction::KeyPress(key, mods)).await;
            actions.send_key(KeyAction::KeyRelease).await;
        }
        None
    }
}
