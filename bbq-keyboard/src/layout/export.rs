//! A machine-readable description of the chord tables.
//!
//! Everything a host tool needs to talk about a Taipo or Posh chord without
//! reimplementing the layout: the chord tables themselves, the scan map in
//! both row positions, the per-board scan code translation, and the codes of
//! the keys the layout manager handles itself.
//!
//! The output is checked in as `bbq-keyboard/layouts.json`, following the
//! `bbq-consts` precedent: generated from the real tables by the compiler,
//! committed, and guarded by a test that fails when it goes stale.  Regenerate
//! it with
//!
//! ```sh
//! cd bbq-keyboard && cargo run --example gen-layouts > layouts.json
//! ```
//!
//! This is host-only (it builds a `String`), so it is behind the `std`
//! feature and costs the firmware nothing.
//!
//! # The scan code pipeline
//!
//! A physical key travels through three stages before it reaches a chord, and
//! the JSON describes each of them so that a log of raw key codes can be
//! resolved the same way the firmware resolves it:
//!
//! 1. `boards[<board>][scan]` — the matrix position, translated to the proto3
//!    key code numbering.  255 means the position has no key.  This is the
//!    space the event log records, because it is board-independent.
//! 2. `scan_map.upper[key]` / `scan_map.lower[key]` — the key code, in
//!    whichever row position the layout is in, resolved to a hand and a chord
//!    bit.  Two-row boards are always `upper`.
//! 3. `variants.<variant>.chords[]` — the assembled chord code, looked up in
//!    the table the engine currently has selected.
//!
//! JSON is written by hand rather than through serde: the structure is small
//! and fixed, the crate has no serde dependency, and writing it out here makes
//! the field order (and therefore the checked-in file) deterministic.

use alloc::format;
use alloc::string::{String, ToString};

use crate::Mods;

use super::posh::POSH_ACTIONS;
use super::taipo::{Action, Entry, CHORD_TIME, SCAN_MAP, TAIPO_ACTIONS};
use super::MODE_KEY;
#[cfg(feature = "proto3")]
use super::{POSH_TOGGLE_KEY, ROW_TOGGLE_KEY};

/// The format version of the emitted document.
///
/// Bumped when the shape changes in a way a consumer has to know about.  A new
/// field with an obvious meaning does not need a bump; a renamed or
/// re-interpreted one does.
const FORMAT_VERSION: u32 = 1;

/// The name of each chord bit, in bit order, as `TAIPO.md` names them: the
/// bottom row from pinky to index, then the top row, then the two thumbs.
///
/// These are the names everything outside the firmware uses for a key: the
/// JSON export, the replay's log format, and the drills.
pub const BIT_NAMES: [&str; 10] = ["a", "o", "t", "e", "r", "s", "n", "i", "Sp", "Bk"];

/// The finger each chord bit is struck with.
pub const BIT_FINGERS: [&str; 10] = [
    "pinky", "ring", "middle", "index", "pinky", "ring", "middle", "index", "thumb", "thumb",
];

/// The row each chord bit sits on.
pub const BIT_ROWS: [&str; 10] = [
    "bottom", "bottom", "bottom", "bottom", "top", "top", "top", "top", "thumb", "thumb",
];

/// The key code numbering the tables are indexed in.
#[cfg(feature = "proto3")]
const SCANCODE_SET: &str = "proto3";
#[cfg(feature = "proto2")]
const SCANCODE_SET: &str = "proto2";

/// Apply the row position shift, so that the `lower` scan map can be built the
/// same way the layout builds it.  Two-row builds have no shift.
#[cfg(feature = "proto3")]
fn remap_lower(key: u8) -> u8 {
    super::lower_row_remap(key)
}

#[cfg(not(feature = "proto3"))]
fn remap_lower(key: u8) -> u8 {
    key
}

/// The whole document, as pretty-printed JSON with a trailing newline.
pub fn layouts_json() -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"format_version\": {},\n", FORMAT_VERSION));
    out.push_str(
        "  \"generator\": \"bbq-keyboard/src/layout/export.rs; \
         regenerate with `cargo run --example gen-layouts > layouts.json`\",\n",
    );
    out.push_str(&format!("  \"scancode_set\": \"{}\",\n", SCANCODE_SET));
    out.push_str(&format!("  \"chord_time_ms\": {},\n", CHORD_TIME));
    out.push_str(&format!("  \"bits\": [\n{}\n  ],\n", bits_json()));
    out.push_str(&format!("  \"special_keys\": {},\n", special_keys_json()));
    out.push_str(&format!("  \"scan_map\": {},\n", scan_map_json()));
    out.push_str(&format!("  \"boards\": {},\n", boards_json()));
    out.push_str("  \"variants\": {\n");
    out.push_str(&format!(
        "    \"taipo\": {},\n",
        variant_json(TAIPO_ACTIONS)
    ));
    out.push_str(&format!("    \"posh\": {}\n", variant_json(POSH_ACTIONS)));
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

/// The ten chord bits, in bit order.
fn bits_json() -> String {
    let mut rows = String::new();
    for bit in 0..10 {
        if bit != 0 {
            rows.push_str(",\n");
        }
        rows.push_str(&format!(
            "    {{ \"bit\": {}, \"mask\": {}, \"name\": {}, \"finger\": {}, \"row\": {} }}",
            bit,
            1u16 << bit,
            quote(BIT_NAMES[bit]),
            quote(BIT_FINGERS[bit]),
            quote(BIT_ROWS[bit]),
        ));
    }
    rows
}

/// The key codes the layout manager handles for itself, before any layout sees
/// them.  A replay has to know these to explain a key event that produced no
/// chord.
fn special_keys_json() -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("    \"mode\": {},\n", MODE_KEY));
    #[cfg(feature = "proto3")]
    {
        out.push_str(&format!("    \"row_toggle\": {},\n", ROW_TOGGLE_KEY));
        out.push_str(&format!("    \"posh_toggle\": {},\n", POSH_TOGGLE_KEY));
    }
    // The taipo latch keys, which let taipo be typed while in steno mode.
    // Found by asking, rather than by naming the constants, so that a board
    // with a different set needs no change here.
    let mut latch = String::new();
    for key in 0..SCAN_MAP.len() as u8 {
        if let Some(bit) = super::taipo_map(key) {
            if !latch.is_empty() {
                latch.push_str(", ");
            }
            latch.push_str(&format!("{{ \"key\": {}, \"bit\": {} }}", key, bit));
        }
    }
    out.push_str(&format!("    \"taipo_latch\": [{}]\n", latch));
    out.push_str("  }");
    out
}

/// `SCAN_MAP` in both row positions, indexed by key code.
fn scan_map_json() -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("    \"upper\": [\n{}\n    ],\n", scan_row(false)));
    out.push_str(&format!("    \"lower\": [\n{}\n    ]\n", scan_row(true)));
    out.push_str("  }");
    out
}

/// One row position's worth of the scan map, one key code per line.
fn scan_row(lower: bool) -> String {
    let mut rows = String::new();
    for key in 0..SCAN_MAP.len() as u8 {
        if key != 0 {
            rows.push_str(",\n");
        }
        let looked_up = if lower { remap_lower(key) } else { key };
        match SCAN_MAP[looked_up as usize] {
            Some((side, mask)) => {
                let bit = mask.trailing_zeros() as usize;
                rows.push_str(&format!(
                    "      {{ \"key\": {}, \"side\": {}, \"bit\": {}, \"mask\": {}, \"name\": {} }}",
                    key,
                    quote(side_name(side)),
                    bit,
                    mask,
                    quote(BIT_NAMES[bit]),
                ));
            }
            None => rows.push_str(&format!("      {{ \"key\": {}, \"side\": null }}", key)),
        }
    }
    rows
}

/// Every board's scan position to key code table, 255 for a position with no
/// key.  The array is indexed by the matrix's scan code.
fn boards_json() -> String {
    let mut out = String::new();
    out.push_str("{\n");
    for (num, board) in crate::translate::BOARDS.iter().enumerate() {
        if num != 0 {
            out.push_str(",\n");
        }
        let xlate = crate::translate::get_translation(board);
        let mut codes = String::new();
        for scan in 0..SCAN_MAP.len() as u8 {
            if scan != 0 {
                codes.push_str(", ");
            }
            codes.push_str(&xlate(scan).to_string());
        }
        out.push_str(&format!("    {}: [{}]", quote(board), codes));
    }
    out.push('\n');
    out.push_str("  }");
    out
}

/// One chord table, in table order.
fn variant_json(table: &[Entry]) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("      \"count\": {},\n", table.len()));
    out.push_str("      \"chords\": [\n");
    for (num, entry) in table.iter().enumerate() {
        if num != 0 {
            out.push_str(",\n");
        }
        out.push_str(&format!(
            "        {{ \"code\": {}, \"code_hex\": \"0x{:03x}\", \"keys\": [{}], \
             \"action\": {} }}",
            entry.code,
            entry.code,
            key_names(entry.code),
            action_json(&entry.action),
        ));
    }
    out.push_str("\n      ]\n");
    out.push_str("    }");
    out
}

/// The names of the keys making up a chord code, in bit order.
fn key_names(code: u16) -> String {
    let mut out = String::new();
    for (bit, name) in BIT_NAMES.iter().enumerate() {
        if code & (1 << bit) != 0 {
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(&quote(name));
        }
    }
    out
}

/// One table entry's action.
///
/// `types` is what the chord puts on the screen, where that is a printable
/// character or a string; it is absent for actions with no textual result.  It
/// is derived by inverting the same table [`crate::usb_typer`] types strings
/// with, so it cannot disagree with what the keyboard actually sends.
fn action_json(action: &Action) -> String {
    match action {
        Action::Simple(key) => format!(
            "{{ \"kind\": \"key\", \"key\": {}, \"usage\": {}{} }}",
            quote(&format!("{:?}", key)),
            u8::from(*key),
            types_field(*key, false),
        ),
        Action::Shifted(key) => format!(
            "{{ \"kind\": \"shifted\", \"key\": {}, \"usage\": {}{} }}",
            quote(&format!("{:?}", key)),
            u8::from(*key),
            types_field(*key, true),
        ),
        Action::Text(text) => format!(
            "{{ \"kind\": \"text\", \"text\": {}, \"types\": {} }}",
            quote(text),
            quote(text),
        ),
        Action::OneShot(mods) => format!(
            "{{ \"kind\": \"oneshot\", \"mods\": [{}] }}",
            mod_names(*mods)
        ),
        Action::Release => "{ \"kind\": \"release\" }".to_string(),
    }
}

/// The `, "types": "x"` fragment for a key, or the empty string when the key
/// has no printable character.
fn types_field(key: crate::Keyboard, shifted: bool) -> String {
    match char_for_key(key, shifted) {
        Some(ch) => format!(", \"types\": {}", quote(&ch.to_string())),
        None => String::new(),
    }
}

/// The printable character a key types, with or without shift.
///
/// Found by inverting [`crate::usb_typer::key_for_char`] over printable ASCII,
/// so the answer is whatever the firmware would really send.  Keys with no
/// printable character -- the arrows, the function keys, enter -- have none.
pub fn char_for_key(key: crate::Keyboard, shifted: bool) -> Option<char> {
    let want = if shifted { Mods::SHIFT } else { Mods::empty() };
    (' '..='~').find(|ch| crate::usb_typer::key_for_char(*ch) == Some((key, want)))
}

/// The modifiers in a set, in a fixed order.
fn mod_names(mods: Mods) -> String {
    let mut out = String::new();
    for (flag, name) in [
        (Mods::CONTROL, "control"),
        (Mods::SHIFT, "shift"),
        (Mods::ALT, "alt"),
        (Mods::GUI, "gui"),
    ] {
        if mods.contains(flag) {
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(&quote(name));
        }
    }
    out
}

fn side_name(side: crate::Side) -> &'static str {
    match side {
        crate::Side::Left => "left",
        crate::Side::Right => "right",
    }
}

/// A JSON string literal.  The tables only hold printable ASCII, but escaping
/// properly costs nothing and keeps a future entry from producing a broken
/// document.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// The bit a key name refers to, or `None` if nothing is called that.
///
/// The lookup is case sensitive, matching [`BIT_NAMES`]: the finger keys are
/// lower case and the two thumbs are `Sp` and `Bk`.
pub fn bit_for_name(name: &str) -> Option<usize> {
    BIT_NAMES.iter().position(|n| *n == name)
}

/// The names of the keys making up a chord code, joined with `+`.
///
/// The inverse of [`code_for_names`], and the spelling the log format and the
/// drills use.
pub fn name_for_code(code: u16) -> String {
    let mut out = String::new();
    for (bit, name) in BIT_NAMES.iter().enumerate() {
        if code & (1 << bit) != 0 {
            if !out.is_empty() {
                out.push('+');
            }
            out.push_str(name);
        }
    }
    out
}

/// The chord code for a `+`-joined list of key names, or `None` if any of them
/// is not a key name.
pub fn code_for_names(names: &str) -> Option<u16> {
    let mut code = 0;
    for name in names.split('+') {
        code |= 1 << bit_for_name(name)?;
    }
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::{bit_for_name, code_for_names, layouts_json, name_for_code, BIT_NAMES};

    /// Names round trip through a chord code.
    #[test]
    fn test_name_round_trip() {
        for code in 1..0x400u16 {
            let names = name_for_code(code);
            assert_eq!(code_for_names(&names), Some(code), "code {code:#05x}");
        }
        assert_eq!(bit_for_name("nope"), None);
        assert_eq!(code_for_names("a+nope"), None);
        for (bit, name) in BIT_NAMES.iter().enumerate() {
            assert_eq!(bit_for_name(name), Some(bit));
        }
    }

    /// The document parses as JSON.
    ///
    /// There is no JSON parser in this crate's dependencies, so this is a
    /// structural sanity check rather than a real parse: braces and brackets
    /// balance, and every quote is closed.
    #[test]
    fn test_json_balanced() {
        let text = layouts_json();
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        for ch in text.chars() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    in_string = false;
                }
                continue;
            }
            match ch {
                '"' => in_string = true,
                '{' | '[' => depth += 1,
                '}' | ']' => depth -= 1,
                _ => (),
            }
            assert!(depth >= 0, "unbalanced close in layouts.json");
        }
        assert!(!in_string, "unterminated string in layouts.json");
        assert_eq!(depth, 0, "unbalanced layouts.json");
    }
}
