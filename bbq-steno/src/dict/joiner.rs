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
//
// The difference between space and force space is that if a "space" is next to a non "space"
// stroke, there won't be a space, whereas either having "force space" will result in a space.
#[derive(Clone, Debug)]
struct State {
    cap: bool,
    space: bool,
    force_space: bool,
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
            State { cap: true, space: false, force_space: false, stitch: false }
        };

        // Carry the cap through, which we will remove, once we actually capitalize something.
        let next_state = State { cap: state.cap, space: state.space, force_space: false, stitch: false };

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
                if (self.state.space && (!self.state.stitch || !self.next_state.stitch)) ||
                    (self.state.force_space || self.next_state.force_space)
                {
                    self.append.push(' ');
                    self.state.space = false;
                    self.state.force_space = false;
                    self.next_state.force_space = false;
                }
                for ch in t.as_str().chars() {
                    // If capitalization is expected, and the next character is alphabetic, consider
                    // it capitalized (even if it is already capitalized).  This can carry through,
                    // even across multiple strokes until something is actually affected by the caps.
                    // We also want to stop after numeric things, to catch sentences that start with
                    // a number, and avoid capitalizing the word after that.
                    if self.state.cap && ch.is_alphanumeric() {
                        // TODO: This doesn't handle unicode caps that are more than one character.
                        self.append.push(ch.to_uppercase().next().unwrap());

                        // Now that we've capitalized something, we can stop.
                        self.state.cap = false;
                        self.next_state.cap = false;
                    } else {
                        self.append.push(ch);
                    }
                }
                self.next_state.space = true;
            }
            Replacement::DeleteSpace => {
                // Handle the ambiguity of this occurring at either the beginning or end.
                self.state.space = false;
                self.next_state.space = false;
            }
            Replacement::CapNext => self.next_state.cap = true,
            Replacement::ForceSpace => {
                // Same ambiguity as above.
                self.state.force_space = true;
                self.next_state.force_space = true;
            }
            Replacement::Stitch => self.next_state.stitch = true,

            // Capitalize the previous 'n' words.
            Replacement::Previous(n, Previous::Capitalize) => {
                self.fix_prior_words(joiner, *n as usize, CapMode::Title);
            }
            Replacement::Previous(n, Previous::Lowerize) => {
                self.fix_prior_words(joiner, *n as usize, CapMode::Lower);
            }
            Replacement::Previous(n, Previous::Upcase) => {
                self.fix_prior_words(joiner, *n as usize, CapMode::Upper);
            }
            // I'm not clear what other options there are, this seems to just be this 10:$ entry.
            Replacement::Previous(1, Previous::Currency(_)) => {
                self.fix_currency(joiner);
            }

            Replacement::Previous(n, Previous::ReplaceSpace(ch)) => {
                self.replace_spaces(joiner, *n as usize,
                                    if *ch == '\0' { None } else { Some(*ch) });
            }

            _ => {
                #[cfg(feature = "std")]
                {
                    eprintln!("Act: {:?}", text);
                }
            }
        }
    }

    /// Update previous 'n' words, appropriately.
    ///
    /// Currently, this views "words" as things separated by spaces, so will be confused by
    /// punctuation.
    fn fix_prior_words(&mut self, joiner: &mut Joiner, words: usize, mode: CapMode) {
        // Holds characters, in reverse order, as we pop them off of typed.
        let mut buf = String::new();
        let mut walk = WordWalk::new();

        while let Some(ch) = joiner.typed.pop() {
            walk.visit(ch);

            if walk.done(words) {
                // Done, push this character back, and act as if we didn't see it.
                joiner.typed.push(ch);
                break;
            } else {
                // Otherwise, push to our buffer and move on.
                buf.push(ch);
                self.removed.push(ch);
                self.remove += 1;
            }
        }

        // Now, retype the text, but with the indicated change.
        mode.convert(&mut buf, &mut self.append);
    }

    /// Replace the previous 'count' spaces with the given replacement.
    fn replace_spaces(&mut self, joiner: &mut Joiner, count: usize, replacement: Option<char>) {
        let mut buf = String::new();
        let mut seen = 0;

        while let Some(ch) = joiner.typed.pop() {
            buf.push(ch);
            self.removed.push(ch);
            self.remove += 1;

            if ch == ' ' {
                seen += 1;
                if seen == count {
                    break;
                }
            }
        }

        // Retype, inserting all back into the input.
        while let Some(ch) = buf.pop() {
            if ch == ' ' {
                if let Some(repl) = replacement {
                    self.append.push(repl);
                }
            } else {
                self.append.push(ch);
            }
        }
    }

    /// Add a dollar sign in front of the previous amount of currency.  There are we're looking for
    /// a sequence of digits to put this in front of.  The examples allow the word "million" and
    /// "billion" to occur, but, frankly, I see little reason to actually use this, and find it is
    /// probably easier to not even bother with the whole retro currency thing.
    fn fix_currency(&mut self, joiner: &mut Joiner) {
        let mut buf = String::new();

        // Pop characters as long as they are digits.
        while let Some(ch) = joiner.typed.pop() {
            if ch.is_digit(10) || ch == ',' || ch == '.' {
                buf.push(ch);
                self.removed.push(ch);
                self.remove += 1;
            } else {
                joiner.typed.push(ch);
                break;
            }
        }

        // Add the dollar sign.
        self.append.push('$');
        while let Some(ch) = buf.pop() {
            self.append.push(ch);
        }
    }
}

/// Capitalization modes.
#[derive(Debug, Clone, Copy)]
enum CapMode {
    /// The entire word should be in uppercase.
    Upper,
    /// The entire word should be in lowercase.
    Lower,
    /// The first letter should be uppercase, with the rest lowercase.
    Title,
}

impl CapMode {
    /// Convert a reversed set of words unto the given CapMode.
    ///
    /// Source is a reversed set of characters containing the words to operate on.  The result will
    /// be pushed to dest.
    fn convert(self, src: &mut String, dest: &mut String) {
        let mut walk = WordWalk::new();

        while let Some(ch) = src.pop() {
            walk.visit(ch);
            if walk.is_in_word() {
                match (walk.is_first(), self) {
                    // Capitalize the next, if we're at the start, asking for either Upper or Title,
                    // or we're in the word, and doing full Upper.
                    (true, CapMode::Upper) |
                        (true, CapMode::Title) |
                        (false, CapMode::Upper) =>
                    {
                        for ch in ch.to_uppercase() {
                            dest.push(ch);
                        }
                    }
                    // Lowerize if we're asking for that.
                    (_, CapMode::Lower) => {
                        for ch in ch.to_lowercase() {
                            dest.push(ch);
                        }
                    }
                    // The inner characters of Title case are left alone.  This changes deSantos to
                    // DeSantos, not Desantos.
                    (false, CapMode::Title) => dest.push(ch),
                }
            } else {
                // Not in word, just pushes it and we continue.
                dest.push(ch);
            }
        }
    }
}

/// Track word boundaries when walking text one character at a time.
///
/// Tracks the number of words visited, as well as whether we are at the start of a word, inside of
/// a word, or between words.
///
/// To this, a word is defined as a sequence of alphanumeric characters separated by
/// non-alphanumeric characters.
///
/// This can be started either in a word or not, and `word` will become 1 at the first character of
/// the first word.
///
/// This is designed to be run in either direction, although "first" will actually mean "last" when
/// run in reverse.
struct WordWalk {
    /// Are we inside of a word.
    in_word: bool,
    /// Is this the first character of a word, also first character of a non-word.
    first: bool,
    /// What word are we on.  Counts the first word as 1.
    word: usize,
}

impl WordWalk {
    /// Construct a new word walker, ready to look at characters.
    fn new() -> WordWalk {
        WordWalk {
            in_word: false,
            first: true,
            word: 0,
        }
    }

    /// Indicate a character visited, updating the state.
    fn visit(&mut self, ch: char) {
        let ch_in_word = ch.is_alphanumeric();

        if ch_in_word != self.in_word {
            self.in_word = ch_in_word;
            self.first = true;
            if ch_in_word {
                self.word += 1;
            }
        } else {
            self.first = false;
        }
    }

    /// Is this a first word character?
    fn is_first(&self) -> bool {
        self.in_word && self.first
    }

    /// Are we in a word?
    fn is_in_word(&self) -> bool {
        self.in_word
    }

    /// Are we done with the given number of words.  Will be true on the first non-word character
    /// after seeing 'n' words.
    fn done(&self, count: usize) -> bool {
        !self.in_word && self.word == count
    }
}
