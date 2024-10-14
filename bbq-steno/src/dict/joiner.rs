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
            println!("Warning negative remove");
            remove = 0;
        }

        let (new, new_state) = self.compute_new(&text, strokes);

        // Pop the remove characters.
        let mut removed = String::new();
        for _ in 0..remove {
            removed.push(self.typed.pop().unwrap_or('?'));
        }
        self.typed.push_str(&new);
        // println!("Typed: {:?}", self.typed);

        // Push to the history.
        self.history.push_back(Add {
            remove: remove as usize,
            removed,
            append: new.clone(),
            state: new_state,
        });

        // Push an action.
        self.actions.push_back((self.now, Joined::Type {
            remove: remove as usize,
            append: new,
        }));
    }

    /// Calculate the new text, based on context, state, and where we are in the input.
    fn compute_new(&mut self, text: &[Replacement], strokes: usize) -> (String, State) {
        let mut result = String::new();

        // Go back in history by one less than the number of strokes in this definition.
        let mut state = if let Some(node) = self.history.iter().rev().skip(strokes - 1).next() {
        // let mut state = if let Some(node) = self.history.back() {
            node.state.clone()
        } else {
            // Fake initial state.
            State { cap: true, space: false }
        };
        // println!("compute_new: state: {:?}", state);

        let mut next_state = State { cap: false, space: state.space };

        for elt in text {
            match elt {
                Replacement::Text(t) => {
                    if state.space {
                        result.push(' ');
                        state.space = false;
                    }
                    if state.cap {
                        // Todo, this doesn't do the cap carry through needed.
                        let mut chars = t.as_str().chars();
                        if let Some(first) = chars.next() {
                            // Push the first char. This doesn't handle the case where the uppercase
                            // version ends up as multiple characters.
                            result.push(first.to_uppercase().next().unwrap());
                        }
                        result.push_str(chars.as_str());
                    } else {
                        result.push_str(t);
                    }
                    state.cap = false;
                    next_state.space = true;
                    next_state.cap = false;
                }
                Replacement::DeleteSpace => {
                    // Handle the ambiguity of this occurring at either the beginning or the end.
                    state.space = false;
                    next_state.space = false;
                }
                Replacement::CapNext => next_state.cap = true,
                // TODO: These should all do something here.
                _ => (),
            }
        }
        // println!("   nextstate: {:?}", next_state);

        (result, next_state)
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
