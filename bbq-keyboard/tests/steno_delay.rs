//! Tests for the steno delay buffer.
//!
//! [`StenoDelay`] holds dictionary output for [`DELAY_MS`] before it may be typed, so that a
//! following stroke's corrections consume still-pending text instead of being typed and then
//! visibly backspaced away.  These tests drive it directly, with time supplied as plain
//! milliseconds.

use bbq_keyboard::steno_delay::{StenoDelay, DELAY_MS};
use bbq_steno::dict::Joined;

/// Build a dictionary result.
fn typed(remove: usize, append: &str) -> Joined {
    Joined::Type {
        remove,
        append: append.into(),
    }
}

/// What a drain should have produced.
fn expect(remove: usize, append: &str) -> Option<Joined> {
    Some(typed(remove, append))
}

#[test]
fn delay_is_the_documented_half_second() {
    assert_eq!(DELAY_MS, 500);
}

#[test]
fn nothing_pending() {
    let mut delay = StenoDelay::new();
    assert_eq!(delay.next_deadline(), None);
    assert_eq!(delay.take_ready(0), None);
    assert_eq!(delay.take_ready(100_000), None);
    assert_eq!(delay.take_all(), None);
}

#[test]
fn single_entry_types_once_at_its_deadline() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "hello"), 0);

    assert_eq!(delay.next_deadline(), Some(500));
    assert_eq!(delay.take_ready(499), None);
    assert_eq!(delay.take_ready(500), expect(0, "hello"));

    // And exactly once.
    assert_eq!(delay.take_ready(500), None);
    assert_eq!(delay.take_ready(5000), None);
    assert_eq!(delay.next_deadline(), None);
}

/// The motivating case: a suffix stroke revises the base word before it was ever typed, so no
/// backspaces reach the host at all.  Net result is "testing".
#[test]
fn suffix_correction_within_the_window() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "test"), 0);
    delay.push(typed(2, "sting"), 300);

    assert_eq!(delay.take_ready(500), expect(0, "te"));
    // Consuming part of an entry does not push out the survivor's deadline.
    assert_eq!(delay.next_deadline(), Some(800));
    assert_eq!(delay.take_ready(800), expect(0, "sting"));
    assert_eq!(delay.next_deadline(), None);
}

/// Written slowly enough that the base word has already gone out, the correction still needs real
/// backspaces, exactly as the undelayed firmware did.
#[test]
fn correction_after_the_text_has_flushed() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "test"), 0);
    assert_eq!(delay.take_ready(500), expect(0, "test"));

    delay.push(typed(2, "sting"), 600);
    assert_eq!(delay.take_ready(1100), expect(2, "sting"));
}

/// A large correction (an `*` undo, say) cancels several pending entries at once.
#[test]
fn large_correction_cancels_several_entries() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "hello"), 0);
    delay.push(typed(2, "p"), 100);
    // Pending is now "hel" + "p", four characters, none of it typed.

    // Remove ten: four come out of the buffer, six are real backspaces.
    delay.push(typed(10, "bye"), 200);
    assert_eq!(delay.next_deadline(), Some(700));
    assert_eq!(delay.take_ready(700), expect(6, "bye"));
}

/// A cancelled entry's own backspaces were aimed at text before it, so they have to survive the
/// entry that carried them.
#[test]
fn cancelled_entry_carries_its_own_removal() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "abc"), 0);
    assert_eq!(delay.take_ready(500), expect(0, "abc"));

    // Buffer is empty, so this removal is real backspacing, held pending with its text.
    delay.push(typed(2, "xy"), 600);

    // Cancel it: two of the five removals eat "xy", and the entry's own two backspaces are added
    // back to the three that are left over.
    delay.push(typed(5, "z"), 700);
    assert_eq!(delay.take_ready(1200), expect(5, "z"));
}

#[test]
fn removal_larger_than_everything_pending() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "ab"), 0);
    delay.push(typed(0, "cd"), 10);

    delay.push(typed(7, "new"), 20);
    assert_eq!(delay.take_all(), expect(3, "new"));
}

/// A removal that exactly consumes the pending text leaves nothing behind but the new text.
#[test]
fn removal_exactly_consumes_pending_text() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "ab"), 0);
    delay.push(typed(2, "xyz"), 10);

    assert_eq!(delay.take_all(), expect(0, "xyz"));
}

/// A removal that cancels everything and appends nothing leaves the buffer empty rather than an
/// entry that types nothing.
#[test]
fn fully_cancelling_removal_empties_the_buffer() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "abc"), 0);
    delay.push(typed(3, ""), 100);

    assert_eq!(delay.next_deadline(), None);
    assert_eq!(delay.take_all(), None);
}

#[test]
fn take_all_ignores_deadlines_and_empties_the_buffer() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "one "), 0);
    delay.push(typed(0, "two "), 100);
    delay.push(typed(0, "three"), 200);

    assert_eq!(delay.take_all(), expect(0, "one two three"));
    assert_eq!(delay.next_deadline(), None);
    assert_eq!(delay.take_all(), None);
}

#[test]
fn take_ready_merges_only_what_is_due() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "one "), 0);
    delay.push(typed(0, "two "), 100);
    delay.push(typed(0, "three"), 900);

    // The first two are due at 500 and 600; the third not until 1400.
    assert_eq!(delay.take_ready(700), expect(0, "one two "));
    assert_eq!(delay.next_deadline(), Some(1400));
    assert_eq!(delay.take_ready(700), None);
    assert_eq!(delay.take_ready(1400), expect(0, "three"));
}

/// Backspace removes a character, not a byte.
#[test]
fn removal_counts_characters_not_bytes() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "café"), 0);
    delay.push(typed(1, "es"), 100);

    assert_eq!(delay.take_ready(500), expect(0, "caf"));
    assert_eq!(delay.take_ready(600), expect(0, "es"));
}

#[test]
fn multibyte_entry_cancelled_whole() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "naïve"), 0);
    // Five characters, six bytes: removing five takes the entry and no more.
    delay.push(typed(5, "simple"), 100);

    assert_eq!(delay.take_all(), expect(0, "simple"));
}

#[test]
fn multibyte_removal_spilling_past_the_buffer() {
    let mut delay = StenoDelay::new();
    delay.push(typed(0, "über"), 0);
    // Four characters pending, so two of the six removals are real.
    delay.push(typed(6, "x"), 100);

    assert_eq!(delay.take_all(), expect(2, "x"));
}

/// Only the front entry may carry a removal: a leftover can only arise once the buffer has been
/// emptied, which makes the new entry the front one.  Observably, draining a buffer one deadline
/// at a time may only produce backspaces on the very first drain.
#[test]
fn only_the_front_entry_carries_a_removal() {
    // A scripted mix: plain text, partial corrections, whole-entry cancellations, and removals
    // that spill past everything pending.
    let script: &[(usize, &str)] = &[
        (0, "test"),
        (2, "sting"),
        (0, " is"),
        (3, "was"),
        (9, "hello"),
        (0, " there"),
        (11, "hi"),
        (4, "greetings"),
        (0, "!"),
    ];

    let mut delay = StenoDelay::new();
    for (step, (remove, append)) in script.iter().enumerate() {
        delay.push(typed(*remove, append), step as u64 * 10);
    }

    let mut drains = 0;
    while let Some(deadline) = delay.next_deadline() {
        let action = delay.take_ready(deadline).expect("entry at its own deadline");
        let Joined::Type { remove, .. } = action;
        if drains > 0 {
            assert_eq!(remove, 0, "drain {drains} carried a removal");
        }
        drains += 1;
    }
    assert!(drains > 1, "the script should leave several entries pending");
}

/// Whatever the buffer does internally, the text the host ends up with has to match what typing
/// each action immediately would have produced.
#[test]
fn net_effect_matches_undelayed_typing() {
    let script: &[(usize, &str)] = &[
        (0, "test"),
        (2, "sting"),
        (0, " is"),
        (3, "was"),
        (9, "hello"),
        (0, " there"),
        (11, "hi"),
        (4, "greetings"),
        (0, "!"),
    ];

    // Type everything immediately, with plenty of leading text for backspaces to eat into.
    let prefix = "..........";
    let mut immediate = String::from(prefix);
    for (remove, append) in script {
        for _ in 0..*remove {
            immediate.pop();
        }
        immediate.push_str(append);
    }

    // Now the same script through the buffer, flushing only at the very end.
    let mut delayed = String::from(prefix);
    let mut delay = StenoDelay::new();
    for (step, (remove, append)) in script.iter().enumerate() {
        delay.push(typed(*remove, append), step as u64 * 10);
    }
    if let Some(Joined::Type { remove, append }) = delay.take_all() {
        for _ in 0..remove {
            delayed.pop();
        }
        delayed.push_str(&append);
    }

    assert_eq!(delayed, immediate);
}
