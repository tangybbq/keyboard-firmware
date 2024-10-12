//! Dictionary lookup.
//!
//! Lookup manages the lookups using a set of dictionaries.  The Dictionaries all implement
//! [`Dict`], which allows incremental lookup.
//!
//!
//! The output of the translation is an "Action" which indicates 1. the translation text for this
//! lookup, and 2. How many previous actions this replaces.  The replacement is due finding longer
//! matches in the dictionary.
//!
//! In addition to an action, there is also an Undo, which repeals the previous action.  Other
//! layers will need to potentially replay what was replaced by the deleted action.  We maintain a
//! limited amount of undo history due to memory constraints.
//!
//! The only thing this layer knows about the translations is the concept of an undo barrier. This
//! mostly comes from translations that indicate direct keypresses, and when these are sent, it is
//! not meaningful to undo.  Lookup will simple discard the undo history when these are encountered.

use heapless::Deque;

use crate::Stroke;

use super::{Dict, Selector};

extern crate alloc;

/// The maximum history length.  This should be the sum of the desired history and the longest
/// stroke we will process.  Realistically, undo is not typically done more than a dozen.
const HISTORY_LEN: usize = 32;

/// A Deque that can hold `HISTORY_LEN` entries.
type HistoryDeque<T> = Deque<T, HISTORY_LEN>;

/// Track dictionary lookups maintaining undo history.
pub struct Lookup {
    /// The dictionaries to use for the lookups.
    dicts: Vec<Dict>,

    /// The nodes at each state.  These correspond 1:1 with the input strokes.  New values go to
    /// "back", and are removed from the front as history expires.
    history: HistoryDeque<Entry>,
}

/// At a given state, these are the possible places we can go.
#[derive(Debug)]
struct Entry {
    /// NFA states at this point.
    nodes: Vec<Box<dyn Selector>>,
}

impl Entry {
    fn new() -> Entry {
        Entry {
            nodes: Vec::new(),
        }
    }
}

/// Value returned for each stroke.
///
/// Indicates what text should be typed for this translation, as well as how many strokes this entry
/// consumed.  The replacement will be for 1 less than the number of strokes as this.
#[derive(Debug)]
pub enum Action {
    /// This stroke added a definition, with the given text, and 
    Add {
        /// The text of the translation.
        text: String,
        /// The number of strokes consumed by this insertion.
        strokes: usize,
    },
    /// The undo key was pressed.
    Undo,
}

impl Lookup {
    pub fn new(dicts: Vec<Dict>) -> Self {
        let mut history = HistoryDeque::new();
        history.push_back(Entry::new()).unwrap();

        Lookup {
            dicts,
            history,
        }
    }

    /// Add a new stroke to the Translator.  Updates the internal state.
    pub fn add(&mut self, stroke: Stroke) -> Action {
        if stroke.is_star() {
            self.undo()
        } else {
            self.add_stroke(stroke)
        }
    }

    fn add_stroke(&mut self, stroke: Stroke) -> Action {
        // The history should never be empty.
        let last = self.history.back().unwrap();

        let mut nodes = vec![];
        let mut best_len = 0;
        let mut best_text = None;

        // Iterate over all current nodes, along with an additional episilon node for each
        // dictionary.
        let fresh: Vec<_> = self.dicts.iter().map(|d| d.clone().selector()).collect();
        for entry in last.nodes.iter().chain(fresh.iter()) {
            if let Some((sel, text)) = entry.lookup_step(stroke) {
                // Dictionaries are in priority.  Any new entries override those of the same length.
                if let Some(text) = text {
                    if sel.count() >= best_len {
                        best_len = sel.count();
                        best_text = Some(text);
                    }
                }

                nodes.push(sel);
            }
        }

        // If we got a translation, use it.  Otherwise fake a single stroke definition that is just
        // the raw steno of this stroke.
        let (best, best_len) = if let Some(best) = best_text {
            (best, best_len)
        } else {
            (stroke.to_string(), 1) 
        };

        // When we have a match, we will never go back to previous matches that were shorter.  Think
        // of this:
        //     a b
        //       b c
        // When we get the input 'a b', we have matched that, and need to remove the partial matches
        // for 'b', since we won't consider them.  Note that this is _not_ what Plover does, so this
        // probably won't do the right thing with the plover dictionary.  This is intentional.
        let nodes: Vec<_> = nodes.into_iter().filter(|x| x.count() >= best_len).collect();

        // Purge an entry from the Deque if needed.
        if self.history.len() == self.history.capacity() {
            let _ = self.history.pop_front();
        }

        // Add a new node to the history.
        self.history.push_front(Entry { nodes }).unwrap();

        // TODO: Purge the history when something is seen that is a keypress.

        // This is a type action.
        Action::Add { text: best, strokes: best_len }
    }

    fn undo(&mut self) -> Action {
        // Be sure to not remove the first entry, as we need at least one starting point. This might
        // be potentially confusing, though.
        if self.history.len() > 1 {
            let _ = self.history.pop_front();
        }
        Action::Undo
    }

    /// Print short debugging information for the current state.
    #[cfg(feature = "std")]
    pub fn show(&self) {
        println!("Lookup show");
    }

    /// Print verbose debugging information for the current state.
    #[cfg(feature = "std")]
    pub fn show_verbose(&self) {
        println!("Lookup show_verbose");
    }
}
