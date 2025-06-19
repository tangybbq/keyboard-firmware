//! Dictionary operations.
//!
//! A steno dictionary contains a mapping between steno strokes and definitions.
//! The definitions are represented as strings that are a mix of literal text to
//! be "typed" and control sequences that simulate other behavior, and control
//! how the text is combined together.  This module does not implement the
//! dictionaries themselves, but represents the support code to handle
//! performing dictionary lookups of strokes as they come in.

extern crate alloc;

use core::fmt::Debug;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};

use crate::Stroke;

pub use self::mapdict::{RamDict, MapDictBuilder};
pub use self::translate::Translator;
pub use self::typer::TypeAction;
pub use self::lookup::Lookup;
pub use self::joiner::{Joiner, Joined, State};
pub use self::emily::EmilySymbols;

mod emily;
mod joiner;
mod lookup;
mod mapdict;
mod ortho;
mod translate;
pub mod typer;

pub type Dict = Rc<dyn DictImpl>;

/// A selector is something that maintains a selection over a partial range
/// within a dictionary.
pub trait Selector: Debug {
    /// Lookup an additional stroke within this selector.  If this still results
    /// in entries being present, returns a new selector, as well as a possible
    /// translation if this stroke is sufficient to give a result.
    fn lookup_step(&self, key: Stroke) -> Option<(Box<dyn Selector>, Option<String>)>;

    /// Return if this result is unique, meaning that there are no longer
    /// dictionary entries that could be found by searching for additional strokes.
    fn unique(&self) -> bool;

    /// Return a count of the number of strokes that have been selected.
    fn count(&self) -> usize;

    /// Print this selector, verbosely.  This will print all entries that match.
    fn dump(&self);
}

/// A Selector over a dictionary tracks a range of the dictionary that specifies
/// a range of entries in the dictionary that cover a given prefix.
pub struct BinarySelector {
    /// The dictionary this entry applies to.
    dict: Dict,

    /// The number of strokes that have been matched so far.
    pub count: usize,

    /// Start and stop are the bounds of the lookup.  These are Rust-style
    /// range, where stop is one past the end, and not like traditional btree
    /// lookups where stop is inclusive.
    pub left: usize,
    pub right: usize,
}

impl alloc::fmt::Debug for BinarySelector {
    fn fmt(&self, f: &mut alloc::fmt::Formatter) -> Result<(), alloc::fmt::Error> {
        // Don't print the dict.
        write!(f, "Selector {{ count: {}, left: {}, right: {}}}",
               self.count, self.left, self.right)
    }
}

impl BinarySelector {
    /// Create the empty selector, that selects no strokes entered.
    pub fn new(dict: Dict) -> BinarySelector {
        let left = 0;
        let right = dict.len();
        BinarySelector {
            dict,
            left,
            right,
            count: 0,
        }
    }
}

impl Selector for BinarySelector {
    /// Perform a single lookup step.  Returns a new cursor that matches the
    /// given token.  If there are zero entries in the dictionary that match,
    /// this will return None.
    fn lookup_step(&self, key: Stroke) -> Option<(Box<dyn Selector>, Option<String>)> {
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
            Some((Box::new(BinarySelector {
                dict: self.dict.clone(),
                count: self.count + 1,
                left,
                right,
            }),
                  text))
        } else {
            None
        }
    }

    /// Is this selector uniqueue, meaning will any additional strokes possibly
    /// result in more translations?
    fn unique(&self) -> bool {
        self.left + 1 == self.right
    }

    fn count(&self) -> usize {
        self.count
    }

    // The selector is only implemented with std.
    #[cfg(feature = "std")]
    fn dump(&self) {
        use crate::stroke::{StenoPhrase, StenoWord};

        println!("Selector: {:?}", self);
        for pos in self.left .. self.right {
            println!("  {:8}: {:?}: {:?}", pos,
                     StenoPhrase(vec![StenoWord(self.dict.key(pos).to_vec())]).to_string(),
                     self.dict.value(pos));
        }
    }

    #[cfg(not(feature = "std"))]
    fn dump(&self) {
    }
}

/// Implementations of the dictionary will need to provide this view, of the
/// dictionary with sorted keys.
pub trait DictImpl {
    fn len(&self) -> usize;
    fn key(&self, index: usize) -> &[Stroke];
    fn value(&self, index: usize) -> &str;
    fn selector(self: Rc<Self>) -> Box<dyn Selector>;

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

            if pos >= k.len() || needle > k[pos] {
                left = mid + 1;
            } else {
                right = mid;
            }
        }

        // Not found, this is our first key greater than the current one.
        left
    }
}
