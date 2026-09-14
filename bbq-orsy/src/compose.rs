//! The composition rules: from the four Series of a chord to the text it
//! spells, and how that text joins onto its neighbours.
//!
//! This is a port of `translate` in `midi4text-analysis/m4t/theory.py`, and
//! must agree with it exactly; `check_rust.py` there checks that it does over
//! every chord.  The rules are Midi4Text's, recovered from its dictionary,
//! and are described in `docs/midi4text/02-theory.md`.  In outline:
//!
//! 1. Each Series is looked up in its table.  A combination the table has no
//!    entry for makes the whole stroke untranslatable.
//! 2. The nucleus is chosen.  Normally it is the Series 3 vowel.  The three
//!    "free" Series 3 combinations spell a digraph together with Series 2, or
//!    a placeholder glyph alone.  A Series 2 vowel can fuse with the Series 3
//!    vowel into a diphthong.  With no Series 3 at all, Series 2 may supply a
//!    "mirrored" vowel instead of a consonant, which appends a silent `e` and
//!    closes the word.
//! 3. The onset is spelled: a handful of onset clusters are spelled by the
//!    onset and Series 2 together (`str`, `spl`, `qu`), and Series 2's `XI`
//!    is `h` rather than `w` after `p`, `w` and `r`.
//! 4. A bare final `y` takes its space from the `ui` keys, which then spell
//!    nothing themselves.
//! 5. The spacing follows from the vowel: an ending form closes the word,
//!    a plain vowel binds forward to the next stroke, and no vowel at all
//!    makes a fragment that binds backward, unless it is a bare onset, which
//!    binds forward.

use crate::chord::Chord;
use crate::tables::{Outer, Patterns, Second, Vowel};

/// The version of the composition rules, for a fingerprint of the layout.
///
/// Bump it when a change here alters what some stroke spells or how it
/// binds; a host that has learned strokes under the old rules has to know.
/// Table changes are fingerprinted on their own and need no bump.
pub const RULES_VERSION: u32 = 1;

/// The composition rules a stroke used, as flags in [`Translation::rules`].
///
/// A rule is something to learn beyond the patterns themselves, so a
/// trainer wants to know which strokes exercise which.  The plain readings
/// -- an onset, a second character, a vowel, a coda, each spelling what its
/// table says -- are not rules.
pub mod rules {
    /// Series 2 supplied the nucleus: a mirrored vowel with its silent `e`,
    /// or `RX`/`RXI`.
    pub const MIRRORED: u8 = 0x01;
    /// The onset and Series 2 spelled a cluster together (`str`, `qu`, `j`).
    pub const CLUSTER: u8 = 0x02;
    /// Series 2 `XI` read as `h`, after `p`, `w` or `r`.
    pub const XI_H: u8 = 0x04;
    /// A Series 2 vowel fused with the Series 3 vowel (`au`, `ai`).
    pub const DIPHTHONG: u8 = 0x08;
    /// One of the free Series 3 combinations (`ea`, `ou`, or a glyph alone).
    pub const FREE: u8 = 0x10;
    /// A bare final `y` taking its space from the `ui` keys.
    pub const BARE_Y: u8 = 0x20;
    /// The capitalising coda.
    pub const CAPITALISE: u8 = 0x40;
}

/// What a stroke spells and how it joins onto its neighbours.
///
/// The text is kept as the pieces it was composed from rather than copied
/// into a buffer; [`Translation::chars`] yields it in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Translation {
    /// Onset, middle, nucleus, coda, and the silent `e` if there is one.
    parts: [&'static str; 5],
    /// Capitalise the first character.
    capitalize: bool,
    /// A space may come before this stroke.  False for a fragment that
    /// attaches to the preceding syllable.
    pub space_before: bool,
    /// A space may come after this stroke.  False when the stroke binds
    /// forward onto the next.
    pub space_after: bool,
    /// Which composition rules were used; see [`rules`].
    pub rules: u8,
}

impl Translation {
    /// The characters of the text, in order.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        let mut first = self.capitalize;
        self.parts.iter().flat_map(|p| p.chars()).map(move |ch| {
            if first {
                first = false;
                ch.to_ascii_uppercase()
            } else {
                ch
            }
        })
    }

    /// Whether the stroke spells nothing at all.
    pub fn is_empty(&self) -> bool {
        self.parts.iter().all(|p| p.is_empty())
    }

    /// Whether the syllable's first character is a consonant that came from
    /// Series 2 rather than Series 1.
    ///
    /// The manual's general rule is that the initial character is always
    /// written in Series 1, "which is intended to represent it"; a syllable
    /// beginning with a vowel goes in Series 3 instead.  The shipped
    /// dictionary carries the Series 2 spelling as well -- `RIuicf` and
    /// `SCNuicf` both give `list` -- so the rules here translate it, but
    /// [`Writer`](crate::Writer) will not choose it.  A vowel from Series 2
    /// is not this: the `au` and `ai` diphthongs are allowed to open a
    /// syllable there.
    pub fn inner_onset(&self) -> bool {
        self.parts[0].is_empty()
            && self.parts[1]
                .chars()
                .next()
                .is_some_and(|c| !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'))
    }

    /// The text as a string, for the host.
    #[cfg(feature = "std")]
    pub fn text(&self) -> String {
        self.chars().collect()
    }
}

/// What the nucleus of the syllable turned out to be.
struct Nucleus {
    text: &'static str,
    /// Append a silent `e` after the coda.
    silent_e: bool,
    /// The stroke closes the word.
    closes: bool,
    /// Series 2 was used up as the nucleus, and spells no consonant.
    consumed: bool,
    /// The rule that chose it, if one did.
    rule: u8,
}

impl Nucleus {
    const fn new(text: &'static str, silent_e: bool, closes: bool, consumed: bool) -> Nucleus {
        Nucleus { text, silent_e, closes, consumed, rule: 0 }
    }

    const fn by(self, rule: u8) -> Nucleus {
        Nucleus { rule, ..self }
    }
}

/// Choose the nucleus (`_nucleus` in the reference).
fn nucleus(s2: Second, s3: Vowel, s4: Outer) -> Nucleus {
    // The free combinations: alone, a placeholder glyph that closes the
    // word; with Series 2, the digraph Series 2 cannot reach on its own.
    match s3 {
        Vowel::Ea => {
            return if s2 == Second::Empty {
                Nucleus::new("\u{b0}", false, true, false)
            } else {
                Nucleus::new("ea", false, false, false)
            }
            .by(rules::FREE);
        }
        Vowel::Iea => {
            return if s2 == Second::Empty {
                Nucleus::new("_", false, true, false)
            } else {
                Nucleus::new("ea", false, true, false)
            }
            .by(rules::FREE);
        }
        Vowel::Ia => {
            return if s2 == Second::Empty {
                Nucleus::new("*", false, true, false)
            } else {
                Nucleus::new("ou", false, true, false)
            }
            .by(rules::FREE);
        }
        _ => (),
    }

    // A Series 2 vowel fused with the Series 3 vowel.
    match (s2, s3) {
        (Second::U, Vowel::U) | (Second::U, Vowel::Uia) => {
            return Nucleus::new("au", false, s3.ends_word(), true).by(rules::DIPHTHONG);
        }
        (Second::I, Vowel::I) | (Second::I, Vowel::Ui) => {
            return Nucleus::new("ai", false, s3.ends_word(), true).by(rules::DIPHTHONG);
        }
        _ => (),
    }

    if s3 != Vowel::Empty {
        return Nucleus::new(s3.text(), false, s3.ends_word(), false);
    }

    // No Series 3.  Series 2 may supply the nucleus instead.
    match s2 {
        Second::RX => return Nucleus::new("ea", true, true, true).by(rules::MIRRORED),
        Second::RXI => return Nucleus::new("o", false, false, true).by(rules::MIRRORED),
        _ => (),
    }
    if let Some(vowel) = s2.mirrored_vowel() {
        // Only with a real coda: `ck` and the capitalisation marker are
        // digraph tails or commands rather than consonants, and do not
        // license the mirrored reading.
        if s4 != Outer::Empty && !matches!(s4, Outer::CZ | Outer::SCZ) {
            return Nucleus::new(vowel, true, true, true).by(rules::MIRRORED);
        }
    }
    Nucleus::new("", false, false, false)
}

/// An onset that a Series 2 pattern rewrites wholesale.
fn onset_override(s1: Outer, s2: Second) -> Option<&'static str> {
    Some(match (s1, s2) {
        (Outer::FC, Second::R) => "str",
        (Outer::FC, Second::RI) => "spl",
        (Outer::FC, Second::IU) => "spr",
        (Outer::FC, Second::XIU) => "scr",
        (Outer::C, Second::XIU) => "sch",
        (Outer::Z, Second::XIU) => "sk",
        (Outer::S, Second::X) => "sci",
        (Outer::ZN, Second::I) => "j",
        (Outer::CP, Second::XIU) => "qu",
        _ => return None,
    })
}

/// Translate one stroke, or `None` if it is not a valid syllable.
pub fn translate(chord: Chord) -> Option<Translation> {
    // A key the layout does not have (the upper pinky, on a board that has
    // one) is not silently ignored, and a combination outside the tables
    // is not a syllable.
    let Patterns { onset: s1, second: s2, vowel: s3, coda: s4 } = Patterns::of(chord)?;
    // The coda-only shapes have no onset reading.
    let s1_onset = s1.onset()?;

    let n = nucleus(s2, s3, s4);
    let mut used = n.rule;

    // An onset cluster like FC+R = "str" needs Series 2 as a consonant; when
    // Series 2 has been taken for the nucleus instead, the cluster is off.
    let override_ = if n.consumed { None } else { onset_override(s1, s2) };
    let (onset, middle) = match override_ {
        Some(onset) => {
            used |= rules::CLUSTER;
            (onset, "")
        }
        None => {
            let onset = if s1 == Outer::FN && n.consumed { "gn" } else { s1_onset };
            let middle = if n.consumed {
                ""
            } else if s2 == Second::XI {
                if onset.ends_with('p') || onset.ends_with('w') || onset.ends_with('r') {
                    used |= rules::XI_H;
                    "h"
                } else {
                    "w"
                }
            } else {
                s2.spells()
            };
            (onset, middle)
        }
    };

    // A bare final Y takes its word-final space from the `ui` keys, which
    // then spell nothing themselves.
    let nucleus_text = if s3 == Vowel::Ui && s4 == Outer::ZN {
        used |= rules::BARE_Y;
        ""
    } else {
        n.text
    };

    let capitalize = s4 == Outer::SCZ;
    if capitalize {
        used |= rules::CAPITALISE;
    }
    let coda = if capitalize { "" } else { s4.coda() };
    let silent_e = if n.silent_e { "e" } else { "" };

    let (space_before, space_after) = if n.closes {
        (true, true)
    } else if s3 == Vowel::Empty {
        // No Series 3 vowel: a fragment.  A bare onset leans forward,
        // anything carrying Series 2 or a coda leans back onto the preceding
        // syllable.
        let leans_forward =
            (s1 != Outer::Empty && s2 == Second::Empty && s4 == Outer::Empty) || s4 == Outer::ZN;
        if leans_forward {
            (true, false)
        } else {
            (false, true)
        }
    } else {
        (true, false)
    };

    Some(Translation {
        parts: [onset, middle, nucleus_text, coda, silent_e],
        capitalize,
        space_before,
        space_after,
        rules: used,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a chord from patterns rather than bits, so the tests read as
    /// syllables.
    fn chord(s1: Outer, s2: Second, s3: Vowel, s4: Outer) -> Chord {
        Chord::new(s1.bits() | s2.bits(), s3.bits() | s4.bits())
    }

    fn check(c: Chord, text: &str, before: bool, after: bool) {
        let t = translate(c).unwrap_or_else(|| panic!("{c:?} untranslatable"));
        assert_eq!(t.text(), text, "{c:?}");
        assert_eq!((t.space_before, t.space_after), (before, after), "{c:?}");
    }

    /// The example from the mapping document: d + r + ea + m.
    #[test]
    fn dream() {
        check(chord(Outer::SCP, Second::R, Vowel::Ea, Outer::SZP), "dream", true, false);
        // The ending form closes the word.
        check(chord(Outer::SCP, Second::R, Vowel::Iea, Outer::SZP), "dream", true, true);
    }

    /// Plain and ending vowels, and the four kinds of spacing.
    #[test]
    fn spacing() {
        // Whole word.
        check(chord(Outer::FP, Second::Empty, Vowel::Ue, Outer::N), "ten", true, true);
        // Binds forward.
        check(chord(Outer::FP, Second::Empty, Vowel::E, Outer::N), "ten", true, false);
        // Coda-only fragment binds backward.
        check(chord(Outer::Empty, Second::Empty, Vowel::Empty, Outer::FZN), "nt", false, true);
        // A bare onset leans forward.
        check(chord(Outer::S, Second::Empty, Vowel::Empty, Outer::Empty), "s", true, false);
        // Onset with a second character leans back.
        check(chord(Outer::S, Second::RI, Vowel::Empty, Outer::Empty), "sl", false, true);
    }

    /// The onset clusters that Series 2 rewrites wholesale.
    #[test]
    fn onset_clusters() {
        check(chord(Outer::FC, Second::R, Vowel::Ua, Outer::N), "stran", true, true);
        check(chord(Outer::CP, Second::XIU, Vowel::I, Outer::FP), "quit", true, false);
        check(chord(Outer::ZN, Second::I, Vowel::Ua, Outer::SZP), "jam", true, true);
        // But not when Series 2 is the nucleus: FC + I with no vowel is
        // h + mirrored i.
        check(chord(Outer::FC, Second::I, Vowel::Empty, Outer::SCP), "hide", true, true);
    }

    /// Series 2's XI is h after p, w and r, and w otherwise.
    #[test]
    fn xi_is_h_or_w() {
        check(chord(Outer::P, Second::XI, Vowel::Ua, Outer::SCN), "phal", true, true);
        check(chord(Outer::CN, Second::XI, Vowel::E, Outer::N), "when", true, false);
        check(chord(Outer::S, Second::XI, Vowel::E, Outer::FP), "swet", true, false);
    }

    /// Mirrored vowels: Series 2 supplies the nucleus, silent e, closes.
    #[test]
    fn mirrored_vowels() {
        check(chord(Outer::FP, Second::R, Vowel::Empty, Outer::SZP), "tame", true, true);
        check(chord(Outer::FP, Second::XI, Vowel::Empty, Outer::N), "tone", true, true);
        check(chord(Outer::FP, Second::RX, Vowel::Empty, Outer::SZP), "teame", true, true);
        // RXI is o without the silent e, and does not close.
        check(chord(Outer::FP, Second::RXI, Vowel::Empty, Outer::SZP), "tom", false, true);
        // Without a coda the mirrored reading is off: R is a consonant.
        check(chord(Outer::FP, Second::R, Vowel::Empty, Outer::Empty), "tr", false, true);
        // `ck` does not license it either.
        check(chord(Outer::FP, Second::R, Vowel::Empty, Outer::CZ), "trck", false, true);
        // FN as an onset is `ind`, but `gn` before a mirrored vowel.
        check(chord(Outer::FN, Second::XI, Vowel::Empty, Outer::SZP), "gnome", true, true);
    }

    /// Diphthongs and the free combinations.
    #[test]
    fn diphthongs_and_free() {
        check(chord(Outer::P, Second::U, Vowel::U, Outer::SCN), "paul", true, false);
        check(chord(Outer::P, Second::I, Vowel::Ui, Outer::SCN), "pail", true, true);
        check(chord(Outer::FP, Second::R, Vowel::Ia, Outer::FP), "trout", true, true);
        // Alone, the free combinations are placeholder glyphs.
        check(chord(Outer::Empty, Second::Empty, Vowel::Ea, Outer::Empty), "\u{b0}", true, true);
        check(chord(Outer::Empty, Second::Empty, Vowel::Ia, Outer::Empty), "*", true, true);
    }

    /// A bare final y takes its space from the `ui` keys.
    #[test]
    fn bare_final_y() {
        check(chord(Outer::Empty, Second::Empty, Vowel::Ui, Outer::ZN), "y", true, true);
        check(chord(Outer::Empty, Second::Empty, Vowel::Empty, Outer::ZN), "y", true, false);
    }

    /// The capitalisation coda.
    #[test]
    fn capitalise() {
        check(chord(Outer::FP, Second::Empty, Vowel::Ie, Outer::SCZ), "To", true, false);
    }

    /// Each rule is reported on the strokes that use it.
    #[test]
    fn rules_reported() {
        fn used(c: Chord) -> u8 {
            translate(c).unwrap().rules
        }
        assert_eq!(used(chord(Outer::FP, Second::Empty, Vowel::Ue, Outer::N)), 0);
        assert_eq!(used(chord(Outer::FP, Second::R, Vowel::Empty, Outer::SZP)), rules::MIRRORED);
        assert_eq!(used(chord(Outer::FP, Second::RXI, Vowel::Empty, Outer::SZP)), rules::MIRRORED);
        assert_eq!(used(chord(Outer::FC, Second::R, Vowel::Ua, Outer::N)), rules::CLUSTER);
        assert_eq!(used(chord(Outer::P, Second::XI, Vowel::Ua, Outer::SCN)), rules::XI_H);
        assert_eq!(used(chord(Outer::CN, Second::XI, Vowel::E, Outer::N)), rules::XI_H);
        assert_eq!(used(chord(Outer::S, Second::XI, Vowel::E, Outer::FP)), 0);
        assert_eq!(used(chord(Outer::P, Second::U, Vowel::U, Outer::SCN)), rules::DIPHTHONG);
        assert_eq!(used(chord(Outer::SCP, Second::R, Vowel::Ea, Outer::SZP)), rules::FREE);
        assert_eq!(used(chord(Outer::Empty, Second::Empty, Vowel::Ui, Outer::ZN)), rules::BARE_Y);
        assert_eq!(used(chord(Outer::FP, Second::Empty, Vowel::Ie, Outer::SCZ)), rules::CAPITALISE);
    }

    /// Unassigned combinations, and the coda-only shapes as onsets, are
    /// untranslatable.
    #[test]
    fn untranslatable() {
        assert_eq!(translate(Chord::new(0x067, 0)), None);
        assert_eq!(translate(Chord::new(0, 0x200)), None);
        assert_eq!(translate(Chord::new(0x380, 0x008)), None);
        assert_eq!(translate(chord(Outer::FCZ, Second::Empty, Vowel::A, Outer::Empty)), None);
        // The upper pinky is not an Orsy key.
        assert_eq!(translate(Chord::new(0x014, 0x008)), None);
    }
}
