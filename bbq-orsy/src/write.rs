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
//! Many words divide equally well in more than one way (`ten|s` and `te|ns`
//! are both two strokes of four keys).  Ties are broken towards fewer inner
//! keys, so a consonant that could be an onset or a second character is the
//! onset -- `set` is `s` on the outer keys, not the second-character `s` on
//! the index finger -- and then towards longer earlier strokes, so a word
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
                    rules: t.rules,
                });
            }
        }
        for entries in chunks.values_mut() {
            entries.sort_by_key(|e| (e.keys, e.inner, e.chord.left, e.chord.right));
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
        // (strokes, keys, inner keys, length of the last stroke's chunk,
        // path).  The inner count and the chunk length only decide ties.
        let mut best: Vec<Option<(usize, u32, u32, usize, Vec<Chord>)>> = vec![None; n + 1];
        best[0] = Some((0, 0, 0, 0, Vec::new()));
        for i in 0..n {
            let Some((strokes, keys, inner, _, path)) = best[i].clone() else { continue };
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
                let candidate = (strokes + 1, keys + entry.keys, inner + entry.inner, j - i);
                let better = match &best[j] {
                    None => true,
                    Some((s, k, n, l, _)) => candidate < (*s, *k, *n, *l),
                };
                if better {
                    let mut path = path.clone();
                    path.push(entry.chord);
                    best[j] = Some((candidate.0, candidate.1, candidate.2, candidate.3, path));
                }
            }
        }
        best[n].take().map(|(_, _, _, _, path)| path)
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
