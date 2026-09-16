//! Dividing a word into strokes.
//!
//! The layout has no dictionary, but a word still has to be split into
//! syllables the rules can spell, and usually there is more than one way.
//! [`Writer::write`] finds the fewest strokes that spell a word, ties broken
//! by fewest keys: a shortest path over the word's letters, where each step
//! is a chunk some stroke spells with the right binding.  This is the same
//! search as `write` in `midi4text-analysis/m4t/writer.py`, and it is what
//! the corpus figures in `docs/orsy/` are measured with.
//!
//! The binding is where the word-boundary rule comes in.  Every stroke but
//! the last must bind forward (no space after it), the last must close the
//! word (a space after), and the first must take a space before it; so a
//! fragment that leans back onto the syllable before it can end a word but
//! never start one.
//!
//! A syllable's initial consonant comes from Series 1, which is the manual's
//! general rule and not a preference: the shipped dictionary spells `list`
//! both as `SCNuicf` and as `RIuicf`, and only the first is the method.  It
//! has to be ranked above the key count rather than left to the tie-break,
//! because `l` and `m` are cheaper on the inner keys than on the outer ones
//! -- `RI` is one key where `SCN` is two -- so a tie-break never reaches
//! them.  Fewest strokes still comes first; no syllable is added to keep an
//! onset out of Series 2.
//!
//! Series 2 also must not repeat the onset's own letter.  English has no
//! `ll` or `mm` onset, and the manual divides `attempts` as `at-tem-pts`:
//! a doubled consonant straddles the boundary, its first copy the coda of
//! the syllable before.  Left to the key count this loses, because the coda
//! spelling costs a key more -- `co|llab` is fifteen keys against
//! `col|lab`'s sixteen -- so it too is ranked above the count.  It buys
//! back nothing in strokes: over `words/english_10k.json` it changes 3.6%
//! of divisions for 0.3% more keys and not one extra stroke.
//!
//! Many words then divide equally well in more than one way (`ten|s` and
//! `te|ns` are both two strokes of four keys).  Ties are broken towards fewer
//! inner keys, so a consonant that could be an onset or a second character is
//! the onset -- `set` is `s` on the outer keys, not the second-character `s`
//! on the index finger -- and then towards longer earlier strokes, so a word
//! reads as a syllable and a suffix rather than a stub and a lump.
//!
//! The index is built by translating every chord once, which takes a moment,
//! so a `Writer` is meant to be made once and kept.

use std::collections::HashMap;

use crate::chord::{Chord, HAND_MASK, INNER_MASK};
use crate::compose::translate;

/// How many letters a single stroke can spell.  The longest is `str` plus a
/// second character, a two-letter nucleus, a two-letter coda and the silent
/// `e`, which is fewer than this.
const MAX_CHUNK: usize = 12;

/// One way of spelling a chunk of letters.
#[derive(Clone, Copy, Debug)]
struct Entry {
    chord: Chord,
    space_before: bool,
    space_after: bool,
    keys: u32,
    /// How many of the keys are on the inner four of either hand.
    inner: u32,
    /// 1 when the syllable's initial consonant comes from Series 2.
    inner_onset: u32,
    /// 1 when Series 2 repeats the onset, spelling a doubled consonant.
    doubled: u32,
    rules: u8,
}

/// Divides words into strokes.
pub struct Writer {
    /// The strokes spelling each chunk of letters, fewest keys first.
    chunks: HashMap<String, Vec<Entry>>,
}

impl Default for Writer {
    fn default() -> Self {
        Writer::new()
    }
}

impl Writer {
    /// Index every stroke that spells a run of lowercase letters.
    pub fn new() -> Writer {
        let mut chunks: HashMap<String, Vec<Entry>> = HashMap::new();
        for left in 0u16..0x400 {
            for right in 0u16..0x400 {
                if (left | right) & !HAND_MASK != 0 || (left == 0 && right == 0) {
                    continue;
                }
                let chord = Chord::new(left, right);
                let Some(t) = translate(chord) else { continue };
                let text = t.text();
                if text.is_empty() || !text.bytes().all(|b| b.is_ascii_lowercase()) {
                    continue;
                }
                chunks.entry(text).or_default().push(Entry {
                    chord,
                    space_before: t.space_before,
                    space_after: t.space_after,
                    keys: chord.keys(),
                    inner: ((left | right) & INNER_MASK).count_ones()
                        + (left & right & INNER_MASK).count_ones(),
                    inner_onset: t.inner_onset() as u32,
                    doubled: t.doubled_onset() as u32,
                    rules: t.rules,
                });
            }
        }
        for entries in chunks.values_mut() {
            entries
                .sort_by_key(|e| {
                    (e.inner_onset, e.doubled, e.keys, e.inner, e.chord.left, e.chord.right)
                });
        }
        Writer { chunks }
    }

    /// The fewest strokes spelling `word`, ties broken by fewest keys, or
    /// `None` if the rules cannot spell it.  The word must be lowercase
    /// letters.
    pub fn write(&self, word: &str) -> Option<Vec<Chord>> {
        self.write_with(word, |_, _| true)
    }

    /// As [`write`](Self::write), using only strokes that `allowed` accepts,
    /// given the chord and the composition rules it uses.  For a lesson that
    /// has taught only some of the patterns and rules.
    pub fn write_with(
        &self,
        word: &str,
        allowed: impl Fn(Chord, u8) -> bool,
    ) -> Option<Vec<Chord>> {
        if word.is_empty() || !word.bytes().all(|b| b.is_ascii_lowercase()) {
            return None;
        }
        let n = word.len();
        // best[i]: the cheapest way to spell the first i letters, as
        // (strokes, Series 2 onsets, doubled consonants, keys, inner keys,
        // length of the last stroke's chunk, path).  The Series 2 onsets and
        // the doubled consonants both rank above the key count for the
        // reasons the module header gives; the inner count and the chunk
        // length only decide ties.
        let mut best: Vec<Option<(usize, u32, u32, u32, u32, usize, Vec<Chord>)>> =
            vec![None; n + 1];
        best[0] = Some((0, 0, 0, 0, 0, 0, Vec::new()));
        for i in 0..n {
            let Some((strokes, onsets, doubled, keys, inner, _, path)) = best[i].clone() else {
                continue;
            };
            for j in (i + 1)..=n.min(i + MAX_CHUNK) {
                let Some(entries) = self.chunks.get(&word[i..j]) else { continue };
                let last = j == n;
                let entry = entries.iter().find(|e| {
                    // The first stroke of a word takes a space before it; the
                    // last closes the word; the ones between bind forward.
                    (i > 0 || e.space_before)
                        && e.space_after == last
                        && allowed(e.chord, e.rules)
                });
                let Some(entry) = entry else { continue };
                let candidate = (
                    strokes + 1,
                    onsets + entry.inner_onset,
                    doubled + entry.doubled,
                    keys + entry.keys,
                    inner + entry.inner,
                    j - i,
                );
                let better = match &best[j] {
                    None => true,
                    Some((s, o, d, k, n, l, _)) => candidate < (*s, *o, *d, *k, *n, *l),
                };
                if better {
                    let mut path = path.clone();
                    path.push(entry.chord);
                    best[j] = Some((
                        candidate.0,
                        candidate.1,
                        candidate.2,
                        candidate.3,
                        candidate.4,
                        candidate.5,
                        path,
                    ));
                }
            }
        }
        best[n].take().map(|(.., path)| path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::rules;
    use crate::tables::{Outer, Patterns, Second, Vowel};

    fn texts(writer: &Writer, word: &str) -> Vec<String> {
        writer
            .write(word)
            .unwrap_or_else(|| panic!("{word} unwritable"))
            .iter()
            .map(|c| translate(*c).unwrap().text())
            .collect()
    }

    #[test]
    fn divisions() {
        let w = Writer::new();
        assert_eq!(texts(&w, "dream"), ["dream"]);
        assert_eq!(texts(&w, "ten"), ["ten"]);
        // A coda-only fragment can end a word.
        assert_eq!(texts(&w, "tens"), ["ten", "s"]);
        // The mirrored vowel makes this one stroke.
        assert_eq!(texts(&w, "time"), ["time"]);
        // Across a syllable boundary.
        assert_eq!(texts(&w, "tennis").len(), 2);
    }

    /// Ties in strokes are broken by fewest keys, then by fewest inner keys:
    /// a consonant that could be an onset or a second character is the onset.
    #[test]
    fn fewest_keys() {
        let w = Writer::new();
        let strokes = w.write("ten").unwrap();
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].keys(), 4);
        let set = w.write("set").unwrap();
        assert_eq!(set.len(), 1);
        assert_eq!(Patterns::of(set[0]).unwrap().onset, Outer::S);
        assert_eq!(Patterns::of(set[0]).unwrap().second, Second::Empty);
    }

    /// A syllable's initial consonant comes from Series 1, even where Series 2
    /// would spell it with fewer keys.
    #[test]
    fn initial_consonant_is_series_1() {
        let w = Writer::new();
        // `l` is two keys on the outer row and one on the inner, so the key
        // count alone would put it on the index finger.
        for word in ["let", "list", "last", "law"] {
            let strokes = w.write(word).unwrap();
            let p = Patterns::of(strokes[0]).unwrap();
            assert_eq!(p.onset, Outer::SCN, "{word}");
            assert_eq!(p.second, Second::Empty, "{word}");
        }
        // The same for `m`, which is three keys against two.
        let me = w.write("me").unwrap();
        assert_eq!(Patterns::of(me[0]).unwrap().onset, Outer::SZP);
        // Series 2 still carries the second character of a cluster.
        let plan = w.write("plan").unwrap();
        assert_eq!(Patterns::of(plan[0]).unwrap().onset, Outer::P);
        assert_eq!(Patterns::of(plan[0]).unwrap().second, Second::RI);
        // A vowel from Series 2 may open a syllable: `ai` is the diphthong.
        assert_eq!(texts(&w, "aid"), ["aid"]);
    }

    /// A doubled consonant straddles the stroke boundary rather than being
    /// spelled as an onset cluster, even though the coda costs a key more.
    #[test]
    fn doubled_consonants_straddle() {
        let w = Writer::new();
        assert_eq!(texts(&w, "collaborate"), ["col", "lab", "or", "ate"]);
        assert_eq!(texts(&w, "comment"), ["com", "ment"]);
        assert_eq!(texts(&w, "ally"), ["al", "ly"]);
        // And no stroke of any of them spells the doubling itself.
        for word in ["collaborate", "comment", "ally", "illusion", "bestseller"] {
            for stroke in w.write(word).unwrap() {
                assert!(!translate(stroke).unwrap().doubled_onset(), "{word}");
            }
        }
        // A cluster that is not a doubling is untouched.
        assert_eq!(texts(&w, "please"), ["ple", "ase"]);
    }

    /// A fragment that leans back cannot start a word, and nothing unspellable
    /// is written.
    #[test]
    fn unwritable() {
        let w = Writer::new();
        assert_eq!(w.write("q"), None);
        assert_eq!(w.write("Ten"), None);
        assert_eq!(w.write(""), None);
        // Every stroke of a written word takes a space before it or follows
        // one that binds forward.
        for word in ["nt", "ntn", "sting"] {
            if let Some(strokes) = w.write(word) {
                assert!(translate(strokes[0]).unwrap().space_before, "{word}");
            }
        }
    }

    /// Restricting the strokes changes the division, or makes the word
    /// unwritable.
    #[test]
    fn restricted() {
        let w = Writer::new();
        // Without the mirrored-vowel rule, `time` needs two strokes.
        let strokes = w.write_with("time", |_, r| r & rules::MIRRORED == 0).unwrap();
        assert_eq!(strokes.len(), 2);
        // Without any way to spell an `i`, it cannot be written.
        let no_i = |c: Chord, _: u8| {
            let p = Patterns::of(c).unwrap();
            p.second != Second::I && p.vowel != Vowel::I && p.vowel != Vowel::Ui
        };
        assert_eq!(w.write_with("time", no_i), None);
    }
}
