//! Emit `layouts.json` from the real chord tables.
//!
//! The output is checked in as `bbq-keyboard/layouts.json`; regenerate it with
//!
//! ```sh
//! cd bbq-keyboard && cargo run --example gen-layouts > layouts.json
//! ```
//!
//! `tests/layouts_json.rs` fails when the checked-in file no longer matches
//! the tables, which is the whole point of checking it in.

fn main() {
    print!("{}", bbq_keyboard::layout::export::layouts_json());
}
