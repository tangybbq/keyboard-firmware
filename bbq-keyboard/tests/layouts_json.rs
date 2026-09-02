//! The checked-in `layouts.json` has to match the tables it came from.
//!
//! This is the `bbq-consts` pattern: the artifact is generated from the real
//! Rust tables and committed, so that a host tool (or the Swift trainer) can
//! read it without building the crate, and this test is what keeps it from
//! quietly describing a layout the keyboard no longer has.
//!
//! The export describes the proto3 key code numbering, which is what every
//! board translates into, so the test only applies to a proto3 build.
//!
//! There is more than one committed copy.  The Swift trainer cannot depend on
//! a Cargo crate, so it ships its own, and that copy is checked here too --
//! the first one drifted for two commits and the only symptom was a
//! "(layout mismatch)" label on a keyboard that was running exactly the right
//! firmware.

#![cfg(all(feature = "proto3", feature = "std"))]

/// The generated document and the committed one are identical, byte for byte.
#[test]
fn test_layouts_json_current() {
    let generated = bbq_keyboard::layout::export::layouts_json();
    let committed = include_str!("../layouts.json");

    if generated != committed {
        // Show the first line that differs; the whole document is far too big
        // to read in a test failure.
        let mismatch = generated
            .lines()
            .zip(committed.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b);
        let detail = match mismatch {
            Some((num, (a, b))) => format!(
                "line {}:\n  generated: {}\n  committed: {}",
                num + 1,
                a,
                b
            ),
            None => format!(
                "the files agree line for line, but one is longer: \
                 generated {} lines, committed {}",
                generated.lines().count(),
                committed.lines().count()
            ),
        };
        panic!(
            "bbq-keyboard/layouts.json is stale.  Regenerate it with\n    \
             cd bbq-keyboard && cargo run --example gen-layouts > layouts.json\n{}",
            detail
        );
    }
}

/// Every other committed copy matches the one this crate generates.
///
/// The Swift package reads `layouts.json` from its own bundle, so there is a
/// second copy of it under `taipo-teacher/`, and nothing was keeping the two in
/// step.  It went stale the moment the tables changed, and what that produces is
/// worse than a stale file: the app compares the device's reported fingerprint
/// against its copy, so a correctly flashed keyboard reports a layout mismatch
/// and the app resolves chords through tables the keyboard no longer has.
///
/// Checked here rather than in the Swift test suite because here is where it
/// goes stale -- regenerating the export and running `cargo test` is one
/// motion, and the copy has to move with it.
#[test]
fn test_committed_copies_agree() {
    // Relative to this crate, which is where a sibling checkout of the repo
    // puts them.  Missing is a failure, not a skip: a copy that has been moved
    // or deleted is exactly the case this test exists to notice.
    const COPIES: &[&str] = &["../taipo-teacher/Sources/TaipoKit/layouts.json"];

    let ours = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("layouts.json");
    let reference = std::fs::read_to_string(&ours).expect("bbq-keyboard/layouts.json");

    for relative in COPIES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        let copy = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            copy == reference,
            "{} is out of step with bbq-keyboard/layouts.json.  Update it with\n    \
             cp {} {}",
            path.display(),
            ours.display(),
            path.display(),
        );
    }
}
