//! Dump every translatable chord, one per line, for checking against the
//! Python model: `midi4text-analysis/check_rust.py` runs this.
//!
//! Each line is `left right before after text`, with the hands in hex and
//! the flags as 0 or 1.  The text is last and may be empty.

use bbq_orsy::chord::HAND_MASK;
use bbq_orsy::{translate, Chord};
use std::io::Write;

fn main() {
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for left in 0u16..0x400 {
        for right in 0u16..0x400 {
            let chord = Chord::new(left, right);
            if chord.is_empty() || (left | right) & !HAND_MASK != 0 {
                continue;
            }
            if let Some(t) = translate(chord) {
                writeln!(
                    out,
                    "{:03x} {:03x} {} {} {}",
                    left,
                    right,
                    t.space_before as u8,
                    t.space_after as u8,
                    t.text()
                )
                .unwrap();
            }
        }
    }
}
