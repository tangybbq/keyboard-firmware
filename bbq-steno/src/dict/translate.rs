//! Live translation

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::typer::{Typer, TypeAction};
use super::{Dict, Selector};
use crate::Stroke;
#[cfg(feature = "std")]

#[cfg(not(feature = "std"))]
use crate::println;

/// Track a series of translations captured in real-time as they are input.
pub struct Translator {
    /// The dictionaries to use for the lookups.
    dicts: Vec<Dict>,

    /// The nodes at each state.
    history: Vec<Entry>,

    // Tracker of what was typed.
    typer: Typer<HIST_MAX>,
}

/// At a given state, these are the possible places we can go.
#[derive(Debug)]
struct Entry {
    nodes: Vec<Selector>,
    text: Option<String>,

    // How far back in the history have we successfully typed?  0 means we have
    // typed up to the current entry.
    last_typed: usize,
}

impl Entry {
    fn new() -> Entry {
        Entry { nodes: Vec::new(), text: None, last_typed: 0 }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Translation {
    /// The actual definition.
    definition: String,
}

// Maxinum number of entries to keep for undo history. Note that if this is made
// shorter than entries in the dictionary, those entries will never be found.
// const HIST_MAX: usize = 100;
const HIST_MAX: usize = 20;

impl Translator {
    pub fn new(dict: Dict) -> Self {
        Translator {
            dicts: vec![dict],
            history: vec![Entry::new()],
            typer: Typer::new(),
        }
    }

    /// Add a new stroke to the Translator.  Updates the internal state.
    pub fn add(&mut self, stroke: Stroke) {
        if stroke.is_star() {
            self.undo();
        } else {
            self.add_stroke(stroke);
        }
    }

    fn add_stroke(&mut self, stroke: Stroke) {
        let last = self.history.last().unwrap();

        let mut nodes = vec!();
        let mut best_len = 0;
        let mut best_text = None;

        // Iterate over all current nodes, along with an additional epislon node
        // for each dictionary.
        let fresh: Vec<_> = self.dicts.iter().map(|d| Selector::new(d.clone())).collect();
        for entry in last.nodes.iter().chain(fresh.iter()) {
            if let Some((sel, text)) = entry.lookup_step(stroke) {
                // Dictionaries are in priority order.  Any new entries override
                // those of the same length.
                if let Some(text) = text {
                    if sel.count >= best_len {
                        best_len = sel.count;
                        best_text = Some(text);
                    }
                }

                // Unless this node is unique, push it for additional nodes.
                if !sel.unique() {
                    nodes.push(sel);
                }
            }
        }

        // Determine how much previously typed text needs to be deleted.
        if let Some(ref best) = best_text {
            for ent in &self.history[self.history.len() - (best_len - 1) .. self.history.len()] {
                if ent.text.is_some() {
                    self.typer.remove();
                }
            }
            self.typer.add(0, true, best);
        }

        self.history.push(Entry { nodes, text: best_text, last_typed: last.last_typed + 1 });
    }

    fn undo(&mut self) {
        if let Some(entry) = self.history.pop() {
            if entry.text.is_some() {
                self.typer.remove();
            }
        }
    }


    /// Retrieve the next action from the typer.
    pub fn next_action(&mut self) -> Option<TypeAction> {
        self.typer.next_action()
    }

    /// Print out the state of the translator.
    #[cfg(feature = "std")]
    pub fn show(&self) {
        // Just print the latest history, as the history doesn't change.

        use crate::stroke::StenoWord;
        let entry = self.history.last().unwrap();
        println!("Entry: {:?}", entry.text);
        for node in &entry.nodes {
            println!("   {:?}", node);
            // Sometimes, it is useful to see all of the entries.
            if false {
                for i in node.left..node.right {
                    println!("      {} {:?}",
                             StenoWord(node.dict.key(i).to_vec()),
                             node.dict.value(i));
                }
            }
        }
    }

}
