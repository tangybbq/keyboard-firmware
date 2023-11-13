//! Dictionary operations.
//!
//! A steno dictionary contains a mapping between steno strokes and definitions.
//! The definitions are represented as strings that are a mix of literal text to
//! be "typed" and control sequences that simulate other behavior, and control
//! how the text is combined together.  This module does not implement the
//! dictionaries themselves, but represents the support code to handle
//! performing dictionary lookups of strokes as they come in.

extern crate alloc;

use alloc::rc::Rc;

use crate::Stroke;

pub use self::mapdict::{RamDict, MapDict, MapDictBuilder};
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

/// A Selector over a dictionary tracks a range of the dictionary that specifies
/// a range of entries in the dictionary that cover a given prefix.
pub struct Selector {
    /// The dictionary this entry applies to.
    dict: Rc<dyn DictImpl>,

    /// The number of strokes that have been matched so far.
    pub count: usize,

    /// Start and stop are the bounds of the lookup.  These are Rust-style
    /// range, where stop is one past the end, and not like traditional btree
    /// lookups where stop is inclusive.
    pub left: usize,
    pub right: usize,
}

impl Selector {
    /// Create the empty selector, that selects no strokes entered.
    pub fn new(dict: Rc<dyn DictImpl>) -> Selector {
        let left = 0;
        let right = dict.len();
        Selector {
            dict,
            left,
            right,
            count: 0,
        }
    }

    /// Perform a single lookup step.  Returns a new cursor that matches the
    /// given token.  If there are zero entries in the dictionary that match,
    /// this will return None.
    pub fn lookup_step(&self, key: Stroke) -> Option<(Selector, Option<String>)> {
        let left = self.dict.scan(self.left, self.right, self.count, key);
        // println!("left = {}", left);
        let right = self.dict.scan(self.left, self.right, self.count, key.succ());
        // println!("right = {}", right);
        if right > left {
            let key = self.dict.key(left);
            let text = if key.len() == self.count + 1 {
                Some(self.dict.value(left).to_string())
            } else {
                None
            };
            Some((Selector {
                dict: self.dict.clone(),
                count: self.count + 1,
                left,
                right,
            },
                  text))
        } else {
            None
        }
    }

    /// Is this selector uniqueue, meaning will any additional strokes possibly
    /// result in more translations?
    pub fn unique(&self) -> bool {
        self.left + 1 == self.right
    }
}

/// Implementations of the dictionary will need to provide this view, of the
/// dictionary with sorted keys.
pub trait DictImpl {
    fn len(&self) -> usize;
    fn key(&self, index: usize) -> &[Stroke];
    fn value(&self, index: usize) -> &str;

    /// For a given range of the dictionary, do a binary search for the given
    /// key as the nth character of a key.
    fn scan(&self, a: usize, b: usize, pos: usize, needle: Stroke) -> usize {
        // This is taken from the Rust std slice's binary_search_by.
        let mut left = a;
        let mut right = b;
        while left < right {
            let mid = left + (right - left) / 2;
            let k = self.key(mid);
            // println!("scan: {} {} {}, k:{}, pos:{}, n:{}", left, right, mid,
            //          StenoWord(k.to_vec()),
            //          pos, needle);
            // If this entry matches, and the length is exact, we can stop.
            if pos == k.len() - 1 && k[pos] == needle {
                // println!("  found at: {}", mid);
                return mid;
            }

            if needle > k[pos] {
                // If the needls is past this entry, move the right.
                // println!("search is before find point, move left");
                left = mid + 1;
            } else {
                // Otherwise, we are to the left, advance the left of the search.
                // println!("search is past find point, move right");
                right = mid;
            }
        }

        // Not found, this is our first key greater than the current one.
        left
    }
}
