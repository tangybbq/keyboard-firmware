//! Joiner
//!
//! The Joiner is responsible for taking the output of the dictionary translations and joining them
//! together into a sequence of things to be typed on the keyboard.
//!
//! There is an aspect of a delay to this process, which can help keep us from having to type things
//! that we then just backspace over.
//!
//! Each incoming translation comes into the Joiner as a slice of [`Replacement`] values.  The
//! [`Replacement::Text`] is just some text to be typed.  Some others indicate changes to the state
//! between strokes, such as whether a space should be inserted, or if the next should be made
//! capitalized.
//!
//! To make this more complicated, the action from the translation can also be an "undo", which
//! needs to restore the input to the state it was in before that stroke was typed. Undo can be
//! pressed repeatedly, up until a given history length.

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::replacements::Previous;
use crate::Replacement;

use super::lookup::Action;

/// The minimum amount of typed history to keep.
const MIN_TYPED: usize = 256;

/// The maximum amount of typed history.  It will be shortened to [`MIN_TYPED`] when it reaches this
/// value.  This should be larger than MIN_TYPED by at least the length of the longest typed entry
/// in the dictionary.
const MAX_TYPED: usize = MIN_TYPED * 2 + 64;

/// A Joiner to join steno translations together.
pub struct Joiner {
    /// The current time, in ms.
    now: u64,

    // TODO: Consider an ArrayString, although it doesn't have the replace_range.
    /// Typed.  Our internal memory of what has been typed.  This will be truncated, from the
    /// beginning periodically.
    typed: String,

    /// The history.
    ///
    /// Each positive action is added here, to the back.
    history: VecDeque<Add>,

    /// What has been typed.  These have an associated age" when they were created, and can be
    /// retrieved only as long as the age is valid.
    actions: VecDeque<(u64, Joined)>,
}

// Information carried from one stroke to the next.
#[derive(Clone, Debug)]
struct State {
    cap: bool,
    space: bool,
    stitch: bool,
}

/// Just the fields from the add action.
#[derive(Debug)]
struct Add {
    /// How many characters to remove?  Avoids having to count characters in `removed`.
    remove: usize,
    /// The characters that were removed.  Reversed, as we don't need it unless Undo is pressed.
    removed: String,
    /// What new characters to append.
    append: String,
    /// The meta state after this Add.
    state: State,
}

/// The result of the Joiner's calculations.
#[derive(Debug)]
pub enum Joined {
    Type {
        /// How many times to press backspace.
        remove: usize,
        /// Characters to type.
        append: String,
    }
}

/// This describes the states in the process of computing the set of actions based on the
/// translation.
struct Next {
    // How much is being removed by this action.
    remove: usize,
    // Characters actually removed.
    removed: String,
    // Characters to be appended as part of the remove.
    append: String,
    // Tracks the state as we type.
    state: State,
    // What will be the end state after the actions.
    next_state: State,
}

impl Joiner {
    pub fn new() -> Joiner {
        Joiner {
            now: 0,
            typed: String::new(),
            history: VecDeque::new(),
            actions: VecDeque::new(),
        }
    }

    /// Record an incoming stroke.
    pub fn add(&mut self, action: Action) {
        self.shrink();

        match action {
            Action::Undo => self.undo(),
            Action::Add { text, strokes } => {
                self.do_add(text, strokes);
            }
        }
    }

    /// Perform an add of additional data.
    fn do_add(&mut self, text: Vec<Replacement>, strokes: usize) {
        // println!("do_add: {} {:?}", strokes, text);

        // Figure out how much to delete based on the previous state.
        // remove must be signed because this can go negative at times.
        let mut remove: isize = 0;
        let mut tmp = vec![];
        for _ in 1..strokes {
            let elt = self.history.pop_back().unwrap();
            // println!("remove: len:{}, remove:{}", elt.append.len(), elt.remove);
            remove += elt.append.len() as isize;
            remove -= elt.remove as isize;
            tmp.push(elt);
        }

        // Push the history back to the way it was.
        while let Some(h) = tmp.pop() {
            self.history.push_back(h);
        }

        if remove < 0 {
            // println!("Warning negative remove");
            remove = 0;
        }

        // Compute the new state and action based on what is in the definition.
        let mut next = Next::new(self, remove as usize, strokes);

        // Pop the removed characters.
        for _ in 0..remove {
            next.removed.push(self.typed.pop().unwrap_or('?'));
        }

        // Handle each incoming operation.
        for elt in &text {
            next.add_replacement(self, elt);
        }

        // Perform the action computed above.

        self.typed.push_str(&next.append);
        // println!("Typed: {:?}", self.typed);

        // Push to the history.
        self.history.push_back(Add {
            remove: next.remove,
            removed: next.removed,
            append: next.append.clone(),
            state: next.next_state,
        });

        // Push an action.
        self.actions.push_back((self.now, Joined::Type {
            remove: next.remove,
            append: next.append,
        }));
    }

    fn undo(&mut self) {
        if let Some(add) = self.history.pop_back() {
            // Reverse, as it was pulled out in reverse order.
            let removed: String = add.removed.chars().rev().collect();

            // Remove what was appended.  These are just discarded as undo is permanent.
            // This needs to count codepoints, not bytes.
            for _ in 0..add.append.chars().count() {
                self.typed.pop();
            }

            // Add the removed characters to what is typed.
            self.typed.push_str(&removed);

            // Synthesize an action for this.
            self.actions.push_back((self.now, Joined::Type {
                remove: add.append.len(),
                append: removed,
            }));

            // println!("Typed: {:?}", self.typed);
        }
    }

    /// Retrieve an action, if there is one whose age is sufficiently old.
    /// TODO: Use the age.
    pub fn pop(&mut self, _min_age: u64) -> Option<Joined> {
        if let Some((_age, act)) = self.actions.pop_front() {
            Some(act)
        } else {
            None
        }
    }

    /// Shrink the history down enough so any additional can be added.
    fn shrink(&mut self) {
        // This is actually a little messy, because of Unicode.  We'll avoid the length calculation
        // unless it needs to be shrunk.
        if self.typed.len() >= MAX_TYPED {
            let len = self.typed.chars().count();
            if len > MIN_TYPED {
                self.typed = self.typed.chars().skip(len - MAX_TYPED).collect();
            } else {
                // This should warn somehow that we're running the count loop excessively.  As long
                // as max is sufficiently larger than min, this shouldn't ever occur.
            }
            self.typed.replace_range(0..(self.typed.len() - MIN_TYPED), "");
        }
    }

    #[cfg(feature = "std")]
    pub fn show(&self) {
        println!("--- state ---");
        println!("Typed: {:?}", self.typed);
        println!("history: [");
        for h in &self.history {
            println!("  {:?}", h);
        }
        // println!("actions: {:?}", self.actions);
        println!("--- end state ---");
    }
}

impl Next {
    fn new(joiner: &mut Joiner, remove: usize, strokes: usize) -> Next {
        // Go back in history, one less than the number of strokes in this definition to get our
        // starting state.
        let state = if let Some(node) = joiner.history.iter().rev().skip(strokes - 1).next() {
            node.state.clone()
        } else {
            // Fake an initial state.  Shouldn't happen unless we back up over the history.
            State { cap: true, space: false, stitch: false }
        };

        let next_state = State { cap: false, space: state.space, stitch: false };

        Next {
            remove,
            removed: String::new(),
            append: String::new(),
            state,
            next_state,
        }
    }

    /// Add a replacement to the current state.
    fn add_replacement(&mut self, joiner: &mut Joiner, text: &Replacement) {
        match text {
            Replacement::Text(t) => {
                if self.state.space && (!self.state.stitch || !self.next_state.stitch) {
                    self.append.push(' ');
                    self.state.space = false;
                }
                if self.state.cap {
                    // TODO: This doesn't do the cap carry through needed.
                    let mut chars = t.as_str().chars();
                    if let Some(first) = chars.next() {
                        // TODO: This doesn't handle unicode caps that are more than one character.
                        self.append.push(first.to_uppercase().next().unwrap());
                    }
                    self.append.push_str(chars.as_str());
                } else {
                    self.append.push_str(t);
                }
                self.state.cap = false;
                self.next_state.space = true;
                self.next_state.cap = false;
            }
            Replacement::DeleteSpace => {
                // Handle the ambiguity of this occurring at either the beginning or end.
                self.state.space = false;
                self.next_state.space = false;
            }
            Replacement::CapNext => self.next_state.cap = true,
            Replacement::Stitch => self.next_state.stitch = true,

            // Capitalize the previous 'n' words.
            Replacement::Previous(n, Previous::Capitalize) => {
                let mut buf = String::new();

                let mut word_count = 1;

                // Pop characters until we find 'n' word boundaries.  We assume that we're inside of
                // a word when we start, what are word boundards needs to be better determined.
                while let Some(ch) = joiner.typed.pop() {
                    // For now, just consider space to be word boundaries.
                    if ch != ' ' {
                        buf.push(ch);
                        self.removed.push(ch);
                        self.remove += 1;
                    } else {
                        if word_count == *n {
                            // Done, put the character back.  And finish.
                            joiner.typed.push(ch);
                            break;
                        } else {
                            // Otherwise, we got another word boundary.
                            buf.push(ch);
                            self.removed.push(ch);
                            self.remove += 1;
                            word_count += 1;
                        }
                    }
                }

                // Now, retype the text, but with capitalization.
                let mut word_start = true;
                while let Some(ch) = buf.pop() {
                    if word_start {
                        // TODO: Handle multiple characters from this.
                        self.append.push(ch.to_uppercase().next().unwrap());
                        word_start = false;
                    } else {
                        self.append.push(ch);
                    }
                    if ch == ' ' {
                        word_start = true;
                    }
                }
            }

            _ => {
                eprintln!("Act: {:?}", text);
            }
        }
    }
}
