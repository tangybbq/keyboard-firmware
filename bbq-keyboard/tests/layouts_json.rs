//! The checked-in `layouts.json` has to match the tables it came from.
//!
//! This is the `bbq-consts` pattern: the artifact is generated from the real
//! Rust tables and committed, so that a host tool (or the Swift trainer) can
//! read it without building the crate, and this test is what keeps it from
//! quietly describing a layout the keyboard no longer has.
//!
//! The export describes the proto3 key code numbering, which is what every
//! board translates into, so the test only applies to a proto3 build.

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
