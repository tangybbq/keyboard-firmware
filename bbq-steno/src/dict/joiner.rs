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
#[derive(Clone)]
struct State {
    cap: bool,
    space: bool,
}

/// Just the fields from the add action.
struct Add {
    remove: usize,
    append: String,
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
            Action::Undo => todo!(),
            Action::Add { text, strokes } => {
                self.do_add(text, strokes);
            }
        }
    }

    /// Perform an add of additional data.
    fn do_add(&mut self, text: Vec<Replacement>, strokes: usize) {
        println!("do_add: {} {:?}", strokes, text);

        // Figure out how much to delete based on the previous state.
        // remove must be signed because this can go negative at times.
        let mut remove: isize = 0;
        for _ in 1..strokes {
            let elt = self.history.pop_back().unwrap();
            println!("remove: len:{}, remove:{}", elt.append.len(), elt.remove);
            remove += elt.append.len() as isize;
            remove -= elt.remove as isize;
        }
        if remove < 0 {
            println!("Warning negative remove");
            remove = 0;
        }

        let (new, new_state) = self.compute_new(&text);

        // Pop the remove characters.
        for _ in 0..remove {
            self.typed.pop();
        }
        self.typed.push_str(&new);
        println!("Typed: {:?}", self.typed);

        // Push to the history.
        self.history.push_back(Add {
            remove: remove as usize,
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
    fn compute_new(&mut self, text: &[Replacement]) -> (String, State) {
        let mut result = String::new();

        // Get the values from the history.
        let mut state = if let Some(node) = self.history.back() {
            node.state.clone()
        } else {
            // Fake initial state.
            State { cap: true, space: false }
        };

        let mut next_state = State { cap: false, space: false };

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

        (result, next_state)
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
}
