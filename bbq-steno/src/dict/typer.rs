//! Typing tracker.
//!
//! Keep track of the things we typed, including things that type backspaces
//! over previous things, allowing for corrections due to more redined
//! dictionary lookups as well as additional things.

// Implementation:
//
// This is actually a bit tricky. It is notable that Plover has bugs in the
// equivalent code, especially regards to undo.
//
// There are basically two operations we need to support. One is "replace". This
// inserts new text, after removing what was typed from zero or more strokes.
//
// The other is a plan undo. The undo operation is very similar to the first
// part of the replace operation, except that it then removes the record that
// the stroke was even made.
//
// For a few examples of the complexity:
//
// In Phoenix, "PH*FP" is "matter of principle", while "PH*FP/PH*FP" is the
// single colon, with a cap next. Image the user types "PH*FP/PH*FP/*". The
// first stroke will insert "matter of principle" (possibly with a leading
// space). The next stroke will delete this "matter of principle" and insert the
// colon. Then the undo stroke will delete the colon, and retype the "matter of
// principle" text.
//
// Given the following:
// "APB" -> "an"
// "ABG" -> "ac"
// "ABG/TKEPL" -> "academ"
// "ABG/TKEPL/-BG" -> "academic"
//
// Writing out "APB/ABG/TKEPL/BG" will write:
// "an", " ac", bsp 3, " academ", bsp 7, "academic".
//
// It is clear from this example that we also need the ability to fold deletes
// an typed text to improve this. But this isn't necessary, and can be
// implemented after the main algorithm is implemented.
//
// To help understand how to do this, we see that every stroke written always
// types something. It is possible that what is typed is blank, but even
// untranslates will insert raw steno.
//
// When the user writes a 3 stroke word, it is necessary to undo the actions of
// the previous 2 strokes before we can type the new text.
//
// Undo is actually two separate actions. First, it will need to remove some
// number of characters that were typed. Then it will need retype the characters
// that were removed by that action. When doing a replace, the last retype is
// not needed, as it is effectively folded into what we are doing now.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::VecDeque;

// #[cfg(not(feature = "std"))]
// use crate::println;

/// The typing tracker.  LIMIT is the limit of the history.
pub struct Typer<const LIMIT: usize> {
    words: Vec<Word>,

    /// Things to be typed.
    to_type: VecDeque<TypeAction>,
}

/// A single thing that has been typed.
struct Word {
    /// Characters that typing removed.  These are used to make slight changes
    /// to the previous word, such as fixing word endings and such.
    remove: String,
    /// The text typed by this word.
    typed: String,
}

/// The action that results from text being typed.
#[derive(Debug, Eq, PartialEq)]
pub struct TypeAction {
    /// How many characters to remove before typing this text.
    pub remove: usize,
    /// The text to type.
    pub text: String,
}

impl<const LIMIT: usize> Typer<LIMIT> {
    pub fn new() -> Self {
        Typer {
            words: Vec::new(),
            to_type: VecDeque::new(),
        }
    }

    /// Add something related to a new stroke, replacing zero or more previously written strokes.
    pub fn replace(&mut self, remove: usize, space: bool, typed: &str) {
        // To replace text, we need to effectively remove what was there, but it
        // needs to be done without popping.
        let mut words = self.words.iter();
        let mut old_typed = String::new();
        for pos in 0..remove {
            if let Some(word) = words.next_back() {
                let text = if pos + 1 < remove {
                    word.remove.clone()
                } else {
                    String::new()
                };
                self.to_type.push_back(TypeAction {
                    remove: word.typed.len(),
                    text,
                });
                old_typed.push_str(&word.typed);
            }
        }

        let mut text = String::new();
        if space {
            text.push(' ');
        }
        text.push_str(typed);
        self.to_type.push_back(TypeAction {
            remove: 0,
            text: text.clone(),
        });

        self.words.push(Word {
            remove: old_typed,
            typed: text,
        });
    }

    /// Remove the latest thing we typed.
    pub fn remove(&mut self) {
        if let Some(word) = self.words.pop() {
            self.to_type.push_back(TypeAction {
                remove: word.typed.len(),
                text: word.remove,
            });
        }
    }

    /// Retrieve the actions that have resulted from translation.
    pub fn next_action(&mut self) -> Option<TypeAction> {
        self.to_type.pop_front()
    }
}
