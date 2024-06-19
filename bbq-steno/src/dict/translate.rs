//! Live translation

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
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

    /// The nodes at each state.  These correspond 1:1 with the input strokes.
    history: Vec<Entry>,

    // Tracker of what was typed.
    typer: Typer<HIST_MAX>,
}

/// At a given state, these are the possible places we can go.
#[derive(Debug)]
struct Entry {
    // NFA states at this point.
    nodes: Vec<Box<dyn Selector>>,

    // The decoded text if there is an entry.  This will be raw steno if nothing
    // decodes at this point.
    text: Decoded,

    // How far back in the history have we successfully typed?  0 means we have
    // typed up to the current entry.
    last_typed: usize,

    // Should we capitalize the next word?
    cap_next: bool,

    // Should we auto-insert a space?
    auto_space: bool,
}

impl Entry {
    fn new() -> Entry {
        Entry {
            nodes: Vec::new(),
            text: Decoded::empty(),
            last_typed: 0,
            cap_next: true,
            auto_space: false,
        }
    }
}

/*
#[derive(Debug, Eq, PartialEq)]
struct Translation {
    /// The actual definition.
    definition: String,
}
*/

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
        let fresh: Vec<_> = self.dicts.iter().map(|d| d.clone().selector()).collect();
        for entry in last.nodes.iter().chain(fresh.iter()) {
            if let Some((sel, text)) = entry.lookup_step(stroke) {
                // Dictionaries are in priority order.  Any new entries override
                // those of the same length.
                if let Some(text) = text {
                    if sel.count() >= best_len {
                        best_len = sel.count();
                        best_text = Some(text);
                    }
                }

                nodes.push(sel);
            }
        }

        // If there is a translation here, use it.  Otherwise fake a single
        // stroke definition that is just the raw steno of this stroke.
        let best = if let Some(best) = best_text {
            // println!("raw: {:?}", best);
            Decoded::new(&best, best_len)
        } else {
            Decoded::fake(stroke.to_string())
        };

        // If this definition was multiple strokes, "untype" what was inserted
        // by those previous strokes.  After this, 'pos' will point to the
        // history entry preceeding the current definition.
        // It is important that when we delete, we only delete the entries that
        // were actually words.  They should always line up, so we don't check
        // that here.
        // Skip tracks how many strokes to skip.
        let pos = self.history.len() - best.strokes;
        // println!("best: {:?}", best);
        // println!("history:");
        // for h in &self.history {
        //     println!("  {:?}", h);
        // }

        // TODO: Make an iterator?
        let prior = &self.history[pos];

        // Capitalize the new text, if that is requested.
        // TODO: Add an entry to decoded for force-not-caps.
        let text = if prior.cap_next {
            capitalize(&best.text)
        } else {
            // TODO: Clean this flow up so we don't need an extra allocation here.
            best.text.clone()
        };

        let add_space =
            // If the auto-spacing rules require a space
            (prior.auto_space && best.allow_space_before) &&
            // But, if we are supposed to stitch, remove that space.
            !(prior.text.stitch && best.stitch);

        // type in this result.
        self.typer.replace(best.strokes - 1, add_space, &text);

        // Record this history entry.
        let auto_space = best.space_after;
        let cap_next = best.cap_next;
        self.history.push(Entry {
            nodes,
            text: best,
            last_typed: last.last_typed + 1, // This doesn't seem right.
            auto_space,
            cap_next,
        })
    }

    fn undo(&mut self) {
        // Don't remove the initial entry, which has the initial state.
        if self.history.len() > 1 {
            let _ = self.history.pop();
            self.typer.remove();
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

        let entry = self.history.last().unwrap();
        println!("Entry: {:?}", entry.text);
        for node in &entry.nodes {
            println!("   {:?}", node);
            // Show the node state where we are.
            if let Some(last) = self.history.last() {
                println!("    {:?}", last);
            }
            // Sometimes, it is useful to see all of the entries.
            /* TODO: Doesn't work after abstraction
            if false {
                for i in node.left..node.right {
                    println!("      {} {:?}",
                             StenoWord(node.dict.key(i).to_vec()),
                             node.dict.value(i));
                }
            }
            */
        }
    }

    /// Print out, very verbosely, the state of the translator.
    #[cfg(feature = "std")]
    pub fn show_verbose(&self) {
        let entry = self.history.last().unwrap();
        println!("Entry: {:?}", entry.text);
        for node in &entry.nodes {
            // println!("   {:?}", node);
            node.dump();
            // Show the node state where we are.
            if let Some(last) = self.history.last() {
                println!("    {:?}", last);
            }
            // Sometimes, it is useful to see all of the entries.
            /* TODO: Doesn't work after abstraction
            if false {
                for i in node.left..node.right {
                    println!("      {} {:?}",
                             StenoWord(node.dict.key(i).to_vec()),
                             node.dict.value(i));
                }
            }
            */
        }
    }
}

fn capitalize(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        None => String::new(),
        // There is an extra allocation here, so improve this.
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[derive(Debug)]
struct Decoded {
    allow_space_before: bool,
    space_after: bool,
    cap_next: bool,
    stitch: bool,
    text: String, // TODO: This could be just a &str but that is complicated.
    /// How many strokes does this definition consume.
    strokes: usize,
}

impl Decoded {
    fn new(text: &str, strokes: usize) -> Decoded {
        let mut allow_space_before = true;
        let mut space_after = true;
        let mut cap_next = false;
        let mut stitch = false;

        let mut iter = text.chars();
        let mut text = String::new();

        // Deal with leading characters that control things.
        while let Some(ch) = iter.next() {
            match ch {
                '\x01' => allow_space_before = false,
                // Allow the cap-next marker at the start, to cover the cases
                // when there is no text, and this is just the caps-next marker.
                '\x02' => cap_next = true,
                '\x03' => stitch = true,
                ch => {
                    text.push(ch);
                    break;
                }
            }
        }

        // Just push everything else onto text.
        while let Some(ch) = iter.next() {
            text.push(ch);
        }

        // Now, see what is at the end.
        while let Some(ch) = text.pop() {
            match ch {
                '\x01' => space_after = false,
                '\x02' => cap_next = true,
                ch => {
                    text.push(ch);
                    break;
                }
            }
        }

        Decoded {
            allow_space_before,
            space_after,
            cap_next,
            stitch,
            text,
            strokes,
        }
    }

    fn fake(text: String) -> Decoded {
        Decoded {
            allow_space_before: true,
            space_after: true,
            cap_next: false,
            stitch: false,
            text, strokes: 1,
        }
    }

    fn empty() -> Decoded {
        Decoded {
            allow_space_before: true,
            space_after: true,
            cap_next: false,
            stitch: false,
            text: String::new(),
            strokes: 0,
        }
    }
}
