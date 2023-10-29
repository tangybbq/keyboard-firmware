//! Live translation

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::typer::Typer;
use super::Dict;
#[cfg(feature = "std")]
use crate::stroke::StenoWord;
use crate::Stroke;

#[cfg(not(feature = "std"))]
use crate::println;

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
            let _ = e;
            println!(
                "{:>10} {:?}",
                StenoWord(e.strokes.clone()).to_string(),
                e.translation
            );
        }
    }

    /// Compute a translation set from the given strokes.
    fn translate(&self, strokes: &[Stroke]) -> Vec<Entry<'a>> {
        let mut result = Vec::new();

        let mut pos = 0;
        while pos < strokes.len() {
            if let Some((len, defn)) = self.dict.prefix_lookup(&strokes[pos..]) {
                result.push(Entry {
                    strokes: strokes[pos..pos + len].to_vec(),
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
