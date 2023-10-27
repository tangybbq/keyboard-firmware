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

// Bring in the alloc versions in case we are nostd.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

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
    // The strokes we have seen.
    seen: Vec<Stroke>,
    // Parallel to 'seen', the lengths of definitions we have seen. When a
    // definition has multiple strokes, the subsequent strokes will have a value
    // here of zero. Note that this isn't distinguished from words that don't
    // translate, as either will have to be undone.
    lens: Vec<usize>,
    // The dictionary to use.
    dict: &'a D,
    // Cache of the longest key.
    longest: usize,
}

impl<'a, D: Dict> Translator<'a, D> {
    pub fn new(dict: &'a D) -> Self {
        Translator {
            seen: Vec::new(),
            lens: Vec::new(),
            dict,
            longest: dict.longest_key(),
        }
    }

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
}
