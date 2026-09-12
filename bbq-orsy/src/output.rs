//! The output stage: from translations to characters and backspaces.
//!
//! Much smaller than a steno typer, because nothing is ever retranslated: a
//! stroke's text is final the moment it is typed.  What is left is the space
//! between words, capitals, and undo.
//!
//! - **Spacing.**  A space goes between two strokes when the earlier one
//!   allows a space after it and the later one a space before it.  The
//!   translation's flags come straight from the word-boundary rule: an
//!   ending-form vowel allows a space after, a plain vowel does not, and a
//!   fragment with no vowel takes no space before.
//! - **Capitals.**  [`Output::cap_next`] capitalises the first letter of the
//!   next stroke.  [`Output::cap_previous`] walks back over the recent text
//!   and capitalises words already typed.
//! - **Undo.**  Each stroke records how many characters it typed, and undo
//!   backspaces over them.  That is the whole of it.
//!
//! The stage does not type anything itself: each call fills an [`Ops`] with
//! the characters and backspaces to send, so that the caller decides how.

use crate::compose::Translation;
use crate::tables::punctuation;

/// One thing for the keyboard to send.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// Type this character.
    Char(char),
    /// Send a backspace.
    Backspace,
}

/// How much recent text is kept, for retro-capitalisation.
const RECENT: usize = 64;

/// How many strokes back undo reaches.
const STROKES: usize = 8;

/// The most a single call can ask to be sent: retyping all of the recent
/// text after backspacing over it.
const OPS: usize = 2 * RECENT;

/// A buffer of operations, filled by one call on the [`Output`].
pub struct Ops {
    buf: [Op; OPS],
    len: usize,
}

impl Ops {
    pub const fn new() -> Ops {
        Ops { buf: [Op::Backspace; OPS], len: 0 }
    }

    fn push(&mut self, op: Op) {
        if self.len < OPS {
            self.buf[self.len] = op;
            self.len += 1;
        }
    }

    /// The operations, in the order to send them.
    pub fn as_slice(&self) -> &[Op] {
        &self.buf[..self.len]
    }

    /// The characters typed, with a backspace shown as `\u{8}`.  For tests
    /// and the host.
    #[cfg(feature = "std")]
    pub fn text(&self) -> String {
        self.as_slice()
            .iter()
            .map(|op| match op {
                Op::Char(ch) => *ch,
                Op::Backspace => '\u{8}',
            })
            .collect()
    }
}

impl Default for Ops {
    fn default() -> Self {
        Ops::new()
    }
}

/// What one stroke did, so that undo can take it back.
#[derive(Clone, Copy, Default)]
struct Stroke {
    /// Characters typed, the space before the word included.
    chars: u8,
    /// Whether a space was pending before the stroke, to put back.
    pending_space: bool,
}

/// The output stage.
pub struct Output {
    /// The most recent characters typed, oldest first.
    recent: [char; RECENT],
    recent_len: usize,
    /// The most recent strokes, oldest first.
    strokes: [Stroke; STROKES],
    strokes_len: usize,
    /// The last stroke allowed a space after it, so the next word gets one.
    pending_space: bool,
    /// The next letter typed is capitalised.
    pending_cap: bool,
}

impl Default for Output {
    fn default() -> Self {
        Output::new()
    }
}

impl Output {
    pub const fn new() -> Output {
        Output {
            recent: [' '; RECENT],
            recent_len: 0,
            strokes: [Stroke { chars: 0, pending_space: false }; STROKES],
            strokes_len: 0,
            pending_space: false,
            pending_cap: false,
        }
    }

    /// Type a stroke.
    pub fn stroke(&mut self, t: &Translation, ops: &mut Ops) {
        let record = Stroke { chars: 0, pending_space: self.pending_space };
        let mut count = 0u8;
        if self.pending_space && t.space_before && !t.is_empty() {
            self.emit(' ', ops);
            count += 1;
        }
        for ch in t.chars() {
            let ch = if self.pending_cap && ch.is_ascii_alphabetic() {
                self.pending_cap = false;
                ch.to_ascii_uppercase()
            } else {
                ch
            };
            self.emit(ch, ops);
            count = count.saturating_add(1);
        }
        self.pending_space = t.space_after;
        self.record(Stroke { chars: count, ..record });
    }

    /// Type a punctuation mark.
    ///
    /// It attaches to whatever came before, with no space, whether or not that word was
    /// closed.  Most owe a space to the next word; the apostrophe and the hyphen do not,
    /// so what follows binds straight on.  Recorded like any other stroke, so undo takes
    /// it back with the characters it typed.
    pub fn mark(&mut self, mark: &punctuation::Mark, ops: &mut Ops) {
        let record = Stroke { chars: 0, pending_space: self.pending_space };
        let mut count = 0u8;
        for ch in mark.text.chars() {
            self.emit(ch, ops);
            count = count.saturating_add(1);
        }
        self.pending_space = mark.space_after;
        // Never cleared here: a mark that does not capitalise should not cancel a capital
        // the writer has already asked for.
        self.pending_cap |= mark.capitalises;
        self.record(Stroke { chars: count, ..record });
    }

    /// The space command: a space on its own.
    pub fn space(&mut self, ops: &mut Ops) {
        let record = Stroke { chars: 1, pending_space: self.pending_space };
        self.emit(' ', ops);
        self.pending_space = false;
        self.record(record);
    }

    /// Capitalise the next letter typed.
    pub fn cap_next(&mut self) {
        self.pending_cap = true;
    }

    /// Capitalise the previous `n` words of the recent text.
    ///
    /// Backspaces to the start of the `n`th word back (or as far as the
    /// recent text reaches) and retypes it with each word's first letter in
    /// capitals.  Counts as a stroke, so it can be undone.
    pub fn cap_previous(&mut self, n: usize, ops: &mut Ops) {
        let text = &self.recent[..self.recent_len];
        // Walk back over n words: each word is a run of non-spaces, with
        // whatever spaces precede it.
        let mut start = self.recent_len;
        for _ in 0..n {
            while start > 0 && text[start - 1] == ' ' {
                start -= 1;
            }
            while start > 0 && text[start - 1] != ' ' {
                start -= 1;
            }
        }
        let tail = self.recent_len - start;
        let mut retyped = [' '; RECENT];
        let mut at_word_start = true;
        for (i, ch) in text[start..].iter().enumerate() {
            retyped[i] = if at_word_start && ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase()
            } else {
                *ch
            };
            at_word_start = *ch == ' ';
        }
        for _ in 0..tail {
            ops.push(Op::Backspace);
        }
        for ch in &retyped[..tail] {
            ops.push(Op::Char(*ch));
        }
        self.recent[start..self.recent_len].copy_from_slice(&retyped[..tail]);
        // Nothing to undo: the text is the same length, and undoing back
        // over a retype would only lose the capitals it added.
    }

    /// A character was erased by something other than this stage -- a
    /// backspace played through the Dosh escape -- so the record of the
    /// text keeps up with the screen.  Nothing is sent.
    ///
    /// The erased character comes off the last stroke's count, so an undo
    /// afterwards takes back only what is still there.  Erasing a space
    /// puts the space back on order, so the next word gets one again.
    pub fn erase(&mut self) {
        if self.recent_len == 0 {
            return;
        }
        self.recent_len -= 1;
        let erased = self.recent[self.recent_len];
        if erased == ' ' {
            self.pending_space = true;
        }
        if self.strokes_len > 0 {
            let last = &mut self.strokes[self.strokes_len - 1];
            last.chars = last.chars.saturating_sub(1);
        }
    }

    /// Take back the last stroke.
    pub fn undo(&mut self, ops: &mut Ops) {
        if self.strokes_len == 0 {
            return;
        }
        self.strokes_len -= 1;
        let stroke = self.strokes[self.strokes_len];
        for _ in 0..stroke.chars {
            ops.push(Op::Backspace);
        }
        let keep = self.recent_len.saturating_sub(stroke.chars as usize);
        self.recent_len = keep;
        self.pending_space = stroke.pending_space;
        self.pending_cap = false;
    }

    /// The recent text, for tests.
    #[cfg(feature = "std")]
    pub fn recent(&self) -> String {
        self.recent[..self.recent_len].iter().collect()
    }

    fn emit(&mut self, ch: char, ops: &mut Ops) {
        ops.push(Op::Char(ch));
        if self.recent_len == RECENT {
            self.recent.copy_within(1.., 0);
            self.recent_len -= 1;
        }
        self.recent[self.recent_len] = ch;
        self.recent_len += 1;
    }

    fn record(&mut self, stroke: Stroke) {
        if self.strokes_len == STROKES {
            self.strokes.copy_within(1.., 0);
            self.strokes_len -= 1;
        }
        self.strokes[self.strokes_len] = stroke;
        self.strokes_len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::Chord;
    use crate::compose::translate;
    use crate::tables::{Outer, Second, Vowel};

    fn chord(s1: Outer, s2: Second, s3: Vowel, s4: Outer) -> Chord {
        Chord::new(s1.bits() | s2.bits(), s3.bits() | s4.bits())
    }

    /// Type a chord and return what was sent.
    fn stroke(out: &mut Output, c: Chord) -> String {
        let t = translate(c).expect("valid chord");
        let mut ops = Ops::new();
        out.stroke(&t, &mut ops);
        ops.text()
    }

    // Some strokes to type with.
    const TEN: (Outer, Second, Vowel, Outer) = (Outer::FP, Second::Empty, Vowel::Ue, Outer::N);
    const TE_: (Outer, Second, Vowel, Outer) = (Outer::FP, Second::Empty, Vowel::E, Outer::N);
    const NT: (Outer, Second, Vowel, Outer) = (Outer::Empty, Second::Empty, Vowel::Empty, Outer::FZN);
    const S: (Outer, Second, Vowel, Outer) = (Outer::S, Second::Empty, Vowel::Empty, Outer::Empty);
    /// A coda-only `s`, which leans back onto the syllable before it and closes the word.
    const S_CODA: (Outer, Second, Vowel, Outer) = (Outer::Empty, Second::Empty, Vowel::Empty, Outer::S);

    fn c(p: (Outer, Second, Vowel, Outer)) -> Chord {
        chord(p.0, p.1, p.2, p.3)
    }

    /// Words get a space between them, and none at the start.
    #[test]
    fn spaces_between_words() {
        let mut out = Output::new();
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        assert_eq!(out.recent(), "ten ten");
    }

    /// A plain vowel binds forward; a fragment binds back; a bare onset
    /// leans forward.
    #[test]
    fn binding() {
        let mut out = Output::new();
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(stroke(&mut out, c(TE_)), " ten");
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(stroke(&mut out, c(NT)), "nt");
        assert_eq!(stroke(&mut out, c(S)), " s");
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(out.recent(), "ten tentennt sten");
    }

    /// The space command puts a space in, and doesn't double up.
    #[test]
    fn space_command() {
        let mut out = Output::new();
        let mut ops = Ops::new();
        stroke(&mut out, c(TE_));
        out.space(&mut ops);
        assert_eq!(ops.text(), " ");
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        let mut ops = Ops::new();
        out.space(&mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(out.recent(), "ten ten ten");
    }

    /// A mark attaches to the word before it, closed or not, and the next word gets its
    /// space back; a sentence-ender capitalises what follows.
    #[test]
    fn marks() {
        fn find(text: &str) -> &'static punctuation::Mark {
            punctuation::ALL.iter().find(|m| m.text == text).expect("a mark")
        }
        let mut out = Output::new();
        let mut ops = Ops::new();
        // After a closed word.
        stroke(&mut out, c(TEN));
        out.mark(find("."), &mut ops);
        assert_eq!(ops.text(), ".");
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        // And after one left open, where the escape used to run the next word on.
        stroke(&mut out, c(TE_));
        let mut ops = Ops::new();
        out.mark(find(","), &mut ops);
        assert_eq!(ops.text(), ",");
        assert_eq!(out.recent(), "ten. Ten ten,");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        // Undo takes the word back, and then the mark, rather than leaving it stranded
        // the way an escaped mark did.
        let mut ops = Ops::new();
        out.undo(&mut ops);
        out.undo(&mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}\u{8}\u{8}\u{8}");
        assert_eq!(out.recent(), "ten. Ten ten");
    }

    /// The apostrophe and the hyphen bind forward, so what follows joins the same word.
    #[test]
    fn binding_marks() {
        fn find(text: &str) -> &'static punctuation::Mark {
            punctuation::ALL.iter().find(|m| m.text == text).expect("a mark")
        }
        let mut out = Output::new();
        let mut ops = Ops::new();
        // A word, an apostrophe, and a coda fragment: one word, and then a space.
        stroke(&mut out, c(TE_));
        out.mark(find("'"), &mut ops);
        assert_eq!(stroke(&mut out, c(S_CODA)), "s");
        assert_eq!(out.recent(), "ten's");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        // The hyphen joins two whole words.
        let mut out = Output::new();
        let mut ops = Ops::new();
        stroke(&mut out, c(TEN));
        out.mark(find("-"), &mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(out.recent(), "ten-ten");
    }

    /// Cap next capitalises the first letter of the next stroke, once.
    #[test]
    fn cap_next() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        out.cap_next();
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
    }

    /// Cap previous walks back over words and retypes them.
    #[test]
    fn cap_previous() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        stroke(&mut out, c(TEN));
        stroke(&mut out, c(TEN));
        let mut ops = Ops::new();
        out.cap_previous(2, &mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}Ten Ten");
        assert_eq!(out.recent(), "ten Ten Ten");
        // More words than there are is fine.
        let mut ops = Ops::new();
        out.cap_previous(9, &mut ops);
        assert_eq!(out.recent(), "Ten Ten Ten");
        assert_eq!(ops.as_slice().len(), 22);
    }

    /// Undo backspaces exactly what the stroke typed, space included, and
    /// puts the spacing state back.
    #[test]
    fn undo() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        stroke(&mut out, c(TE_));
        stroke(&mut out, c(NT));
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}");
        assert_eq!(out.recent(), "ten ten");
        // The undone stroke's spacing is forgotten: the plain vowel still
        // binds forward.
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        let mut ops = Ops::new();
        out.undo(&mut ops);
        out.undo(&mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}");
        assert_eq!(out.recent(), "ten");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        // Undo with nothing to undo does nothing.
        let mut out = Output::new();
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(ops.as_slice().len(), 0);
    }

    /// An erased character comes off the record and off the last stroke's
    /// undo count; an erased space is owed again.
    #[test]
    fn erase() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        stroke(&mut out, c(TEN));
        out.erase();
        out.erase();
        assert_eq!(out.recent(), "ten t");
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}");
        assert_eq!(out.recent(), "ten");
        // Erasing the space between words asks for it back.
        stroke(&mut out, c(TEN));
        out.erase();
        out.erase();
        out.erase();
        out.erase();
        assert_eq!(out.recent(), "ten");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        // Erasing into nothing is harmless.
        let mut out = Output::new();
        out.erase();
        assert_eq!(out.recent(), "");
    }

    /// Undo reaches back a bounded number of strokes.
    #[test]
    fn undo_depth() {
        let mut out = Output::new();
        for _ in 0..(STROKES + 2) {
            stroke(&mut out, c(TEN));
        }
        let mut ops = Ops::new();
        for _ in 0..(STROKES + 2) {
            out.undo(&mut ops);
        }
        assert_eq!(ops.as_slice().len(), STROKES * 4);
    }

    /// The recent text is bounded, dropping the oldest.
    #[test]
    fn recent_is_bounded() {
        let mut out = Output::new();
        for _ in 0..40 {
            stroke(&mut out, c(TEN));
        }
        assert_eq!(out.recent().len(), RECENT);
        assert!(out.recent().ends_with(" ten"));
    }
}
