//! Delayed typing of steno translations.
//!
//! The steno dictionary frequently revises what it has already produced: adding a suffix to a word
//! may require removing or doubling letters of the base word.  Typed immediately, this shows up on
//! the host as text followed by visible backspacing, which looks odd and misbehaves outright in
//! places that treat backspace specially (browser address bar completion, for example).
//!
//! This module buffers each [`Joined`] for a fixed [`DELAY_MS`] before it may be typed.  A later
//! stroke's corrections silently consume text that is still pending, so fluent writing produces
//! only the final text.  Text older than the delay has already gone out, and is still corrected
//! with real backspaces, exactly as before.
//!
//! The logic here is pure: time is passed in as milliseconds, so it can be tested on the host.

use alloc::collections::VecDeque;
use alloc::string::String;

use bbq_steno::dict::Joined;

/// How long, in milliseconds, output is held before it is typed.
pub const DELAY_MS: u64 = 500;

/// A chunk of output that hasn't been typed yet.
struct Entry {
    /// The time at which this entry may be typed.
    deadline: u64,
    /// Real backspaces to send before `append`, against text already typed.
    remove: usize,
    /// The characters to type.
    append: String,
}

impl Entry {
    fn is_empty(&self) -> bool {
        self.remove == 0 && self.append.is_empty()
    }
}

/// A buffer of steno output waiting to be typed.
///
/// Entries are held in deadline order.  Only the front entry can ever have a nonzero `remove`: a
/// leftover removal can only arise once everything pending has been consumed.
pub struct StenoDelay {
    pending: VecDeque<Entry>,
}

impl StenoDelay {
    pub fn new() -> Self {
        StenoDelay {
            pending: VecDeque::new(),
        }
    }

    /// Buffer a new dictionary result, produced at time `now`.
    ///
    /// Removals are applied to still-pending text first, newest first, and only what is left over
    /// becomes real backspaces.
    pub fn push(&mut self, action: Joined, now: u64) {
        let Joined::Type {
            mut remove,
            append,
        } = action;

        while remove > 0 {
            let Some(back) = self.pending.back_mut() else {
                break;
            };
            let len = back.append.chars().count();
            if remove < len {
                truncate_chars(&mut back.append, remove);
                remove = 0;
            } else {
                // This entry is cancelled entirely before it was ever typed.  Its own backspaces
                // were aimed at text before it, so they carry over to what remains.
                let entry = self.pending.pop_back().unwrap();
                remove = remove - len + entry.remove;
            }
        }

        // An entry that lost all of its text and has no backspaces of its own is nothing at all.
        if self.pending.back().is_some_and(Entry::is_empty) {
            self.pending.pop_back();
        }

        if remove > 0 || !append.is_empty() {
            self.pending.push_back(Entry {
                deadline: now + DELAY_MS,
                remove,
                append,
            });
        }
    }

    /// The earliest deadline of anything pending, for the firmware's timer.
    pub fn next_deadline(&self) -> Option<u64> {
        self.pending.front().map(|entry| entry.deadline)
    }

    /// Remove and merge everything whose deadline has passed.
    pub fn take_ready(&mut self, now: u64) -> Option<Joined> {
        self.take(|entry| entry.deadline <= now)
    }

    /// Remove and merge everything pending, regardless of deadline.
    ///
    /// Used when leaving steno mode, so nothing lingers while typing in another layout.
    pub fn take_all(&mut self) -> Option<Joined> {
        self.take(|_| true)
    }

    /// Drain entries from the front for as long as `wanted` holds, merging them into a single
    /// action.
    fn take(&mut self, wanted: impl Fn(&Entry) -> bool) -> Option<Joined> {
        if !self.pending.front().is_some_and(&wanted) {
            return None;
        }

        let first = self.pending.pop_front().unwrap();
        let remove = first.remove;
        let mut append = first.append;

        while self.pending.front().is_some_and(&wanted) {
            let entry = self.pending.pop_front().unwrap();
            // Only the front entry can carry a removal, so merging is just concatenation.
            debug_assert_eq!(entry.remove, 0);
            append.push_str(&entry.append);
        }

        Some(Joined::Type { remove, append })
    }
}

impl Default for StenoDelay {
    fn default() -> Self {
        StenoDelay::new()
    }
}

/// Remove the last `count` characters from `text`.
fn truncate_chars(text: &mut String, count: usize) {
    if count == 0 {
        return;
    }
    match text.char_indices().nth_back(count - 1) {
        Some((offset, _)) => text.truncate(offset),
        None => text.clear(),
    }
}
