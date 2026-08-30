//! A cheap fingerprint of the layout tables.
//!
//! The event log in `taipo-teacher.md` phase 2 records raw key codes, and
//! everything interesting about them -- which chord they formed, on which
//! hand, what it typed -- is derived later by replaying them through these
//! tables.  A replay against tables the firmware no longer has is not
//! detectably wrong; it is quietly wrong, which is worse.  So the device
//! reports this value in `Reply::Hello`, the host compares it against the one
//! in `layouts.json`, and a mismatch is something the host can refuse to
//! derive from rather than something nobody notices.
//!
//! FNV-1a over a canonical encoding of the tables, not a cryptographic hash:
//! the question is "did these change", asked of a value the device computed
//! about itself.  There is no adversary, and the firmware should not be
//! carrying a SHA-256 for this.  64 bits is far more than enough for accidental
//! collisions between two versions of a hand-written table.
//!
//! This is `no_std` and allocation-free, so the firmware can compute it, which
//! is the point: it has to be the compiled-in tables that answer, not a
//! constant someone remembered to update.  `export::layouts_json` puts the same
//! number in `layouts.json`, and `tests/layouts_json.rs` is what keeps the two
//! in step.

use crate::translate::{get_translation, BOARDS};

use super::posh::POSH_ACTIONS;
use super::taipo::{Action, Entry, SCAN_MAP, TAIPO_ACTIONS};

/// Bumped if the encoding below changes, so that a fingerprint from an older
/// scheme cannot accidentally equal one from this scheme.
const SCHEME: u8 = 1;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a, one byte at a time.
struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Fnv(FNV_OFFSET)
    }

    fn byte(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    fn bytes(&mut self, bs: &[u8]) {
        for &b in bs {
            self.byte(b);
        }
    }

    fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    /// A length-prefixed run, so that concatenation is unambiguous: without
    /// this, two adjacent short tables could not be told from one long one.
    fn run(&mut self, bs: &[u8]) {
        self.u16(bs.len() as u16);
        self.bytes(bs);
    }
}

/// A fingerprint of every table a host needs in order to replay a key log.
///
/// Covers both chord tables, the scan map, and each board's scan code
/// translation.  It deliberately does *not* cover `CHORD_TIME`: that is
/// timing, not naming, and a replay driven from logged timestamps reproduces
/// whatever window the firmware had.  A future phase may want that too, in
/// which case bump [`SCHEME`].
pub fn layout_fingerprint() -> u64 {
    let mut h = Fnv::new();
    h.byte(SCHEME);

    for table in [TAIPO_ACTIONS, POSH_ACTIONS] {
        h.u16(table.len() as u16);
        for entry in table {
            entry_hash(&mut h, entry);
        }
    }

    h.u16(SCAN_MAP.len() as u16);
    for slot in SCAN_MAP.iter() {
        match slot {
            None => h.byte(0xff),
            Some((side, code)) => {
                h.byte(side.index() as u8);
                h.u16(*code);
            }
        }
    }

    h.u16(BOARDS.len() as u16);
    for name in BOARDS {
        h.run(name.as_bytes());
        let xlate = get_translation(name);
        // The scan code space a board can present is the key code space it
        // translates into; 255 marks a position with no key.
        for scan in 0..=u8::MAX {
            h.byte(xlate(scan));
        }
    }

    h.0
}

fn entry_hash(h: &mut Fnv, entry: &Entry) {
    h.u16(entry.code);
    match &entry.action {
        Action::Simple(k) => {
            h.byte(0);
            h.byte(u8::from(*k));
        }
        Action::Shifted(k) => {
            h.byte(1);
            h.byte(u8::from(*k));
        }
        Action::Text(s) => {
            h.byte(2);
            h.run(s.as_bytes());
        }
        Action::OneShot(m) => {
            h.byte(3);
            h.byte(m.bits());
        }
        Action::Release => h.byte(4),
    }
}

#[cfg(test)]
mod tests {
    use super::{layout_fingerprint, Fnv};

    /// The fingerprint is stable within a build, and not a degenerate value.
    #[test]
    fn test_stable() {
        let a = layout_fingerprint();
        assert_eq!(a, layout_fingerprint());
        assert_ne!(a, 0);
        assert_ne!(a, super::FNV_OFFSET);
    }

    /// FNV-1a against the published test vectors, so that a mistake in the
    /// hash itself shows up here rather than as an unexplained fingerprint
    /// change.
    #[test]
    fn test_fnv_vectors() {
        for (input, expect) in [
            ("", 0xcbf2_9ce4_8422_2325u64),
            ("a", 0xaf63_dc4c_8601_ec8c),
            ("foobar", 0x8594_4171_f739_67e8),
        ] {
            let mut h = Fnv::new();
            h.bytes(input.as_bytes());
            assert_eq!(h.0, expect, "FNV-1a of {input:?}");
        }
    }

    /// Length prefixing is what stops two tables from hashing the same as one.
    #[test]
    fn test_run_is_unambiguous() {
        let mut a = Fnv::new();
        a.run(b"ab");
        a.run(b"c");

        let mut b = Fnv::new();
        b.run(b"abc");

        assert_ne!(a.0, b.0);
    }
}
