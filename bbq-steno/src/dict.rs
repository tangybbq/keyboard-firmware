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

pub use self::mapdict::{MapDict, MapDictBuilder};
pub use self::translate::Translator;
pub use self::typer::TypeAction;

mod mapdict;
mod ortho;
mod translate;
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
        for len in 1..(longest + 1) {
            let key = &query[..len];
            if let Some(result) = self.lookup(key) {
                best = Some((len, result));
            }
        }

        best
    }
}
