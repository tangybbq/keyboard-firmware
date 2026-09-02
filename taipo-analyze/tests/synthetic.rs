//! The analysis against synthetic logs whose faults are known in advance.
//!
//! `bbq_keyboard::synth` builds a log from a target string with exactly the faults asked
//! for, which is the only way to say "this should find three same-hand runs" and mean it.
//! Testing against real typing could only ever check that the numbers look plausible.

use bbq_keyboard::layout::taipo::TaipoVariant;
use bbq_keyboard::replay::log_to_text;
use bbq_keyboard::synth::{synth, ErrorKind, Style};
use taipo_analyze::stats::{Analysis, CorrectionKind, Item};
use taipo_analyze::Options;

/// A writer who does everything right: alternating hands, chording every gram, no errors.
fn clean() -> Style {
    Style {
        variant: TaipoVariant::Taipo,
        start_ms: 0,
        spread_ms: 2,
        hold_ms: 30,
        gap_ms: 40,
        same_hand_every: 0,
        spell_every: 0,
        split_every: 0,
        error_every: 0,
        errors: vec![ErrorKind::Misfingering],
    }
}

fn analyze(target: &str, style: &Style) -> Analysis {
    let log = synth(target, style).expect("synth");
    let text = log_to_text(&log.events);
    taipo_analyze::analyze(&text, true, &Options::default()).expect("analyze")
}

/// The same typing twice, as two sessions of one file.
///
/// What a day's log actually looks like: the collector appends to it each time it
/// connects, and each session's offsets count from its own zero.
fn analyze_twice(target: &str, style: &Style) -> Analysis {
    let log = synth(target, style).expect("synth");
    let one = log_to_text(&log.events);
    let text = format!(
        "# session device=mesa1 boot_id=0x1 layout=0x2\n         # started 1788121960 (unix seconds)\n         {one}         # session device=mesa1 boot_id=0x1 layout=0x2\n         # started 1788122340 (unix seconds)\n         {one}"
    );
    taipo_analyze::analyze(&text, true, &Options::default()).expect("analyze")
}

/// A file with two sessions is two timelines, and neither one leaks into the other.
///
/// The second session's offsets restart at zero, which as a single stream is a step
/// backwards in time.  Everything countable should simply double, and the pair that would
/// straddle the join -- the last chord of one session against the first of the next --
/// should not exist: it is two chords that may be hours apart, whose times cannot even be
/// subtracted from each other.
#[test]
fn test_two_sessions_do_not_join_up() {
    let target = "the quick brown fox jumps over the lazy dog";
    let one = analyze(target, &clean());
    let two = analyze_twice(target, &clean());

    assert_eq!(two.sessions, 2);
    assert_eq!(one.sessions, 1);
    assert_eq!(two.total_chords, one.total_chords * 2);
    // Summed per session, so the gap between sittings is not counted as typing time.
    assert_eq!(two.span_ms, one.span_ms * 2);
    // The join contributes no pair.  Flattened it would have contributed one more.
    assert_eq!(two.eligible_pairs, one.eligible_pairs * 2);
    assert_eq!(two.spelled.len(), one.spelled.len() * 2);

    // And the hesitation times say which session they are in.
    assert!(two
        .hesitations
        .iter()
        .all(|h| h.at.session < 2 && h.at.time_ms <= one.span_ms));
}

/// A same-hand pair that straddles a session boundary is not a same-hand pair.
///
/// The clean writer alternates, so the last chord of one session and the first of the next
/// land on the same hand.  Read as one stream that is a same-hand pair inside the window,
/// which is exactly the false fault a naive concatenation invents.
#[test]
fn test_a_session_join_invents_no_fault() {
    let target = "the quick brown fox";
    let one = analyze(target, &clean());
    let two = analyze_twice(target, &clean());
    assert_eq!(one.same_hand.len(), 0, "the control alternates");
    assert_eq!(two.same_hand.len(), 0, "and the join adds nothing");
    assert_eq!(two.corrections.len(), one.corrections.len() * 2);
}

/// A clean writer produces a clean report.  This is the control: if it fails, everything
/// below is measuring the generator rather than the analysis.
#[test]
fn test_clean_typing_has_nothing_to_report() {
    let a = analyze("the quick brown fox jumps over the lazy dog", &clean());
    assert!(a.total_chords > 20, "got {} chords", a.total_chords);
    assert_eq!(a.same_hand.len(), 0, "clean typing alternates");
    assert_eq!(a.corrections.len(), 0, "clean typing does not correct");
    assert_eq!(a.spelled.len(), 0, "clean typing chords its grams");
    assert_eq!(a.dead.len(), 0, "clean typing hits real chords");
}

/// Same-hand runs are found, and only inside the window.
#[test]
fn test_same_hand_runs_are_found() {
    let style = Style {
        same_hand_every: 4,
        ..clean()
    };
    let a = analyze("the quick brown fox jumps over the lazy dog", &style);
    assert!(a.same_hand.len() >= 5, "found {}", a.same_hand.len());
    assert!(a.eligible_pairs > a.same_hand.len());
    assert!(a.same_hand_rate() > 0.0 && a.same_hand_rate() < 1.0);
}

/// The alternation window is what makes a pause not count.
///
/// The same writer, the same faults; only the gap between chords changes.  With a gap wider
/// than the window, nothing is eligible and nothing is reported -- which is the rule the
/// whole metric rests on.
#[test]
fn test_pause_exempts_the_next_chord() {
    let style = Style {
        same_hand_every: 3,
        gap_ms: 3000,
        ..clean()
    };
    let a = analyze("the quick brown fox", &style);
    assert_eq!(a.eligible_pairs, 0, "nothing should be eligible");
    assert_eq!(a.same_hand.len(), 0, "and so nothing should be reported");

    // The identical faults, typed without the pauses, are reported.
    let tight = Style { gap_ms: 40, ..style };
    let b = analyze("the quick brown fox", &tight);
    assert!(b.eligible_pairs > 0);
    assert!(!b.same_hand.is_empty());
}

/// Errors and their corrections are found and classified.
#[test]
fn test_corrections_are_classified() {
    let style = Style {
        error_every: 5,
        errors: vec![ErrorKind::Misfingering],
        ..clean()
    };
    let a = analyze("the quick brown fox jumps over the lazy dog", &style);
    assert!(!a.corrections.is_empty(), "no corrections found");
    // The generator makes one-key-off errors, so that is what should dominate.  It is not
    // all of them: a one-key-off chord can land on another real chord, and correcting to
    // the intended one is then a different-chord correction by the bits alone.
    let one_off = a
        .corrections
        .iter()
        .filter(|c| c.kind == CorrectionKind::OneKeyOff)
        .count();
    assert!(
        one_off > 0,
        "expected one-key-off corrections, got {:?}",
        a.corrections.iter().map(|c| c.kind).collect::<Vec<_>>()
    );
}

/// A gram typed letter by letter is caught, which is the check no other tool can make.
#[test]
fn test_spelled_grams_are_found() {
    let style = Style {
        spell_every: 1,
        ..clean()
    };
    // "the" and "for" both have chords in TAIPO_ACTIONS.
    let a = analyze("the for the for", &style);
    assert!(!a.spelled.is_empty(), "no spelled grams found");
    assert!(
        a.spelled.iter().any(|g| g.text == "the"),
        "expected 'the', got {:?}",
        a.spelled.iter().map(|g| g.text).collect::<Vec<_>>()
    );

    // The same text chorded properly reports nothing.
    let b = analyze("the for the for", &clean());
    assert_eq!(b.spelled.len(), 0);
}

/// Chord endings are counted, and rolling the next chord into the current one is what
/// produces the cross-hand ending.
#[test]
fn test_chord_endings() {
    // A negative gap rolls the hands together, which is the only way OtherHand fires.
    let rolled = Style {
        gap_ms: -20,
        hold_ms: 60,
        ..clean()
    };
    let a = analyze("the quick brown fox", &rolled);
    assert!(
        a.ended_other_hand > 0,
        "rolled typing should end chords with the other hand: {} released, {} timer, {} other",
        a.ended_released,
        a.ended_timer,
        a.ended_other_hand
    );

    // Held well past the window, the timer is what ends them.
    let slow = Style {
        hold_ms: 400,
        gap_ms: 200,
        ..clean()
    };
    let b = analyze("the quick brown fox", &slow);
    assert!(b.ended_timer > 0);
    assert_eq!(b.ended_other_hand, 0, "nothing should overlap at this pace");
}

//////////////////////////////////////////////////////////////////////////////
// Ranking
//////////////////////////////////////////////////////////////////////////////

/// Typing at a mix of paces, none of which is any item's fault, ranks nothing.
///
/// This is the regression test for what the first version of the ranking did, and the
/// pace has to vary for it to be one.  Real gaps are strongly right skewed, and summing
/// `max(0, gap - median)` charges every item for its own right tail while crediting it
/// with nothing for being quick, so the total grew with exposure alone: the list came out
/// as the frequency table in order, space first, then the commonest letters.  Against the
/// mean, and clamping only the total, a writer whose speed varies for reasons that have
/// nothing to do with any particular chord has nothing to show.
///
/// Uniform synthetic gaps cannot catch this -- with no tail there is nothing for the
/// truncation to keep -- so the log here is four sittings of the same text, three brisk
/// and one slow.
#[test]
fn test_a_mix_of_paces_ranks_nothing() {
    let target = "the quick brown fox jumps over the lazy dog and the dog does not mind";
    let brisk = log_to_text(&synth(target, &clean()).expect("synth").events);
    let slow = log_to_text(
        &synth(
            target,
            &Style {
                gap_ms: 600,
                ..clean()
            },
        )
        .expect("synth")
        .events,
    );
    // Each sitting is its own session, which is what a day's log is.
    let text = [&brisk, &brisk, &brisk, &slow]
        .iter()
        .map(|s| format!("# session device=mesa1 boot_id=0x1 layout=0x2\n{s}"))
        .collect::<String>();
    let a = taipo_analyze::analyze(&text, true, &Options::default()).expect("analyze");
    let rows = a.ranked(false);
    let total: u64 = rows.iter().map(|r| r.cost_ms).sum();
    assert!(
        total < a.span_ms as u64 / 20,
        "a change of pace is nobody's trouble spot; got {total}ms over {}ms",
        a.span_ms
    );

    // And in particular the ranking is not the frequency table.  The commonest chord is
    // typed many times more than the rarest and must not be at the top for it.
    let commonest = a
        .items
        .iter()
        .filter(|(i, _)| matches!(i, Item::Chord { .. }))
        .max_by_key(|(_, s)| s.count)
        .map(|(i, _)| *i)
        .expect("something was typed");
    assert!(
        rows.first().map(|r| r.item) != Some(commonest) || rows.len() <= 1,
        "the ranking has come out as the frequency table again"
    );
}

/// One item really being slow is what the ranking is supposed to find.
///
/// Two writers of the same text, differing only in pace: the slow one's every item is
/// above the fast one's baseline, but the baseline is its own, so it still ranks nothing.
/// What must rank is an item that is slow *relative to the writer it belongs to*, which is
/// what the corrections give us here -- they are the one thing in the generator that costs
/// a specific chord a specific amount of extra time.
#[test]
fn test_corrections_carry_their_measured_cost() {
    let style = Style {
        error_every: 4,
        ..clean()
    };
    let a = analyze("the quick brown fox jumps over the lazy dog", &style);
    let rows = a.ranked(false);
    assert!(!rows.is_empty(), "corrections should cost something");

    // Every correction that had a replacement is priced, and the prices add up to what the
    // items were charged.
    let priced: u64 = a
        .corrections
        .iter()
        .filter_map(|c| c.cost_ms.filter(|_| c.deleted.is_some()))
        .map(|ms| ms as u64)
        .sum();
    let charged: u64 = a
        .items
        .values()
        .map(|s| s.correction_ms)
        .sum();
    assert_eq!(priced, charged, "every correction is charged to what it deleted");
    assert!(priced > 0, "the generator was asked for errors");

    // A correction costs more than nothing and less than the ceiling, which is the only
    // claim the measurement makes.
    for c in &a.corrections {
        if let Some(ms) = c.cost_ms {
            assert!(ms > 0 && ms <= Options::default().idle_ms, "cost {ms}ms");
        }
    }
}

/// Working the machine is not typing, and stays out of the ranking.
#[test]
fn test_the_ranking_is_only_of_chords_that_type() {
    use bbq_keyboard::layout::taipo::{Action, TAIPO_ACTIONS};
    use taipo_analyze::stats::{types_a_character, VariantKey};

    for entry in TAIPO_ACTIONS {
        let types = types_a_character(VariantKey::Taipo, entry.code);
        match &entry.action {
            Action::OneShot(_) | Action::Release | Action::Variant(_) => {
                assert!(!types, "0x{:03x} is not typing", entry.code)
            }
            Action::Text(_) => assert!(types, "0x{:03x} types text", entry.code),
            // Return, the arrows and the function keys have no character; the letters,
            // the digits, the punctuation and space all do.
            Action::Simple(_) | Action::Shifted(_) => (),
        }
    }
    assert!(types_a_character(VariantKey::Taipo, 0x100), "space types");
    // A code with no entry at all is a dead chord, which types nothing by definition.
    assert!(!types_a_character(VariantKey::Taipo, 0x1ff));
}
