//! The chord tables, transcribed from `docs/orsy/orsy-mapping.json`.
//!
//! That file is the frozen mapping and this is a copy of it, checked against
//! it by `check_rust.py` in `midi4text-analysis`.  Each pattern is named by
//! the Michela key pattern it came from (`FC`, `RI`, `uia`), which is what the
//! composition rules in [`compose`](crate::compose) are written in terms of,
//! so that they read the same as the reference `theory.py`.  What the pattern
//! *spells* is a property of the variant.
//!
//! Each table has an `Empty` variant for the Series being absent, which is
//! distinct from a key combination the table has no entry for: the first is
//! an ordinary part of a stroke, the second makes the stroke untranslatable.
//! The commands sit on such combinations, so they never reach the rules.

/// A shape on the outer five keys.
///
/// The same shape spells an onset on the left hand and a coda on the right,
/// usually the same consonant.  Three shapes are coda-only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outer {
    /// No outer key.
    Empty,
    /// `n`: onset `n`, coda `n`.
    N,
    /// `s`: onset `s`, coda `s`.
    S,
    /// `t`: onset `t`, coda `t`.
    FP,
    /// `a`: onset `r`, coda `r`.
    FCN,
    /// `o`: onset `c`, coda `c`.
    CP,
    /// `a+t`: onset `d`, coda `d`.
    SCP,
    /// `o+s`: onset `p`, coda `p`.
    P,
    /// `o+t`: onset `f`, coda `f`.
    F,
    /// `a+n`: onset `y`, coda `y`.
    ZN,
    /// `o+n`: onset `th`, coda `th`.
    FZ,
    /// `s+t`: onset `l`, coda `l`.
    SCN,
    /// `a+o+s`: onset `b`, coda `b`.
    FCP,
    /// `s+n`: onset `w`, coda `w`.
    CN,
    /// `a+o`: onset `g`, coda `g`.
    ZP,
    /// `t+n`: onset `h`, coda `st`.
    FC,
    /// `a+o+t`: onset `v`, coda `v`.
    SC,
    /// `a+o+n`: onset `m`, coda `m`.
    SZP,
    /// `a+s+t`: onset `ind`, coda `nd`.
    FN,
    /// `a+s+n`: onset `inc`, coda `ng`.
    SN,
    /// `a+t+n`: onset `k`, coda `k`.
    SZ,
    /// `o+s+t`: onset `ch`, coda `ch`.
    SP,
    /// `o+s+n`: onset `int`, coda `nt`.
    FZN,
    /// `o+t+n`: onset `x`, coda `x`.
    SZN,
    /// `a+o+s+t`: onset `sh`, coda `sh`.
    C,
    /// `s+t+n`: onset `gh`, coda `gh`.
    FZP,
    /// `a+s`: onset `z`, coda `z`.
    Z,
    /// `a+o+s+n`: onset `ck`, coda `ck`.
    CZ,
    /// `a+o+t+n`: coda-only; as a coda it spells nothing and capitalises
    /// the syllable (Michela's `zcs`).
    SCZ,
    /// `a+s+t+n`: coda-only, `h`.
    FCZ,
    /// `o+s+t+n`: coda-only, `e`.
    CZP,
}

impl Outer {
    /// Every shape, in the order of the mapping file.
    pub const ALL: [Outer; 30] = [
        Outer::N, Outer::S, Outer::FP, Outer::FCN, Outer::CP, Outer::SCP, Outer::P, Outer::F, Outer::ZN, Outer::FZ, Outer::SCN, Outer::FCP, Outer::CN, Outer::ZP, Outer::FC, Outer::SC, Outer::SZP, Outer::FN, Outer::SN, Outer::SZ, Outer::SP, Outer::FZN, Outer::SZN, Outer::C, Outer::FZP, Outer::Z, Outer::CZ, Outer::SCZ, Outer::FCZ, Outer::CZP,
    ];

    /// The keys that strike this shape.
    pub const fn bits(self) -> u16 {
        match self {
            Outer::Empty => 0,
            Outer::N => 0x040,
            Outer::S => 0x020,
            Outer::FP => 0x004,
            Outer::FCN => 0x001,
            Outer::CP => 0x002,
            Outer::SCP => 0x005,
            Outer::P => 0x022,
            Outer::F => 0x006,
            Outer::ZN => 0x041,
            Outer::FZ => 0x042,
            Outer::SCN => 0x024,
            Outer::FCP => 0x023,
            Outer::CN => 0x060,
            Outer::ZP => 0x003,
            Outer::FC => 0x044,
            Outer::SC => 0x007,
            Outer::SZP => 0x043,
            Outer::FN => 0x025,
            Outer::SN => 0x061,
            Outer::SZ => 0x045,
            Outer::SP => 0x026,
            Outer::FZN => 0x062,
            Outer::SZN => 0x046,
            Outer::C => 0x027,
            Outer::FZP => 0x064,
            Outer::Z => 0x021,
            Outer::CZ => 0x063,
            Outer::SCZ => 0x047,
            Outer::FCZ => 0x065,
            Outer::CZP => 0x066,
        }
    }

    /// The Michela key pattern, for talking to the Python model.
    pub const fn michela(self) -> &'static str {
        match self {
            Outer::Empty => "",
            Outer::N => "N",
            Outer::S => "S",
            Outer::FP => "FP",
            Outer::FCN => "FCN",
            Outer::CP => "CP",
            Outer::SCP => "SCP",
            Outer::P => "P",
            Outer::F => "F",
            Outer::ZN => "ZN",
            Outer::FZ => "FZ",
            Outer::SCN => "SCN",
            Outer::FCP => "FCP",
            Outer::CN => "CN",
            Outer::ZP => "ZP",
            Outer::FC => "FC",
            Outer::SC => "SC",
            Outer::SZP => "SZP",
            Outer::FN => "FN",
            Outer::SN => "SN",
            Outer::SZ => "SZ",
            Outer::SP => "SP",
            Outer::FZN => "FZN",
            Outer::SZN => "SZN",
            Outer::C => "C",
            Outer::FZP => "FZP",
            Outer::Z => "Z",
            Outer::CZ => "CZ",
            Outer::SCZ => "SCZ",
            Outer::FCZ => "FCZ",
            Outer::CZP => "CZP",
        }
    }

    /// What the shape spells as Series 1, the syllable onset.  `None` for
    /// the coda-only shapes, which make a stroke untranslatable.
    pub const fn onset(self) -> Option<&'static str> {
        match self {
            Outer::Empty => Some(""),
            Outer::N => Some("n"),
            Outer::S => Some("s"),
            Outer::FP => Some("t"),
            Outer::FCN => Some("r"),
            Outer::CP => Some("c"),
            Outer::SCP => Some("d"),
            Outer::P => Some("p"),
            Outer::F => Some("f"),
            Outer::ZN => Some("y"),
            Outer::FZ => Some("th"),
            Outer::SCN => Some("l"),
            Outer::FCP => Some("b"),
            Outer::CN => Some("w"),
            Outer::ZP => Some("g"),
            Outer::FC => Some("h"),
            Outer::SC => Some("v"),
            Outer::SZP => Some("m"),
            Outer::FN => Some("ind"),
            Outer::SN => Some("inc"),
            Outer::SZ => Some("k"),
            Outer::SP => Some("ch"),
            Outer::FZN => Some("int"),
            Outer::SZN => Some("x"),
            Outer::C => Some("sh"),
            Outer::FZP => Some("gh"),
            Outer::Z => Some("z"),
            Outer::CZ => Some("ck"),
            Outer::SCZ => None,
            Outer::FCZ => None,
            Outer::CZP => None,
        }
    }

    /// What the shape spells as Series 4, the syllable coda.
    pub const fn coda(self) -> &'static str {
        match self {
            Outer::Empty => "",
            Outer::N => "n",
            Outer::S => "s",
            Outer::FP => "t",
            Outer::FCN => "r",
            Outer::CP => "c",
            Outer::SCP => "d",
            Outer::P => "p",
            Outer::F => "f",
            Outer::ZN => "y",
            Outer::FZ => "th",
            Outer::SCN => "l",
            Outer::FCP => "b",
            Outer::CN => "w",
            Outer::ZP => "g",
            Outer::FC => "st",
            Outer::SC => "v",
            Outer::SZP => "m",
            Outer::FN => "nd",
            Outer::SN => "ng",
            Outer::SZ => "k",
            Outer::SP => "ch",
            Outer::FZN => "nt",
            Outer::SZN => "x",
            Outer::C => "sh",
            Outer::FZP => "gh",
            Outer::Z => "z",
            Outer::CZ => "ck",
            Outer::SCZ => "",
            Outer::FCZ => "h",
            Outer::CZP => "e",
        }
    }

    /// The shape struck by these keys, if the table has one.
    pub fn lookup(bits: u16) -> Option<Outer> {
        if bits == 0 {
            return Some(Outer::Empty);
        }
        Outer::ALL.iter().copied().find(|o| o.bits() == bits)
    }
}

/// A Series 2 pattern: the second character, on the left hand's inner four.
///
/// Mostly consonants.  Some also read as a "mirrored" vowel when the stroke
/// has no Series 3 vowel; see [`Second::mirrored_vowel`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Second {
    /// No inner key.
    Empty,
    /// `Sp`: `r`, or the vowel `a`.
    R,
    /// `Bk`: `l`.
    RI,
    /// `i`: `i`, or the vowel `i`.
    I,
    /// `i+Bk`: `t`.
    RIU,
    /// `e+Bk`: `m`.
    RU,
    /// `e+i`: `o`.
    RXI,
    /// `e`: `s`, or the vowel `e`.
    X,
    /// `e+Sp`: `e`, or the vowel `ea`.
    RX,
    /// `e+i+Bk`: `w`, or the vowel `o`.
    XI,
    /// `e+i+Sp`: `c`.
    XIU,
    /// `i+Sp`: `u`, or the vowel `u`.
    U,
    /// `Sp+Bk`: `p`.
    IU,
    /// `e+Sp+Bk`: `n`.
    XU,
}

impl Second {
    /// Every pattern, in the order of the mapping file.
    pub const ALL: [Second; 13] = [
        Second::R, Second::RI, Second::I, Second::RIU, Second::RU, Second::RXI, Second::X, Second::RX, Second::XI, Second::XIU, Second::U, Second::IU, Second::XU,
    ];

    /// The keys that strike this pattern.
    pub const fn bits(self) -> u16 {
        match self {
            Second::Empty => 0,
            Second::R => 0x100,
            Second::RI => 0x200,
            Second::I => 0x080,
            Second::RIU => 0x280,
            Second::RU => 0x208,
            Second::RXI => 0x088,
            Second::X => 0x008,
            Second::RX => 0x108,
            Second::XI => 0x288,
            Second::XIU => 0x188,
            Second::U => 0x180,
            Second::IU => 0x300,
            Second::XU => 0x308,
        }
    }

    /// The Michela key pattern, for talking to the Python model.
    pub const fn michela(self) -> &'static str {
        match self {
            Second::Empty => "",
            Second::R => "R",
            Second::RI => "RI",
            Second::I => "I",
            Second::RIU => "RIU",
            Second::RU => "RU",
            Second::RXI => "RXI",
            Second::X => "X",
            Second::RX => "RX",
            Second::XI => "XI",
            Second::XIU => "XIU",
            Second::U => "U",
            Second::IU => "IU",
            Second::XU => "XU",
        }
    }

    /// What the pattern spells, read as a consonant.
    pub const fn spells(self) -> &'static str {
        match self {
            Second::Empty => "",
            Second::R => "r",
            Second::RI => "l",
            Second::I => "i",
            Second::RIU => "t",
            Second::RU => "m",
            Second::RXI => "o",
            Second::X => "s",
            Second::RX => "e",
            Second::XI => "w",
            Second::XIU => "c",
            Second::U => "u",
            Second::IU => "p",
            Second::XU => "n",
        }
    }

    /// The vowel the pattern reads as when Series 3 is empty and a real
    /// coda is present.  Using it appends a silent `e` and closes the word.
    pub const fn mirrored_vowel(self) -> Option<&'static str> {
        match self {
            Second::R => Some("a"),
            Second::I => Some("i"),
            Second::X => Some("e"),
            Second::RX => Some("ea"),
            Second::XI => Some("o"),
            Second::U => Some("u"),
            _ => None,
        }
    }

    /// The pattern struck by these keys, if the table has one.
    pub fn lookup(bits: u16) -> Option<Second> {
        if bits == 0 {
            return Some(Second::Empty);
        }
        Second::ALL.iter().copied().find(|s| s.bits() == bits)
    }
}

/// A Series 3 pattern: the vowel, on the right hand's inner four.
///
/// Every vowel has a plain form and an ending form, the ending form being the
/// plain one plus `Bk`.  The ending form closes the word, which is how the
/// space between words is folded into the last stroke of a word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vowel {
    /// No inner key.
    Empty,
    /// `Sp`: `a`.
    A,
    /// `e`: `e`.
    E,
    /// `i`: `i`.
    I,
    /// `e+i`: `o`.
    Ie,
    /// `i+Sp`: `u`.
    U,
    /// `e+Sp`: `ea`.
    Ea,
    /// `Sp+Bk`: `a`, closing the word.
    Ua,
    /// `e+Bk`: `e`, closing the word.
    Ue,
    /// `i+Bk`: `i`, closing the word.
    Ui,
    /// `e+i+Bk`: `o`, closing the word.
    Uie,
    /// `i+Sp+Bk`: `u`, closing the word.
    Uia,
    /// `e+Sp+Bk`: `ea`, closing the word.
    Iea,
    /// `e+i+Sp+Bk`: `ou`, closing the word.
    Ia,
}

impl Vowel {
    /// Every pattern, in the order of the mapping file.
    pub const ALL: [Vowel; 13] = [
        Vowel::A, Vowel::E, Vowel::I, Vowel::Ie, Vowel::U, Vowel::Ea, Vowel::Ua, Vowel::Ue, Vowel::Ui, Vowel::Uie, Vowel::Uia, Vowel::Iea, Vowel::Ia,
    ];

    /// The keys that strike this pattern.
    pub const fn bits(self) -> u16 {
        match self {
            Vowel::Empty => 0,
            Vowel::A => 0x100,
            Vowel::E => 0x008,
            Vowel::I => 0x080,
            Vowel::Ie => 0x088,
            Vowel::U => 0x180,
            Vowel::Ea => 0x108,
            Vowel::Ua => 0x300,
            Vowel::Ue => 0x208,
            Vowel::Ui => 0x280,
            Vowel::Uie => 0x288,
            Vowel::Uia => 0x380,
            Vowel::Iea => 0x308,
            Vowel::Ia => 0x388,
        }
    }

    /// The Michela key pattern, for talking to the Python model.
    pub const fn michela(self) -> &'static str {
        match self {
            Vowel::Empty => "",
            Vowel::A => "a",
            Vowel::E => "e",
            Vowel::I => "i",
            Vowel::Ie => "ie",
            Vowel::U => "u",
            Vowel::Ea => "ea",
            Vowel::Ua => "ua",
            Vowel::Ue => "ue",
            Vowel::Ui => "ui",
            Vowel::Uie => "uie",
            Vowel::Uia => "uia",
            Vowel::Iea => "iea",
            Vowel::Ia => "ia",
        }
    }

    /// The vowel the pattern spells.
    pub const fn text(self) -> &'static str {
        match self {
            Vowel::Empty => "",
            Vowel::A => "a",
            Vowel::E => "e",
            Vowel::I => "i",
            Vowel::Ie => "o",
            Vowel::U => "u",
            Vowel::Ea => "ea",
            Vowel::Ua => "a",
            Vowel::Ue => "e",
            Vowel::Ui => "i",
            Vowel::Uie => "o",
            Vowel::Uia => "u",
            Vowel::Iea => "ea",
            Vowel::Ia => "ou",
        }
    }

    /// Whether this is an ending form, which closes the word.
    pub const fn ends_word(self) -> bool {
        matches!(self, Vowel::Ua | Vowel::Ue | Vowel::Ui | Vowel::Uie | Vowel::Uia | Vowel::Iea | Vowel::Ia)
    }

    /// The pattern struck by these keys, if the table has one.
    pub fn lookup(bits: u16) -> Option<Vowel> {
        if bits == 0 {
            return Some(Vowel::Empty);
        }
        Vowel::ALL.iter().copied().find(|v| v.bits() == bits)
    }
}

/// The four patterns of a chord, one per Series.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Patterns {
    pub onset: Outer,
    pub second: Second,
    pub vowel: Vowel,
    pub coda: Outer,
}

impl Patterns {
    /// Split a chord into its patterns, or `None` if any Series holds a
    /// combination its table has no entry for, or the chord has a key the
    /// layout does not.
    pub fn of(chord: crate::chord::Chord) -> Option<Patterns> {
        if (chord.left | chord.right) & !crate::chord::HAND_MASK != 0 {
            return None;
        }
        Some(Patterns {
            onset: Outer::lookup(chord.series1())?,
            second: Second::lookup(chord.series2())?,
            vowel: Vowel::lookup(chord.series3())?,
            coda: Outer::lookup(chord.series4())?,
        })
    }
}

/// The commands: whole strokes that no syllable can produce, because each
/// sits on a key combination unassigned within its own group.
pub mod commands {
    /// Left hand, `i+Sp+Bk`, held while the right hand plays a Dosh chord.
    pub const DOSH_ONESHOT: u16 = 0x380;
    /// Either hand, `e+i+Sp+Bk`: switch to Dosh and back.  Unmapped in Dosh
    /// too, so the one shape does both.
    pub const DOSH_TOGGLE: u16 = 0x388;
    /// Right hand, `e+i+Sp`: capitalise the next word.
    pub const CAP_NEXT: u16 = 0x188;
    /// Right hand, `Bk`: a space on its own.
    pub const SPACE: u16 = 0x200;
    /// Left hand, all five outer keys: undo the last stroke.
    pub const UNDO: u16 = 0x067;
}

/// The punctuation that takes part in spacing.
///
/// Everything else reaches the keyboard through the Dosh escape, which is a whole
/// keyboard's worth of symbols for nothing new to learn.  These few cannot: a mark that
/// attaches to the word before it, owes a space to the word after it, or capitalises what
/// follows is doing the output stage's job, and the escape types outside the output stage
/// -- so an escaped full stop could not be undone with the word it ended, did not appear
/// in the text retro-capitalisation walks back over, and left the next word to run on when
/// the word before it had not been closed.
///
/// The chords extend the space command rather than carving anything out of the alphabets:
/// `Bk` alone is a space, and `Bk` plus one outer key is a mark.  `Bk` is not a vowel form
/// on its own, so no syllable can produce any of them, and the whole family is two keys on
/// the hand that already ends words.
pub mod punctuation {
    /// One mark: what it types, and how it joins to what is around it.
    ///
    /// Every mark attaches to whatever came before, closed or not, since that is what
    /// makes it punctuation rather than a word.  What they differ in is the far side: most
    /// owe a space to the next word, while the apostrophe and the hyphen bind straight on
    /// to it, so `don` `'` `t` comes out as one word.
    #[derive(PartialEq, Eq, Debug)]
    pub struct Mark {
        /// The right-hand keys.  The left hand is empty.
        pub bits: u16,
        pub text: &'static str,
        /// Whether the next word gets a space.  False for the marks that bind forward.
        pub space_after: bool,
        /// Whether the next letter is capitalised, which the sentence-enders do.
        pub capitalises: bool,
    }

    /// Every mark, in the order the lessons teach them.
    ///
    /// One outer key each for the five commonest, which is all the one-key shapes there
    /// are; the rest take two.  Ranked by how often English prose wants them rather than
    /// by any tidiness of the shapes: the apostrophe and the hyphen are wanted far more
    /// than the exclamation mark and the colon, and neither of them can be had from the
    /// escape without breaking the word it belongs to.
    pub static ALL: &[Mark] = &[
        Mark { bits: 0x204, text: ".", space_after: true, capitalises: true },   // Bk+t
        Mark { bits: 0x240, text: ",", space_after: true, capitalises: false },  // Bk+n
        Mark { bits: 0x220, text: "'", space_after: false, capitalises: false }, // Bk+s
        Mark { bits: 0x202, text: "?", space_after: true, capitalises: true },   // Bk+o
        Mark { bits: 0x201, text: "-", space_after: false, capitalises: false }, // Bk+a
        Mark { bits: 0x206, text: "!", space_after: true, capitalises: true },   // Bk+o+t
        Mark { bits: 0x260, text: ":", space_after: true, capitalises: false },  // Bk+s+n
        Mark { bits: 0x244, text: ";", space_after: true, capitalises: false },  // Bk+t+n
    ];

    /// The mark a right hand strikes, if it is one.  The caller checks the left hand is
    /// empty; a mark is a right-handed stroke on its own.
    pub fn lookup(right: u16) -> Option<&'static Mark> {
        ALL.iter().find(|m| m.bits == right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chord must mean one thing within its group.
    #[test]
    fn no_duplicate_chords() {
        for i in 0..Outer::ALL.len() {
            for j in 0..i {
                assert_ne!(Outer::ALL[i].bits(), Outer::ALL[j].bits(), "{:?}", Outer::ALL[i]);
            }
            assert_ne!(Outer::ALL[i].bits(), 0);
        }
        for i in 0..Second::ALL.len() {
            for j in 0..i {
                assert_ne!(Second::ALL[i].bits(), Second::ALL[j].bits(), "{:?}", Second::ALL[i]);
            }
            assert_ne!(Second::ALL[i].bits(), 0);
        }
        for i in 0..Vowel::ALL.len() {
            for j in 0..i {
                assert_ne!(Vowel::ALL[i].bits(), Vowel::ALL[j].bits(), "{:?}", Vowel::ALL[i]);
            }
            assert_ne!(Vowel::ALL[i].bits(), 0);
        }
    }

    /// Every shape stays within its group's keys.
    #[test]
    fn shapes_within_masks() {
        use crate::chord::{INNER_MASK, OUTER_MASK};
        for o in Outer::ALL {
            assert_eq!(o.bits() & !OUTER_MASK, 0, "{o:?}");
        }
        for s in Second::ALL {
            assert_eq!(s.bits() & !INNER_MASK, 0, "{s:?}");
        }
        for v in Vowel::ALL {
            assert_eq!(v.bits() & !INNER_MASK, 0, "{v:?}");
        }
    }

    /// No punctuation mark is a syllable, a command, or another mark, and every one of
    /// them is the word-end marker plus outer keys.
    #[test]
    fn punctuation_is_free() {
        use crate::chord::{Chord, INNER_MASK, OUTER_MASK};
        for (i, mark) in punctuation::ALL.iter().enumerate() {
            assert_eq!(mark.bits & INNER_MASK, commands::SPACE, "{mark:?}");
            assert_ne!(mark.bits & OUTER_MASK, 0, "{mark:?}");
            assert_ne!(mark.bits, commands::SPACE);
            assert_ne!(mark.bits, commands::CAP_NEXT);
            // Not a syllable, whatever the left hand is doing.
            assert_eq!(crate::compose::translate(Chord::new(0, mark.bits)), None, "{mark:?}");
            for other in &punctuation::ALL[..i] {
                assert_ne!(mark.bits, other.bits, "{mark:?}");
            }
            assert_eq!(punctuation::lookup(mark.bits), Some(mark));
        }
        assert_eq!(punctuation::lookup(commands::SPACE), None);
    }

    /// The commands are free within their own group, so no syllable can
    /// produce them.  `DOSH_TOGGLE` must be free in Series 2 (it is only ever
    /// looked for on the left) and `SPACE` and `CAP_NEXT` in Series 3.
    #[test]
    fn commands_are_free() {
        assert_eq!(Second::lookup(commands::DOSH_ONESHOT), None);
        assert_eq!(Second::lookup(commands::DOSH_TOGGLE), None);
        assert_eq!(Vowel::lookup(commands::CAP_NEXT), None);
        assert_eq!(Vowel::lookup(commands::SPACE), None);
        assert_eq!(Outer::lookup(commands::UNDO), None);
    }

    /// The ending form of a vowel is its plain form plus `Bk`.
    #[test]
    fn ending_forms_add_bk() {
        for v in Vowel::ALL {
            if v.ends_word() {
                assert_ne!(v.bits() & 0x200, 0, "{v:?}");
                let plain = Vowel::lookup(v.bits() & !0x200);
                // `ia` (ou) has no plain form; that chord is the cap command.
                if v == Vowel::Ia {
                    assert_eq!(plain, None);
                } else {
                    let plain = plain.expect("plain form");
                    assert_eq!(plain.text(), v.text(), "{v:?}");
                    assert!(!plain.ends_word());
                }
            }
        }
    }
}

