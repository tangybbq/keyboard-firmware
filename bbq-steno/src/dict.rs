//! Dictionary operations.
//!
//! A steno dictionary contains a mapping between steno strokes and definitions.
//! The definitions are represented as strings that are a mix of literal text to
//! be "typed" and control sequences that simulate other behavior, and control
//! how the text is combined together.  This module does not implement the
//! dictionaries themselves, but represents the support code to handle
//! performing dictionary lookups of strokes as they come in.

extern crate alloc;

use crate::Stroke;
use crate::stroke::StenoWord;

// Bring in the alloc versions in case we are nostd.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use self::typer::Typer;

mod typer;

/// A Dictionary is something that strokes can be looked up in.
pub trait Dict {
    /// The core lookup, works like a Map lookup, finding exact matches and
    /// their results.  It is possible that longer sequences of strokes will
    /// match other definitions, and searches fill fail if the input is longer
    /// than an entry.
    fn lookup<'a>(&'a self, strokes: &[Stroke]) -> Option<&'a str>;

    /// Determine the longest stroke sequence used as a key in the dictionary.
    /// This is needed for the naive implementation of `prefix_lookup`. It is
    /// recommended that this value be cached, or pre-computed, as it will be
    /// used for each `prefix_lookup`. If an implementation provides its own
    /// `prefix_lookup` performance of this routine is not internally important.
    fn longest_key(&self) -> usize;

    /// Perform a prefix lookup.  Similar to `lookup`, but will return success
    /// if the matched string only returns a prefix of the input.  The return
    /// result is a pair of the number strokes in the match, and the result of
    /// the match.  This is the longest match with the given input strokes, but
    /// adding more strokes and searching again could result in a different
    /// result.  If a dictionary implementation is able, this should be
    /// overridden.
    fn prefix_lookup<'a>(&'a self, query: &[Stroke]) -> Option<(usize, &'a str)> {
        // Limit the query the longest key.
        let longest = self.longest_key().min(query.len());

        let mut best = None;

        // Because we don't have insight into the dictionary, there isn't really
        // much more to do than to lookup all possible prefixes.
        for len in 1..(longest+1) {
            let key = &query[..len];
            if let Some(result) = self.lookup(key) {
                best = Some((len, result));
            }
        }

        best
    }
}

/// A simple dictionary implementation.  This is implemented by storing Vecs of
/// the keys, and Strings as the values.
pub struct MapDict {
    map: BTreeMap<Vec<Stroke>, String>,
    longest: usize,
}

/// A dictionary builder.
pub struct MapDictBuilder {
    map: BTreeMap<Vec<Stroke>, String>,
}

impl Dict for MapDict {
    fn lookup<'a>(&'a self, query: &[Stroke]) -> Option<&'a str> {
        self.map.get(query).map(|s| s.as_ref())
    }

    fn longest_key(&self) -> usize {
        self.longest
    }
}

impl MapDictBuilder {
    pub fn new() -> MapDictBuilder {
        MapDictBuilder { map: BTreeMap::new() }
    }

    /// Insert a definition.
    pub fn insert(&mut self, key: Vec<Stroke>, definition: String) {
        self.map.insert(key, definition);
    }

    /// Freeze the dictionary.
    pub fn into_map_dict(self) -> MapDict {
        let longest = self.map.keys().map(|k| k.len()).max().unwrap_or(0);
        MapDict { map: self.map, longest }
    }
}

/// Track a series of translations captured in real-time as they are input.
pub struct Translator<'a, D: Dict> {
    // The translations we've seen, most recent at the end.
    entries: Vec<Entry<'a>>,
    // The dictionary to use.
    dict: &'a D,
    // Cache of the longest key.
    longest: usize,
    // Tracker of what was typed.
    typer: Typer<HIST_MAX>,
}

/// Each stroke that is captured has with it some possible data.
#[derive(Debug)]
struct Entry<'a> {
    /// The strokes that make up this translation.
    strokes: Vec<Stroke>,
    /// What this translates to, if anything.
    translation: Option<Translation<'a>>,
}

#[derive(Debug, Eq, PartialEq)]
struct Translation<'a> {
    /// The actual definition.
    definition: &'a str,
}

// Maxinum number of entries to keep for undo history. Note that if this is made
// shorter than entries in the dictionary, those entries will never be found.
// const HIST_MAX: usize = 100;
const HIST_MAX: usize = 20;

impl<'a, D: Dict> Translator<'a, D> {
    pub fn new(dict: &'a D) -> Self {
        Translator {
            entries: Vec::new(),
            dict,
            longest: dict.longest_key(),
            typer: Typer::new(),
        }
    }

    /// Add a new stroke to the Translator.  Updates the internal state.

    pub fn add(&mut self, stroke: Stroke) {
        if stroke.is_star() {
            self.modify(None);
        } else {
            self.modify(Some(stroke));
        }
    }

    fn modify(&mut self, stroke: Option<Stroke>) {
        // Clean up excessive history.
        if self.entries.len() >= HIST_MAX {
            let _ = self.entries.remove(0);
        }

        // Track back as far as needed for the longest definition necessary.
        let mut base = self.entries.len();
        let mut count = 0;
        while count < self.longest && base > 0 {
            base -= 1;
            count += self.entries[base].strokes.len();
        }
        println!("Base: {}", base);

        // Build up a new list of strokes, to build a new translation.
        let mut strokes = Vec::new();
        for ent in &self.entries[base..] {
            strokes.extend_from_slice(&ent.strokes);
        }

        if let Some(stroke) = stroke {
            // Add in the newly received stroke.
            strokes.push(stroke);
        } else {
            // Remove a stroke.
            let _ = strokes.pop();
        }
        println!("Try translate: {}", StenoWord(strokes.clone()));

        let mut xlat = self.translate(&strokes);

        // Determine the delta, and use that to determine what to type.
        let mut old_iter = self.entries[base..].iter();
        let mut new_iter = xlat.iter();

        loop {
            let old_elt = old_iter.next();
            let new_elt = new_iter.next();
            if old_elt.is_none() && new_elt.is_none() {
                break;
            }
            if old_elt.is_none() {
                let new_elt = new_elt.unwrap();
                // New translation, just type this.
                self.typer.add(0, true, new_elt.text());
                continue;
            }
            if new_elt.is_none() {
                // Delete, git rid of things typed.
                // TODO: use delete_from.
                self.typer.remove();
                while let Some(_) = old_iter.next() {
                    self.typer.remove();
                }

                break;
            }

            let old_elt = old_elt.unwrap();
            let new_elt = new_elt.unwrap();

            // If the translation differ, here is our undo point.
            if old_elt.translation != new_elt.translation {
                // Delete from the new elt on.

                self.typer.remove();
                while let Some(_) = old_iter.next() {
                    self.typer.remove();
                }

                self.typer.add(0, true, new_elt.text());
                while let Some(new_elt) = new_iter.next() {
                    self.typer.add(0, true, new_elt.text());
                }
                break;
            }

            // Otherwise, they are equal, and we continue.
        }

        // Replace the translation with this one.
        while self.entries.len() > base {
            // Is there a better way to pop these?
            let _ = self.entries.pop();
        }
        self.entries.append(&mut xlat);
    }

    /*
    fn delete_from<'b: 'a, I: Iterator<Item = &'b Entry<'a>>>(elt: &'b Entry, elt_iter: I) {
        let mut hold = vec![elt];
        hold.extend(elt_iter);

        while let Some(elt) = hold.pop() {
            println!("Delete: {:?}", elt.translation);
        }
    }
    */

    /// Print out the state of the translator.
    pub fn show(&self) {
        for e in &self.entries {
            println!("{:>10} {:?}", StenoWord(e.strokes.clone()).to_string(), e.translation);
        }
    }

    /// Compute a translation set from the given strokes.
    fn translate(&self, strokes: &[Stroke]) -> Vec<Entry<'a>> {
        let mut result = Vec::new();

        let mut pos = 0;
        while pos < strokes.len() {
            if let Some((len, defn)) = self.dict.prefix_lookup(&strokes[pos..]) {
                result.push(Entry {
                    strokes: strokes[pos..pos+len].to_vec(),
                    translation: Some(Translation { definition: defn }),
                });
                pos += len;
            } else {
                result.push(Entry {
                    strokes: vec![strokes[pos]],
                    translation: None,
                });
                pos += 1;
            }
        }
        result
    }

    /*
    /// Add a new stroke to the Translator.  Returns what we know about the translation so far.
    pub fn add(&mut self, stroke: Stroke) {
        if self.seen.len() >= self.longest {
            self.seen.remove(0);
            self.lens.remove(0);
        }
        self.seen.push(stroke);
        // self.lens.push(0);

        let mut new_lens = Vec::with_capacity(self.seen.len());

        let mut pos = 0;
        while pos < self.seen.len() {
            if let Some((len, _defn)) = self.dict.prefix_lookup(&self.seen[pos..]) {
                new_lens.push(len);
                for _ in 1..len {
                    new_lens.push(0);
                }
                pos += len;
            } else {
                new_lens.push(0);
                pos += 1;
            }
        }
        #[cfg(feature = "std")]
        println!("Lookup, old {:?}", self.lens);
        #[cfg(feature = "std")]
        println!("        new {:?}", new_lens);

        self.lens = new_lens;
    }
    */
}

impl<'a> Entry<'a> {
    fn text(&self) -> &str {
        if let Some(tr) = &self.translation {
            tr.definition
        } else {
            "TODO:RAW"
        }
    }
/*
    fn new_empty(stroke: Stroke) -> Self {
        Entry {
            strokes: vec![stroke],
            translation: None,
        }
    }
*/
}
