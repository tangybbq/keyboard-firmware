//! Typing tracker.
//!
//! Keep track of the things we typed, including things that type backspaces
//! over previous things, allowing for corrections due to more redined
//! dictionary lookups as well as additional things.

extern crate alloc;

use alloc::vec::Vec;

/// The typing tracker.  LIMIT is the limit of the history.
pub struct Typer<const LIMIT: usize> {
    words: Vec<Word>,
}

/// A single thing that has been typed.
struct Word {
    /// Characters that typing removed.  These are used to make slight changes
    /// to the previous word, such as fixing word endings and such.
    remove: String,
    /// The new characters that were typed.
    typed: String,
}

impl<const LIMIT: usize> Typer<LIMIT> {
    pub fn new() -> Self {
        Typer { words: Vec::new() }
    }

    /// Add a track of words that we have typed.  The space will be inserted
    /// before if it is needed.
    pub fn add(&mut self, remove: usize, space: bool, typed: &str) {
        if self.words.len() > LIMIT {
            self.words.remove(0);
        }

        // TODO: remove
        let _ = remove;

        let mut word = String::new();
        if space {
            word.push(' ');
        }
        word.push_str(typed);

        println!("*** remove: {}, type: {:?}", 0, word);

        self.words.push(Word { remove: String::new(), typed: word });
    }

    /// Remove the latest thing we typed.
    pub fn remove(&mut self) {
        if let Some(word) = self.words.pop() {
            println!("*** remove: {}, type: {:?}", word.typed.len(), word.remove);
        }
    }
}
