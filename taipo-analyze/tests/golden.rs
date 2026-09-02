//! Golden corrections: what the analysis makes of the checked-in synthetic logs.
//!
//! `bbq-keyboard/tests/golden/*.log` are the contract for the chord engine; these are the
//! contract for the layer above it.  Which chord a backspace deleted, what replaced it,
//! and what shape the mistake was are all derived here and reimplemented in Swift, because
//! the trainer builds its drill material from them and cannot round-trip through this
//! crate to do it.  `taipo-teacher.md` names golden files as the mechanism that keeps a
//! second implementation from drifting; this is that file for corrections.
//!
//! Regenerate after a deliberate change with
//!
//! ```sh
//! cd taipo-analyze && UPDATE_GOLDEN=1 cargo test --test golden
//! ```
//!
//! and read the diff before committing it.  The Swift copy in
//! `taipo-teacher/Tests/TaipoKitTests/golden/` has to be updated with it.

use std::fs;
use std::path::PathBuf;

use taipo_analyze::stats::{Confusion, CorrectionKind};
use taipo_analyze::Options;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// The engine's golden logs live in the crate that generates them.
fn log_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../bbq-keyboard/tests/golden")
        .join(format!("{name}.log"))
}

fn kind_name(kind: CorrectionKind) -> &'static str {
    match kind {
        CorrectionKind::SameChord => "same-chord",
        CorrectionKind::OneKeyOff => "one-key-off",
        CorrectionKind::Different => "different",
        CorrectionKind::NoReplacement => "no-replacement",
    }
}

fn shape_name(shape: Option<Confusion>) -> &'static str {
    match shape {
        None => "-",
        Some(Confusion::WrongRow) => "wrong-row",
        Some(Confusion::WrongFinger) => "wrong-finger",
        Some(Confusion::DroppedKey) => "key-dropped",
        Some(Confusion::AddedKey) => "key-added",
        Some(Confusion::WrongLayer) => "wrong-layer",
        Some(Confusion::Unrelated) => "unrelated",
    }
}

/// One correction per line: what was typed, what replaced it, how far off it was, and what
/// shape the mistake had.  `-` where there is nothing to name.
fn corrections_text(name: &str) -> String {
    let text = fs::read_to_string(log_path(name)).expect("golden log");
    let a = taipo_analyze::analyze(&text, true, &Options::default()).expect("analyze");
    let mut out = String::new();
    for c in &a.corrections {
        let code = |v: Option<u16>| match v {
            Some(c) => format!("{c:#05x}"),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "{} {} {} {}\n",
            code(c.deleted),
            code(c.replacement),
            kind_name(c.kind),
            shape_name(c.confusion),
        ));
    }
    out
}

fn check(name: &str, produced: &str) {
    let path = golden_dir().join(format!("{name}.corrections"));
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
    assert_eq!(
        produced,
        committed,
        "{} has changed.  If that was meant, regenerate with \
         `UPDATE_GOLDEN=1 cargo test --test golden`",
        path.display()
    );
}

#[test]
fn test_golden_sloppy_corrections() {
    check("sloppy", &corrections_text("sloppy"));
}

#[test]
fn test_golden_slips_corrections() {
    check("slips", &corrections_text("slips"));
}
