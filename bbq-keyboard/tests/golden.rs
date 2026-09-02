//! Golden files: a synthetic log, and what the replay makes of it.
//!
//! Both halves are checked in, in the plain text formats
//! [`bbq_keyboard::replay`] defines, so that a change to the chord engine, the
//! tables, the log format or the derived format shows up as a readable diff
//! rather than as a number moving somewhere in the statistics.
//!
//! They are also the contract for phase 4 of `taipo-teacher.md`: the Mac app
//! assembles chords in Swift rather than round-tripping through this crate,
//! which is a second implementation of the engine, and these files are what
//! keeps it from drifting.  A Swift test reads the `.log` and has to produce
//! the `.derived`.
//!
//! Regenerate after a deliberate change with
//!
//! ```sh
//! cd bbq-keyboard && UPDATE_GOLDEN=1 cargo test --test golden
//! ```
//!
//! and read the diff before committing it.

#![cfg(all(feature = "proto3", feature = "std"))]

use std::fs;
use std::path::PathBuf;

use bbq_keyboard::replay::{derived_to_text, log_to_text, replay};
use bbq_keyboard::synth::{synth, ErrorKind, Style};

/// Where the checked-in files live.
fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Compare against the checked-in file, or rewrite it when asked.
fn check(name: &str, extension: &str, produced: &str) {
    let path = golden_dir().join(format!("{name}.{extension}"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(golden_dir()).expect("golden directory");
        fs::write(&path, produced).expect("write golden");
        return;
    }
    let committed = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nRegenerate with `UPDATE_GOLDEN=1 cargo test --test golden`",
            path.display()
        )
    });
    if produced != committed {
        let mismatch = produced
            .lines()
            .zip(committed.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b);
        let detail = match mismatch {
            Some((num, (a, b))) => {
                format!("line {}:\n  now: {}\n  was: {}", num + 1, a, b)
            }
            None => format!(
                "one is longer: now {} lines, was {}",
                produced.lines().count(),
                committed.lines().count()
            ),
        };
        panic!(
            "{} has changed.  If that was meant, regenerate with\n    \
             UPDATE_GOLDEN=1 cargo test --test golden\n{}",
            path.display(),
            detail
        );
    }
}

/// Generate a log, replay it, and check both halves.
fn golden(name: &str, target: &str, style: &Style, two_row: bool) {
    let log = synth(target, style).expect("the table can type this");
    let derived = replay(two_row, &log.events);
    check(name, "log", &log_to_text(&log.events));
    check(name, "derived", &derived_to_text(&derived));
}

/// A tidy writer on a mesa1: chords struck, hands alternating, nothing wrong.
#[test]
fn test_golden_clean() {
    golden(
        "clean",
        "the quick brown fox jumps over the lazy dog",
        &Style::default(),
        true,
    );
}

/// The same writer having a bad day: chords assembled rather than struck, one
/// hand used twice in a row, grams spelled out, a chord coming apart, and all
/// three kinds of mistake with the corrections that follow them.
#[test]
fn test_golden_sloppy() {
    let style = Style {
        spread_ms: 14,
        hold_ms: 45,
        gap_ms: -18,
        same_hand_every: 5,
        spell_every: 3,
        split_every: 11,
        error_every: 7,
        errors: vec![
            ErrorKind::Misfingering,
            ErrorKind::WrongChord,
            ErrorKind::DeadChord,
        ],
        ..Style::default()
    };
    golden(
        "sloppy",
        "the rain in spain falls mainly on the plain",
        &style,
        true,
    );
}

/// A writer whose fingers land on the wrong row, and on the wrong finger.
///
/// The two shapes that dominate real corrections, generated deliberately so the
/// analysis and the Swift trainer both have something with a known answer to be
/// checked against.  Chords are struck and hands alternate, so the mistakes are
/// the only thing in it.
#[test]
fn test_golden_slips() {
    let style = Style {
        error_every: 3,
        errors: vec![
            ErrorKind::RowSlip,
            ErrorKind::Misfingering,
            ErrorKind::RowSlip,
        ],
        ..Style::default()
    };
    golden(
        "slips",
        "some of those seasons matter to no one at all",
        &style,
        true,
    );
}
