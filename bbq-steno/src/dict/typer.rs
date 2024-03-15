//! Typing tracker.
//!
//! Keep track of the things we typed, including things that type backspaces
//! over previous things, allowing for corrections due to more redined
//! dictionary lookups as well as additional things.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::VecDeque;

#[cfg(not(feature = "std"))]
use crate::println;

/// The typing tracker.  LIMIT is the limit of the history.
pub struct Typer<const LIMIT: usize> {
    _words: Vec<Word>,

    /// Things to be typed.
    to_type: VecDeque<TypeAction>,
}

/// A single thing that has been typed.
struct Word {
    /// Characters that typing removed.  These are used to make slight changes
    /// to the previous word, such as fixing word endings and such.
    remove: String,
    /// The new characters that were typed.
    typed: String,
}

/// The action that results from text being typed.
#[derive(Debug)]
pub struct TypeAction {
    /// How many characters to remove before typing this text.
    pub remove: usize,
    /// The text to type.
    pub text: String,
}

impl<const LIMIT: usize> Typer<LIMIT> {
    pub fn new() -> Self {
        Typer {
            _words: Vec::new(),
            to_type: VecDeque::new(),
        }
    }

    /// Add a track of words that we have typed.  The space will be inserted
    /// before if it is needed.
    #[allow(dead_code)]
    pub fn add(&mut self, remove: usize, space: bool, typed: &str) {
        if self._words.len() > LIMIT {
            self._words.remove(0);
        }

        // TODO: remove
        let _ = remove;

        let mut word = String::new();
        if space {
            word.push(' ');
        }
        word.push_str(typed);

        self.to_type.push_back(TypeAction { remove: 0, text: word.clone() });
        // Search something that won't match to excercise all of the patterns.
        let _combined = super::ortho::combine("run", "zzz");
        // println!("*** remove: {}, type: {:?}", 0, word);

        self._words.push(Word {
            remove: String::new(),
            typed: word,
        });
    }

    /// Remove the latest thing we typed.
    pub fn remove(&mut self) {
        if let Some(word) = self._words.pop() {
            let _ = word.typed;
            let _ = word.remove;
            println!("*** remove: {}, type: {:?}", word.typed.len(), word.remove);
            // TODO: Use the ortho rules.
            self.to_type.push_back(TypeAction { remove: word.typed.len(), text: word.remove });
        }
    }

    /// Retrieve the actions that have resulted from translation.
    pub fn next_action(&mut self) -> Option<TypeAction> {
        self.to_type.pop_front()
    }
}
