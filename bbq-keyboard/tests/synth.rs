//! Tests for the synthetic log generator.
//!
//! The generator's job is ground truth: a log where what was meant is known,
//! so the analysis can be checked against something.  These tests mostly close
//! the loop -- generate a log, replay it through the real engine, and check
//! that what came out is what was asked for.

#![cfg(all(feature = "proto3", feature = "std"))]

use bbq_keyboard::layout::export::char_for_key;
use bbq_keyboard::layout::taipo::{ChordEnd, TaipoVariant};
use bbq_keyboard::replay::{chords, replay, Derived};
use bbq_keyboard::synth::{synth, ErrorKind, Purpose, Style, SynthLog};
use bbq_keyboard::{KeyAction, Keyboard, Mods, Side};

/// The text a host would have seen, reconstructed from the HID reports the
/// layout asked for.  Backspaces are applied, so a log with corrections in it
/// still produces the text that was meant.
fn typed(derived: &[Derived]) -> String {
    let mut out = String::new();
    for event in derived {
        let Derived::Key {
            action: KeyAction::KeyPress(key, mods),
            ..
        } = event
        else {
            continue;
        };
        if *key == Keyboard::DeleteBackspace {
            out.pop();
            continue;
        }
        if let Some(ch) = char_for_key(*key, mods.contains(Mods::SHIFT)) {
            out.push(ch);
        }
    }
    out
}

/// Generate and replay, on a two-row board.
fn round_trip(target: &str, style: &Style) -> (SynthLog, Vec<Derived>) {
    let log = synth(target, style).expect("the table can type this");
    let derived = replay(true, &log.events);
    (log, derived)
}

/// The simplest claim the generator makes: replaying its log types the target.
#[test]
fn test_clean_log_types_the_target() {
    let target = "the quick brown fox jumps over the lazy dog";
    let (_, derived) = round_trip(target, &Style::default());
    assert_eq!(typed(&derived), target);
}

/// Capitals, digits and punctuation all come out too, which exercises the
/// shifted half of both tables.
#[test]
fn test_mixed_text() {
    let target = "The Answer: 42 (probably).";
    let (_, derived) = round_trip(target, &Style::default());
    assert_eq!(typed(&derived), target);
}

/// The posh table is a different layout, and the generator follows it.
#[test]
fn test_posh() {
    let style = Style {
        variant: TaipoVariant::Posh,
        ..Style::default()
    };
    let log = synth("the rain in spain", &style).expect("posh can type this");
    // The replay has to be told to use the posh table as well, which the
    // device's Variant marker will say; here it is tapped in.
    let mut events = log.events.clone();
    for event in &mut events {
        event.time_ms += 20;
    }
    let toggle = bbq_keyboard::replay::key_for_name("k1").unwrap();
    let mut all = toggle_tap(toggle);
    all.extend(events);
    let derived = replay(false, &all);
    assert_eq!(typed(&derived), "the rain in spain");
}

/// A solo tap of the variant toggle key, at the start of a log.
fn toggle_tap(toggle: u8) -> Vec<bbq_keyboard::replay::KeyLogEvent> {
    use bbq_keyboard::replay::KeyLogEvent;
    vec![
        KeyLogEvent {
            time_ms: 0,
            key: toggle,
            press: true,
        },
        KeyLogEvent {
            time_ms: 5,
            key: toggle,
            press: false,
        },
    ]
}

/// A multi-character gram is one chord, and spelling it out makes it several
/// while typing the same thing.  This is the signal phase 3 looks for, so the
/// generator has to be able to produce both.
#[test]
fn test_spelling_out() {
    let chorded = Style::default();
    let (log, derived) = round_trip("the", &chorded);
    assert_eq!(log.planned.len(), 1);
    assert_eq!(chords(&derived).len(), 1);
    assert_eq!(typed(&derived), "the");

    let spelled = Style {
        spell_every: 1,
        ..Style::default()
    };
    let (log, derived) = round_trip("the", &spelled);
    assert_eq!(log.planned.len(), 3);
    assert_eq!(log.count(Purpose::Spelled), 3);
    assert_eq!(chords(&derived).len(), 3);
    assert_eq!(typed(&derived), "the");
}

/// Chord spread comes through the replay as the number the analysis wants.
#[test]
fn test_spread() {
    let style = Style {
        spread_ms: 12,
        ..Style::default()
    };
    let (_, derived) = round_trip("and", &style);
    let chords = chords(&derived);
    // `and` is one chord of four keys, so three gaps of 12ms.
    assert_eq!(chords.len(), 1);
    assert_eq!(chords[0].spread_ms(), 36);
    assert_eq!(typed(&derived), "and");
}

/// Hands alternate by default, and stop when told to.
#[test]
fn test_hands() {
    let (_, derived) = round_trip("aeiou", &Style::default());
    let hands: Vec<Side> = chords(&derived).iter().map(|c| c.side).collect();
    assert_eq!(
        hands,
        [Side::Left, Side::Right, Side::Left, Side::Right, Side::Left]
    );

    let style = Style {
        same_hand_every: 2,
        ..Style::default()
    };
    let (_, derived) = round_trip("aeiou", &style);
    let hands: Vec<Side> = chords(&derived).iter().map(|c| c.side).collect();
    // Every second chord stays where the one before it was.
    assert_eq!(
        hands,
        [Side::Left, Side::Left, Side::Right, Side::Right, Side::Left]
    );
}

/// Rolling from one hand into the next is what ends a chord early, and is the
/// only way to produce an `OtherHand` ending.
#[test]
fn test_overlap_ends_chords_early() {
    let style = Style {
        hold_ms: 40,
        overlap_ms: 20,
        gap_ms: 0,
        ..Style::default()
    };
    let (_, derived) = round_trip("aeiou", &style);
    let chords = chords(&derived);
    assert_eq!(typed(&derived), "aeiou");
    assert!(
        chords
            .iter()
            .filter(|c| c.end == ChordEnd::OtherHand)
            .count()
            >= 3,
        "{:?}",
        chords.iter().map(|c| c.to_line()).collect::<Vec<_>>()
    );
}

/// A misfingering types the wrong thing, is backspaced, and is retyped, and
/// the text still comes out right.
#[test]
fn test_misfingering() {
    let style = Style {
        error_every: 3,
        errors: vec![ErrorKind::Misfingering],
        ..Style::default()
    };
    let (log, derived) = round_trip("aeiou", &style);
    assert_eq!(typed(&derived), "aeiou");
    assert_eq!(log.count(Purpose::Error(ErrorKind::Misfingering)), 1);
    assert_eq!(log.count(Purpose::Correction), 1);
    assert_eq!(log.count(Purpose::Retype), 1);
    // The wrong chord really is one key different from the right one.
    let wrong = log
        .planned
        .iter()
        .find(|p| p.purpose == Purpose::Error(ErrorKind::Misfingering))
        .unwrap();
    assert_eq!((wrong.code ^ wrong.intended).count_ones(), 2);
    assert_eq!(wrong.code.count_ones(), wrong.intended.count_ones());
}

/// A dead chord types nothing at all, so it needs no correction, and the
/// replay reports it as a chord with no action.
#[test]
fn test_dead_chord() {
    let style = Style {
        error_every: 2,
        errors: vec![ErrorKind::DeadChord],
        ..Style::default()
    };
    let (log, derived) = round_trip("aeiou", &style);
    assert_eq!(typed(&derived), "aeiou");
    assert_eq!(log.count(Purpose::Correction), 0);
    let dead: Vec<_> = chords(&derived).into_iter().filter(|c| c.is_dead()).collect();
    assert_eq!(dead.len(), log.count(Purpose::Error(ErrorKind::DeadChord)));
    assert_eq!(dead.len(), 2);
}

/// A wrong chord types some other letter, which is then taken back.
#[test]
fn test_wrong_chord() {
    let style = Style {
        error_every: 2,
        errors: vec![ErrorKind::WrongChord],
        ..Style::default()
    };
    let (log, derived) = round_trip("aeiou", &style);
    assert_eq!(typed(&derived), "aeiou");
    assert_eq!(log.count(Purpose::Correction), 2);
    for wrong in log
        .planned
        .iter()
        .filter(|p| p.purpose == Purpose::Error(ErrorKind::WrongChord))
    {
        assert_ne!(wrong.code, wrong.intended);
    }
}

/// A split chord comes out of the engine as two chords, the first ended by the
/// timer.  That is `CHORD_TIME` being hit mid-assembly, and it is the one
/// error the writer never sees as an error.
#[test]
fn test_split_chord() {
    let style = Style {
        split_every: 1,
        ..Style::default()
    };
    let (log, derived) = round_trip("and", &style);
    // One planned chord, two derived ones.
    assert_eq!(log.planned.len(), 1);
    assert!(log.planned[0].split);
    let chords = chords(&derived);
    assert_eq!(chords.len(), 2);
    assert_eq!(chords[0].end, ChordEnd::TimerExpired);
    assert_eq!(chords[0].side, chords[1].side);
    // And it did not type `and`.
    assert_ne!(typed(&derived), "and");
}

/// A character the table cannot type is an error, named, rather than something
/// quietly dropped from a log that claims to be the target text.
#[test]
fn test_untypable() {
    let error = synth("héllo", &Style::default()).unwrap_err();
    assert!(error.contains('é'), "{error}");
}

/// The generated log is in time order and every event names a real key.
#[test]
fn test_log_is_well_formed() {
    let style = Style {
        spread_ms: 8,
        overlap_ms: 15,
        same_hand_every: 5,
        spell_every: 3,
        split_every: 7,
        error_every: 4,
        ..Style::default()
    };
    let (log, derived) = round_trip("the rain in spain falls mainly on the plain", &style);
    let mut last = 0;
    for event in &log.events {
        assert!(event.time_ms >= last, "log went backwards");
        last = event.time_ms;
        assert!(event.key < 48);
    }
    // Every key that goes down comes back up.
    let downs = log.events.iter().filter(|e| e.press).count();
    let ups = log.events.iter().filter(|e| !e.press).count();
    assert_eq!(downs, ups);
    // And the replay made something of all of it.
    assert!(!chords(&derived).is_empty());
}
