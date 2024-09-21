//! The Typer
//!
//! The "typer" is one of two key parts of the using Steno to write text and/or
//! code (the other being the dictionary). The translations from the dictionary
//! are fed into the typer, and the typer generates the keystrokes needed to
//! simulate typing of this information. In addition to this basic
//! functionality, there are other features:
//!
//! * Handling rewrites. Sometimes multi-stroke entries will have other entries
//! with shorter definitions. This results in text being typed that may need to
//! be removed and replaced with a better translation.
//!
//! * The undo key. The steno keyboard has an undo key (by default, just '*' by
//! itself) that is used to undo the previously written stroke. It can be
//! pressed multiple times to undo several levels. New text can be written at
//! that point. It also makes sense to have a "redo" to rewrite what was undone,
//! although I have not seen this implemented.
//!
//! * Delayed typing. Because an incremental dictionary lookup will result in
//! false starts, the type of what is written can be given a small delay. The
//! false starts will then not be typed and then removed, but the correct text
//! written after a short delay. Even a very short delay is useful to avoid
//! things like removing an entire word just to retype it with a different
//! ending.
//!
//! * Capitalization. Some translations will contain directives to indicate that
//! the following text will be capitalized. This has to follow the rules as undo
//! operations are performed. In addition, there are strokes to affect
//! capitalization and spacing retroactively. These are also entries that can be
//! undone, undoing the effect of the rewrite.
//!
//! * Spacing. In addition, most entries from the dictionary will have spaces
//! automatically placed between them. However, there are directives to affect
//! this spacing in various ways, including retroactively. Again, these commands
//! will have full support for both being undone, and/or replaced by upcoming
//! strokes.
//!
//! * Raw keystrokes. Some entries will trigger keypress directly. This can be
//! used to emulate a regular keyboard, but done in the midst of a steno
//! operation. Generally, direct keystrokes will interrupt the undo sequence,
//! since there isn't a full way to know how to undo the behavior of direct
//! keystrokes, especially when using something like a modal editor, where the
//! strokes perform commands rather than just typing themselves.
//!
//! It is important that raw keystrokes do not have entries with differing
//! numbers of strokes. Since these cannot be undone or replaced, if there are
//! entries that have raw keystrokes, and longer entries with that same prefix,
//! those keypresses cannot be removed.
//!
//! All of this functionality requires some complexity to implement. All of this
//! state will be kept in a struct, with possible future support for multiple
//! contexts, where an app running on the computer could inform the keyboard
//! about changes to the keyboard focus, and separate states could be maintained
//! for each of these.
//!
//! Most of this functionality is entirely local to the Typer. However, undo is
//! managed by the translation, because it has to be able to return to earlier
//! states. The information coming from the translator will consist of stuff to
//! be typed, and either "undo", or "replace" commands. The replace command
//! replaces something previously written with something else, whereas undo
//! removes it entirely. The replacements are kept in the history, and can be
//! undone, reverting to the previous state before the stroke before the undo.
//!
//! ## Replacement rules
//!
//! Inserted text can consist of plain text, which will be inserted directly as
//! is (using shift as needed for symbols and capital letters). The state can
//! result in there possibly being a space inserted before the text, and the
//! initial text may be capitalized.
//!
//! The following control codes control various aspects of how text is stitched.
//! This is intended to be compact, as these dictionaries are stored in
//! microcontroller flash.
//!
//! '\x01' - suppress the next space. Spaces are inserted automatically, but
//! this can also be used inside a translation (although it is unclear why one
//! would do this).
//!
//! '\x02' - Capitalize. The following character indicates the context of the
//! capitalization, with a '-' indicating the next word should be capitalized,
//! and the digits 1-9 indicating that the previous number of words should be.
//!
//! '\x03' - Stitch. This should be joined with other things that are also
//! marked as stitched.
//!
//! ...

extern crate alloc;

use alloc::collections::VecDeque;
use alloc::string::String;

/// A TypeHandler accepts these items from the translator, handling them
/// eventually through the keyboard.
pub trait TypeHandler {
    /// Add a sequence of text, replacing zero or more previously added entries.
    /// The replacement is a count of the number of previous calls to `add`
    /// whose text will be replaced. `text` should follow the inserted text
    /// rules described in the module documentation.
    fn add(&mut self, text: &str, replace: usize);

    /// Remove the last thing inserted with `add`. If that add was a
    /// replacement, the earlier added things that were removed due to the
    /// replacement will be put back.
    fn undo(&mut self);
}

/// The TypeOutput is where output from the typer goes. A given typer will have
/// one of these to be able to type. This could be a USB HID device, or mocking
/// for unit testing.
pub trait TypeOutput {
    /// Type the given text.
    fn text(&mut self, text: &str);

    /// Backspace the given number of characters.
    fn backspace(&mut self, count: usize);

    /// Generate the given raw keystroke. For now, this is just a string, but we
    /// will figure out a better encoding.
    fn raw(&mut self, info: &str);
}

/// The Typer implements the handler, and places output in a given TypeOutput.
pub struct Typer<O: TypeOutput> {
    /// Where we are outputting.
    output: O,

    /// The history of generated text.
    history: VecDeque<char>,
}

impl<O: TypeOutput> Typer<O> {
    pub fn new(output: O) -> Typer<O> {
        Typer {
            output,
            history: VecDeque::new(),
        }
    }

    pub fn output_ref(&self) -> &O {
        &self.output
    }
}

impl<O: TypeOutput> TypeHandler for Typer<O> {
    fn add(&mut self, text: &str, replace: usize) {
        if replace > 0 {
            todo!("Replace");
        }
        for ch in text.chars() {
            self.history.push_back(ch);
        }

        // For now, just drain this all.
        let buf: String = self.history.drain(..).collect();
        self.output.text(&buf);
    }

    fn undo(&mut self) {
    }
}

#[cfg(test)]
mod test {
    use super::{TypeHandler, TypeOutput, Typer};

    // A sync absorbs output characters.
    struct Sync(String);

    impl Sync {
        pub fn new() -> Sync {
            Sync(String::new())
        }
    }

    impl TypeOutput for Sync {
        fn text(&mut self, text: &str) {
            self.0.push_str(text);
        }

        fn backspace(&mut self, count: usize) {
            for _ in 0..count {
                self.0.pop();
            }
        }

        fn raw(&mut self, _info: &str) {
            unimplemented!()
        }
    }

    #[test]
    fn basic_typer() {
        let mut hold = Typer::new(Sync::new());
        assert_eq!(hold.output_ref().0, "");
        hold.add("hello", 0);
        assert_eq!(hold.output_ref().0, "hello");
        hold.add("world", 0);
        assert_eq!(hold.output_ref().0, "helloworld");
    }
}
