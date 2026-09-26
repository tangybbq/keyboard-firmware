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
//!   next stroke, and [`Prefix::AllCaps`] every letter of the next word;
//!   [`Prefix::Uncap`] cancels either.  [`Output::cap_previous`] walks back
//!   over the recent text and capitalises words already typed, and
//!   [`Output::uncap_previous`] takes the capitals off again.
//! - **Joining.**  [`Prefix::Join`] drops the space owed before the next
//!   word, so that it runs on from the last.
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

/// A command that types nothing, but changes how the next stroke is typed.
///
/// Given to [`Output::prefix`], which records it as a stroke of its own, so
/// that undo takes back the command and not the word before it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prefix {
    /// Drop the space owed before the next word, so that it joins the last.
    Join,
    /// Capitalise the next letter.
    CapNext,
    /// Capitalise every letter of the next word.
    AllCaps,
    /// Capitalise nothing of the next word, cancelling a capital already
    /// asked for: after a full stop that does not end a sentence, or before
    /// something that must stay lower case.
    Uncap,
}

/// What one stroke did, so that undo can take it back.
#[derive(Clone, Copy, Default)]
struct Stroke {
    /// Characters typed, the space before the word included.
    chars: u8,
    /// Whether a space was pending before the stroke, to put back.
    pending_space: bool,
    /// Whether a capital was pending before the stroke, to put back.
    pending_cap: bool,
    /// Whether the word was being typed in capitals before the stroke, to
    /// put back.
    all_caps: bool,
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
    /// Every letter is capitalised until the word closes.
    all_caps: bool,
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
            strokes: [Stroke { chars: 0, pending_space: false, pending_cap: false, all_caps: false };
                STROKES],
            strokes_len: 0,
            pending_space: false,
            pending_cap: false,
            all_caps: false,
        }
    }

    /// Type a stroke.
    pub fn stroke(&mut self, t: &Translation, ops: &mut Ops) {
        let record = self.before(0);
        let mut count = 0u8;
        if self.pending_space && t.space_before && !t.is_empty() {
            self.emit(' ', ops);
            count += 1;
        }
        for ch in t.chars() {
            let ch = if (self.pending_cap || self.all_caps) && ch.is_ascii_alphabetic() {
                self.pending_cap = false;
                ch.to_ascii_uppercase()
            } else {
                ch
            };
            self.emit(ch, ops);
            count = count.saturating_add(1);
        }
        self.pending_space = t.space_after;
        // A stroke that closes the word ends the capitals.
        self.all_caps &= !t.space_after;
        self.record(Stroke { chars: count, ..record });
    }

    /// Type a punctuation mark.
    ///
    /// It attaches to whatever came before, with no space, whether or not that word was
    /// closed.  Most owe a space to the next word; the apostrophe and the hyphen do not,
    /// so what follows binds straight on.  Recorded like any other stroke, so undo takes
    /// it back with the characters it typed.
    pub fn mark(&mut self, mark: &punctuation::Mark, ops: &mut Ops) {
        let record = self.before(0);
        let mut count = 0u8;
        for ch in mark.text.chars() {
            self.emit(ch, ops);
            count = count.saturating_add(1);
        }
        self.pending_space = mark.space_after;
        // A mark that owes a space ends the word, and so its capitals; the
        // apostrophe and the hyphen carry them on to what follows.
        self.all_caps &= !mark.space_after;
        // Never cleared here: a mark that does not capitalise should not cancel a capital
        // the writer has already asked for.
        self.pending_cap |= mark.capitalises;
        self.record(Stroke { chars: count, ..record });
    }

    /// The space command: a space on its own.
    pub fn space(&mut self, ops: &mut Ops) {
        let record = self.before(1);
        self.emit(' ', ops);
        self.pending_space = false;
        self.all_caps = false;
        self.record(record);
    }

    /// A command that changes how the next stroke is typed.  Recorded as a
    /// stroke that typed nothing, so that undo takes back only the command.
    pub fn prefix(&mut self, prefix: Prefix) {
        let record = self.before(0);
        match prefix {
            Prefix::Join => self.pending_space = false,
            Prefix::CapNext => self.pending_cap = true,
            Prefix::AllCaps => self.all_caps = true,
            Prefix::Uncap => {
                self.pending_cap = false;
                self.all_caps = false;
            }
        }
        self.record(record);
    }

    /// Capitalise the next letter typed.
    ///
    /// Not a stroke of its own, unlike [`Prefix::CapNext`]: for the layout
    /// manager, which asks for it after a sentence-ender played through the
    /// Dosh escape.
    pub fn cap_next(&mut self) {
        self.pending_cap = true;
    }

    /// Capitalise the previous `n` words of the recent text.
    ///
    /// Backspaces to the start of the `n`th word back (or as far as the
    /// recent text reaches) and retypes it with each word's first letter in
    /// capitals.  Not a stroke of its own: the text is the same length, and
    /// undoing back over a retype would only lose the capitals it added.
    pub fn cap_previous(&mut self, n: usize, ops: &mut Ops) {
        self.recase_previous(n, true, ops);
    }

    /// Take the capital off the first letter of each of the previous `n`
    /// words, as [`cap_previous`](Self::cap_previous) puts it on.  The rest
    /// of each word is left as it is.
    pub fn uncap_previous(&mut self, n: usize, ops: &mut Ops) {
        self.recase_previous(n, false, ops);
    }

    fn recase_previous(&mut self, n: usize, upper: bool, ops: &mut Ops) {
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
                if upper {
                    ch.to_ascii_uppercase()
                } else {
                    ch.to_ascii_lowercase()
                }
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

    /// Take back the last stroke, and put the spacing and the pending
    /// capital back as they were before it: a word retyped after undoing it
    /// gets the capital a sentence-ender or cap-next gave it the first time.
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
        self.pending_cap = stroke.pending_cap;
        self.all_caps = stroke.all_caps;
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

    /// The record of a stroke about to type `chars` characters, holding the
    /// state it starts from.
    fn before(&self, chars: u8) -> Stroke {
        Stroke {
            chars,
            pending_space: self.pending_space,
            pending_cap: self.pending_cap,
            all_caps: self.all_caps,
        }
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

    /// Join drops the space the next word is owed, and is undone on its own.
    #[test]
    fn join() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        out.prefix(Prefix::Join);
        assert_eq!(stroke(&mut out, c(TEN)), "ten");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        assert_eq!(out.recent(), "tenten ten");
        // Undo takes the word back, then the join, and the space is owed again.
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        out.prefix(Prefix::Join);
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(ops.as_slice().len(), 0);
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
    }

    /// The cap-next command is a stroke: undo takes it back, not the word
    /// before it.
    #[test]
    fn cap_next_command() {
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        out.prefix(Prefix::CapNext);
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        stroke(&mut out, c(TEN));
        out.prefix(Prefix::CapNext);
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(ops.as_slice().len(), 0);
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        assert_eq!(out.recent(), "ten Ten ten ten");
    }

    /// All caps lasts until the word closes, across the strokes of the word
    /// and through an apostrophe, but not past a space.
    #[test]
    fn all_caps() {
        fn find(text: &str) -> &'static punctuation::Mark {
            punctuation::ALL.iter().find(|m| m.text == text).expect("a mark")
        }
        let mut out = Output::new();
        out.prefix(Prefix::AllCaps);
        assert_eq!(stroke(&mut out, c(TE_)), "TEN");
        assert_eq!(stroke(&mut out, c(TEN)), "TEN");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        out.prefix(Prefix::AllCaps);
        stroke(&mut out, c(TE_));
        let mut ops = Ops::new();
        out.mark(find("'"), &mut ops);
        assert_eq!(stroke(&mut out, c(S_CODA)), "S");
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        assert_eq!(out.recent(), "TENTEN ten TEN'S ten");
        // Undoing into the word puts the capitals back for the retype.
        let mut out = Output::new();
        out.prefix(Prefix::AllCaps);
        stroke(&mut out, c(TE_));
        stroke(&mut out, c(TEN));
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), "TEN");
    }

    /// Uncap cancels a capital asked for, by a command or a full stop, and
    /// all caps; it is undone on its own.
    #[test]
    fn uncap() {
        fn find(text: &str) -> &'static punctuation::Mark {
            punctuation::ALL.iter().find(|m| m.text == text).expect("a mark")
        }
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        let mut ops = Ops::new();
        out.mark(find("."), &mut ops);
        out.prefix(Prefix::Uncap);
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        out.prefix(Prefix::AllCaps);
        out.prefix(Prefix::Uncap);
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        // Undoing the uncap puts the capital back.
        out.prefix(Prefix::CapNext);
        out.prefix(Prefix::Uncap);
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        assert_eq!(out.recent(), "ten. ten ten Ten");
    }

    /// Uncap previous takes the first capital off the words before, and
    /// leaves the rest of each word alone.
    #[test]
    fn uncap_previous() {
        let mut out = Output::new();
        out.prefix(Prefix::AllCaps);
        stroke(&mut out, c(TEN));
        stroke(&mut out, c(TEN));
        out.cap_previous(1, &mut Ops::new());
        assert_eq!(out.recent(), "TEN Ten");
        let mut ops = Ops::new();
        out.uncap_previous(2, &mut ops);
        assert_eq!(ops.text(), "\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}\u{8}tEN ten");
        assert_eq!(out.recent(), "tEN ten");
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

    /// Undo puts back a capital the undone stroke used, so the retype gets
    /// it too, and takes away one it asked for.
    #[test]
    fn undo_restores_capital() {
        fn find(text: &str) -> &'static punctuation::Mark {
            punctuation::ALL.iter().find(|m| m.text == text).expect("a mark")
        }
        let mut out = Output::new();
        stroke(&mut out, c(TEN));
        let mut ops = Ops::new();
        out.mark(find("."), &mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        let mut ops = Ops::new();
        out.undo(&mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), " Ten");
        // Undoing the full stop takes its capital with it.
        let mut ops = Ops::new();
        out.undo(&mut ops);
        out.undo(&mut ops);
        assert_eq!(stroke(&mut out, c(TEN)), " ten");
        assert_eq!(out.recent(), "ten ten");
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
