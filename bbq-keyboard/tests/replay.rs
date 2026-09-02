//! Tests for the key event log replay.
//!
//! These drive [`bbq_keyboard::replay`] with hand-built logs and check the
//! derived stream, which is the thing phase 3's analysis and phase 4's golden
//! files are built on.  `tests/taipo.rs` tests the layout engine itself; this
//! tests what the replay makes of it.

#![cfg(all(feature = "proto3", feature = "std"))]

use bbq_keyboard::layout::taipo::{ChordEnd, TaipoVariant};
use bbq_keyboard::layout::TAIPO_CHORD_TIME;
use bbq_keyboard::replay::{
    chords, key_for_name, log_from_text, log_to_text, replay, replay_sessions, sessions_from_text,
    split_chords, ChordAction, Chord, Derived, KeyLogEvent,
};
use bbq_keyboard::{Keyboard, LayoutMode, Mods, Side};

/// A log under construction, so that a test reads as a sequence of things the
/// writer did rather than as a list of timestamps.
#[derive(Default)]
struct Log {
    events: Vec<KeyLogEvent>,
    now: u32,
}

impl Log {
    fn new() -> Log {
        Log::default()
    }

    /// Let this many milliseconds pass.
    fn wait(&mut self, ms: u32) -> &mut Self {
        self.now += ms;
        self
    }

    fn press(&mut self, name: &str) -> &mut Self {
        self.at(name, true)
    }

    fn release(&mut self, name: &str) -> &mut Self {
        self.at(name, false)
    }

    fn at(&mut self, name: &str, press: bool) -> &mut Self {
        let key = key_for_name(name).unwrap_or_else(|| panic!("no key called {name:?}"));
        self.events.push(KeyLogEvent {
            time_ms: self.now,
            key,
            press,
        });
        self
    }

    /// Strike every key of a chord together and let go `hold` later.
    fn tap(&mut self, hand: char, keys: &str, hold: u32) -> &mut Self {
        for key in keys.split('+') {
            self.press(&format!("{hand}.{key}"));
        }
        self.wait(hold);
        for key in keys.split('+') {
            self.release(&format!("{hand}.{key}"));
        }
        self
    }

    fn run(&self) -> Vec<Derived> {
        replay(false, &self.events)
    }

    /// Replay as a two-row board (mesa1, proto4), which comes up in taipo and
    /// has no row toggle.
    fn run_two_row(&self) -> Vec<Derived> {
        replay(true, &self.events)
    }
}

/// The chords out of a run, owned so the caller isn't holding a borrow.
fn chord_list(derived: &[Derived]) -> Vec<Chord> {
    chords(derived).into_iter().cloned().collect()
}

/// The key actions out of a run.
fn key_actions(derived: &[Derived]) -> Vec<String> {
    derived
        .iter()
        .filter_map(|event| match event {
            Derived::Key { action, .. } => Some(format!("{action:?}")),
            _ => None,
        })
        .collect()
}

//////////////////////////////////////////////////////////////////////////////
// The log format
//////////////////////////////////////////////////////////////////////////////

/// Every key code round trips through its name, and the taipo keys get real
/// names rather than numbers.
#[test]
fn test_key_names_round_trip() {
    for key in 0..48u8 {
        let name = bbq_keyboard::replay::key_name(key);
        assert_eq!(
            bbq_keyboard::replay::key_for_name(&name),
            Some(key),
            "key {key} named {name:?}"
        );
    }
    assert_eq!(bbq_keyboard::replay::key_name(5), "L.a");
    assert_eq!(bbq_keyboard::replay::key_name(43), "R.Sp");
    // A key with no taipo meaning still has to survive the trip.
    assert_eq!(bbq_keyboard::replay::key_name(0), "k0");
}

/// A whole log round trips through the text format, and comments and blank
/// lines are ignored.
#[test]
fn test_log_text_round_trip() {
    let mut log = Log::new();
    log.tap('L', "a", 20).wait(50).tap('R', "e+i", 30);

    let text = log_to_text(&log.events);
    assert_eq!(log_from_text(&text).unwrap(), log.events);

    let commented = format!("# a session header\n\n{text}   # trailing\n");
    assert_eq!(log_from_text(&commented).unwrap(), log.events);

    assert!(log_from_text("12 ? L.a").is_err());
    assert!(log_from_text("12 + L.nope").is_err());
    assert!(log_from_text("hello + L.a").is_err());
}

/// A file holding several sessions is split at the lines that break the timeline.
///
/// This is what a real log looks like: the collector appends to one file per day, and each
/// time it connects it starts counting from zero again.  Read as one stream those offsets
/// step backwards, which the replay refuses; read as sessions each one is in order.
#[test]
fn test_sessions_split_at_a_timeline_break() {
    let mut first = Log::new();
    first.tap('L', "a", 20).wait(50).tap('R', "e+i", 30);
    let mut second = Log::new();
    second.tap('R', "t", 20).wait(40).tap('L', "o", 25);

    for breaker in [
        "# session device=mesa1 boot_id=0x1 layout=0x2\n# started 1788121960 (unix seconds)\n",
        "# scrubbed 400 bytes at 1788121960\n",
        "# device reset\n",
    ] {
        let text = format!(
            "{}{breaker}{}",
            log_to_text(&first.events),
            log_to_text(&second.events)
        );
        let sessions = sessions_from_text(&text).expect("parses");
        assert_eq!(sessions.len(), 2, "{breaker:?} should break the timeline");
        assert_eq!(sessions[0].events, first.events);
        assert_eq!(sessions[1].events, second.events);

        // Read as one stream it is not a log at all, and says so rather than panicking
        // once the events reach the replay.
        assert!(log_from_text(&text).is_err());

        // Each session is replayed on its own, so the second one's chords keep their own
        // times rather than being read as a step backwards.
        let derived = replay_sessions(true, &sessions);
        assert_eq!(derived.len(), 2);
        assert_eq!(chord_list(&derived[0]).len(), 2);
        assert_eq!(chord_list(&derived[1]).len(), 2);
    }
}

/// A timeline break with nothing announcing it still splits.
///
/// The collector's day rollover produced exactly this: a batch formatted with yesterday's
/// offsets, appended to today's file, with no header in between.  The offsets on either
/// side count from different zeros whether or not anyone wrote that down.
#[test]
fn test_sessions_split_at_an_unmarked_step_backwards() {
    let mut first = Log::new();
    first.tap('L', "a", 20).wait(50).tap('R', "e+i", 30);
    let mut second = Log::new();
    second.tap('R', "t", 20).wait(40).tap('L', "o", 25);

    let text = format!(
        "{}{}",
        log_to_text(&first.events),
        log_to_text(&second.events)
    );
    let sessions = sessions_from_text(&text).expect("parses");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].events, first.events);
    assert_eq!(sessions[1].events, second.events);

    let derived = replay_sessions(true, &sessions);
    assert_eq!(chord_list(&derived[0]).len(), 2);
    assert_eq!(chord_list(&derived[1]).len(), 2);
}

/// The `# started` header is kept, and a header with nothing after it is not a session.
#[test]
fn test_session_headers_and_empty_sessions() {
    let mut log = Log::new();
    log.tap('L', "a", 20);
    let text = format!(
        "# session device=mesa1 boot_id=0x1 layout=0x2\n         # started 1788121960 (unix seconds)\n         {}         # session device=mesa1 boot_id=0x1 layout=0x2\n         # started 1788122340 (unix seconds)\n",
        log_to_text(&log.events)
    );
    let sessions = sessions_from_text(&text).expect("parses");
    // The leading header opens the file, and the trailing one caught no typing before the
    // collector went away; neither is a session with anything in it.
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].started_unix, Some(1788121960));
    assert_eq!(sessions[0].events, log.events);

    // A log with no header at all is still one session.
    let bare = sessions_from_text(&log_to_text(&log.events)).expect("parses");
    assert_eq!(bare.len(), 1);
    assert_eq!(bare[0].started_unix, None);
}

//////////////////////////////////////////////////////////////////////////////
// Chords
//////////////////////////////////////////////////////////////////////////////

/// The simplest possible session: one key, struck and released.
#[test]
fn test_single_chord() {
    let mut log = Log::new();
    log.tap('L', "a", 20);

    let derived = log.run_two_row();
    let chords = chord_list(&derived);
    assert_eq!(chords.len(), 1);
    let chord = &chords[0];
    assert_eq!(chord.side, Side::Left);
    assert_eq!(chord.code, 0x001);
    assert_eq!(chord.key_names(), "a");
    assert_eq!(chord.variant, TaipoVariant::Taipo);
    assert_eq!(chord.mode, LayoutMode::Taipo);
    assert_eq!(chord.end, ChordEnd::AllReleased);
    assert_eq!(chord.action, Some(ChordAction::Key(Keyboard::A)));
    assert!(!chord.is_dead());
    assert_eq!(chord.spread_ms(), 0);
    // Committed when the last key came up.
    assert_eq!(chord.time_ms, 20);
    assert_eq!(chord.first_key_ms, 0);

    // And it really typed an `a`.
    assert_eq!(
        key_actions(&derived),
        ["KeyPress(A, Mods(0x0))", "KeyRelease"]
    );
}

/// A chord assembled finger by finger reports how long it took, which is the
/// number the whole exercise is for.
#[test]
fn test_chord_spread() {
    let mut log = Log::new();
    log.press("L.e")
        .wait(15)
        .press("L.i")
        .wait(25)
        .press("L.t")
        .wait(10)
        .release("L.e")
        .release("L.i")
        .release("L.t");

    let chords = chord_list(&log.run_two_row());
    assert_eq!(chords.len(), 1);
    assert_eq!(chords[0].first_key_ms, 0);
    assert_eq!(chords[0].last_key_ms, 40);
    assert_eq!(chords[0].spread_ms(), 40);
    assert_eq!(chords[0].end, ChordEnd::AllReleased);
    // e+i+t is "the".
    assert_eq!(chords[0].action, Some(ChordAction::Text("the")));
}

/// The three ways a chord can end, each reported as itself.
#[test]
fn test_chord_end_reasons() {
    // Struck and released.
    let mut log = Log::new();
    log.tap('L', "a", 10);
    assert_eq!(chord_list(&log.run_two_row())[0].end, ChordEnd::AllReleased);

    // Held past the window.
    let mut log = Log::new();
    log.press("L.a").wait(TAIPO_CHORD_TIME + 10).release("L.a");
    assert_eq!(
        chord_list(&log.run_two_row())[0].end,
        ChordEnd::TimerExpired
    );

    // Rolled out of: the other hand starts while this one is still down.
    let mut log = Log::new();
    log.press("L.a").wait(10).press("R.o").wait(10);
    log.release("L.a").release("R.o");
    let chords = chord_list(&log.run_two_row());
    assert_eq!(chords[0].end, ChordEnd::OtherHand);
    assert_eq!(chords[0].side, Side::Left);
    assert_eq!(chords[1].side, Side::Right);
}

/// A chord the table has no entry for types nothing at all, and is exactly the
/// error signal no host-side approach can see.
#[test]
fn test_dead_chord() {
    // The four pinky-and-ring keys of the bottom row plus both top pinkies is
    // not in the taipo table.
    let mut log = Log::new();
    log.tap('L', "a+o+t+e+r", 20);

    let derived = log.run_two_row();
    let chords = chord_list(&derived);
    assert_eq!(chords.len(), 1);
    assert!(chords[0].is_dead(), "{}", chords[0].to_line());
    assert_eq!(chords[0].action, None);
    // Nothing was typed, which is the whole point.
    assert!(key_actions(&derived).is_empty());
}

/// Hands alternating, which is how taipo is meant to be written.
#[test]
fn test_alternating_hands() {
    let mut log = Log::new();
    log.tap('L', "a", 20)
        .wait(40)
        .tap('R', "o", 20)
        .wait(40)
        .tap('L', "t", 20);

    let chords = chord_list(&log.run_two_row());
    let hands: Vec<Side> = chords.iter().map(|c| c.side).collect();
    assert_eq!(hands, [Side::Left, Side::Right, Side::Left]);
    let text: Vec<Option<ChordAction>> = chords.iter().map(|c| c.action.clone()).collect();
    assert_eq!(
        text,
        [
            Some(ChordAction::Key(Keyboard::A)),
            Some(ChordAction::Key(Keyboard::O)),
            Some(ChordAction::Key(Keyboard::T)),
        ]
    );
}

/// A one-shot modifier reports the modifier state as well as the chords.
#[test]
fn test_modifier() {
    let mut log = Log::new();
    // t+n on the left is one-shot control.
    log.tap('L', "t+n", 20).wait(40).tap('R', "a", 20);

    let derived = log.run_two_row();
    let mods: Vec<(Mods, Mods)> = derived
        .iter()
        .filter_map(|event| match event {
            Derived::Mods {
                oneshot, sticky, ..
            } => Some((*oneshot, *sticky)),
            _ => None,
        })
        .collect();
    assert_eq!(
        mods,
        [
            (Mods::CONTROL, Mods::empty()),
            (Mods::empty(), Mods::empty())
        ]
    );
    let chords = chord_list(&derived);
    assert_eq!(
        chords[0].action,
        Some(ChordAction::OneShot(Mods::CONTROL))
    );
    // The `a` was typed with control held.
    assert!(
        key_actions(&derived).contains(&"KeyPress(A, Mods(CONTROL))".to_string()),
        "{:?}",
        key_actions(&derived)
    );
}

//////////////////////////////////////////////////////////////////////////////
// Things the replay has to learn from the engine
//////////////////////////////////////////////////////////////////////////////

/// The variant toggle is part of the replay: after it, chords are looked up in
/// the posh table, and the switch itself is reported.
#[test]
fn test_variant_toggle() {
    // The posh toggle is the steno `#` key of the outer left column, which the
    // scan map gives no taipo meaning.
    let toggle = "k1";
    let mut log = Log::new();
    log.tap('L', "n+i", 20)
        .wait(50)
        .press(toggle)
        .wait(5)
        .release(toggle)
        .wait(50)
        .tap('L', "n+i", 20);

    let derived = log.run();
    let variants: Vec<TaipoVariant> = derived
        .iter()
        .filter_map(|event| match event {
            Derived::Variant { variant, .. } => Some(*variant),
            _ => None,
        })
        .collect();
    assert_eq!(variants, [TaipoVariant::Posh]);

    let chords = chord_list(&derived);
    assert_eq!(chords.len(), 2);
    // n+i is `y` in taipo and `s` in posh, on the same keys.
    assert_eq!(chords[0].variant, TaipoVariant::Taipo);
    assert_eq!(chords[0].action, Some(ChordAction::Key(Keyboard::Y)));
    assert_eq!(chords[1].variant, TaipoVariant::Posh);
    assert_eq!(chords[1].action, Some(ChordAction::Key(Keyboard::S)));
}

/// The variant selection chords replay the same way the key does: the switch
/// is reported, later chords resolve in the new table, and the switching chord
/// itself is reported against the *old* variant, because that is the table it
/// was looked up in.
#[test]
fn test_variant_chord() {
    let mut log = Log::new();
    log.tap('L', "n+i", 20)
        .wait(50)
        // a+o+t+e, the whole bottom row, selects posh.
        .tap('L', "a+o+t+e", 20)
        .wait(50)
        .tap('L', "n+i", 20)
        .wait(50)
        // r+s+n+i, the whole top row, selects taipo again.
        .tap('R', "r+s+n+i", 20)
        .wait(50)
        .tap('L', "n+i", 20);

    let derived = log.run();
    let variants: Vec<TaipoVariant> = derived
        .iter()
        .filter_map(|event| match event {
            Derived::Variant { variant, .. } => Some(*variant),
            _ => None,
        })
        .collect();
    assert_eq!(variants, [TaipoVariant::Posh, TaipoVariant::Taipo]);

    let chords = chord_list(&derived);
    assert_eq!(chords.len(), 5);

    // n+i is `y` in taipo and `s` in posh, on the same keys.
    assert_eq!(chords[0].variant, TaipoVariant::Taipo);
    assert_eq!(chords[0].action, Some(ChordAction::Key(Keyboard::Y)));

    // The switching chord is reported in the table that named it, which is the
    // one it is replacing.
    assert_eq!(chords[1].variant, TaipoVariant::Taipo);
    assert_eq!(
        chords[1].action,
        Some(ChordAction::Variant(TaipoVariant::Posh))
    );

    assert_eq!(chords[2].variant, TaipoVariant::Posh);
    assert_eq!(chords[2].action, Some(ChordAction::Key(Keyboard::S)));

    assert_eq!(chords[3].variant, TaipoVariant::Posh);
    assert_eq!(
        chords[3].action,
        Some(ChordAction::Variant(TaipoVariant::Taipo))
    );

    assert_eq!(chords[4].variant, TaipoVariant::Taipo);
    assert_eq!(chords[4].action, Some(ChordAction::Key(Keyboard::Y)));
}

/// The row position is part of the replay too: the log records key codes
/// before the shift, and the layout applies it.
#[test]
fn test_row_toggle() {
    let toggle = "k0";
    let mut log = Log::new();
    log.press(toggle).wait(5).release(toggle).wait(50);
    // In the lower position, the layout's `a` is the physical key one row
    // down, which is key code 6 rather than 5.
    log.press("k6").wait(20).release("k6");

    let derived = log.run();
    let rows: Vec<bool> = derived
        .iter()
        .filter_map(|event| match event {
            Derived::RowPosition { lower, .. } => Some(*lower),
            _ => None,
        })
        .collect();
    assert_eq!(rows, [true]);

    let chords = chord_list(&derived);
    assert_eq!(chords.len(), 1);
    assert_eq!(chords[0].code, 0x001);
    assert_eq!(chords[0].action, Some(ChordAction::Key(Keyboard::A)));
    // The timing is still taken from the key that was really pressed.
    assert_eq!(chords[0].first_key_ms, 55);
}

/// A chord still held when the log ends is committed rather than lost.
#[test]
fn test_unreleased_chord_at_end() {
    let mut log = Log::new();
    log.press("L.a");

    let chords = chord_list(&log.run_two_row());
    assert_eq!(chords.len(), 1);
    assert_eq!(chords[0].end, ChordEnd::TimerExpired);
}

//////////////////////////////////////////////////////////////////////////////
// Derived analysis helpers
//////////////////////////////////////////////////////////////////////////////

/// A chord that came apart: the window expired mid-assembly, and the rest of
/// the chord landed immediately afterwards on the same hand.
#[test]
fn test_split_chord() {
    let mut log = Log::new();
    log.press("L.e")
        .wait(TAIPO_CHORD_TIME + 5)
        .press("L.i")
        .wait(10)
        .release("L.e")
        .release("L.i");

    let derived = log.run_two_row();
    let split = chords(&derived);
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].end, ChordEnd::TimerExpired);
    assert_eq!(split_chords(&split, 50), [0]);

    // The same two chords, with a long pause between them, are two chords.
    let mut log = Log::new();
    log.tap('L', "e", 20).wait(500).tap('L', "i", 20);
    let derived = log.run_two_row();
    assert!(split_chords(&chords(&derived), 50).is_empty());
}
