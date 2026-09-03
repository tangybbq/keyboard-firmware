//! The key event log's record format.
//!
//! `taipo-teacher.md` phase 2.  Four bytes per record, fixed stride:
//!
//! ```text
//! byte 0   tag
//! byte 1   delta, low byte
//! byte 2   delta, high byte
//! byte 3   aux
//! ```
//!
//! The stride is fixed on purpose.  It makes the device's ring buffer trivial -- index it,
//! drop-oldest is a pointer bump, a partial drain needs no resynchronization -- where
//! variable-length records would need framing and a recovery story for each of those.
//!
//! # Why a delta rather than an uptime
//!
//! This is the load-bearing choice.  An absolute millisecond uptime narrowed into a record
//! wraps at 49.7 days, and a wrapped entry does not look wrong, it looks *recent*: every
//! consumer would then have to be written in terms of wrapping arithmetic with a "no two
//! points of interest are 49.7 days apart" precondition that the ring buffer itself can
//! violate.  A delta is bounded by human behaviour instead.  Consecutive keys are
//! milliseconds apart while typing and hours apart across sessions, and a saturated delta
//! is self-evidently an enormous gap rather than a plausible small number.  Overflow
//! becomes impossible by construction instead of handled correctly everywhere.
//!
//! # The delta's two units
//!
//! Sixteen bits of milliseconds would only reach 65 seconds, and an idle keyboard easily
//! exceeds that.  So bit 15 selects the unit: clear means the low 15 bits are milliseconds
//! (up to 32.7 s), set means they are seconds (up to 9.1 h).  Resolution is spent only on
//! gaps where nobody cares about the millisecond, the stride stays fixed, and no escape
//! record is needed.

use minicbor::{Decode, Encode};

/// Bytes per record.
pub const RECORD_SIZE: usize = 4;

/// The key code reported when a physical key has no code at all.
///
/// `translate.rs` returns 255 for a matrix position with no key, which does not fit the six
/// bits a record has for a code.  Such a press is still worth recording -- it says a key was
/// hit that does nothing -- so it is folded onto this value rather than dropped.
pub const CODE_NONE: u8 = 63;

/// The largest real key code a record can carry.
///
/// Codes run 0..47 across every board after translation, so this is roomy.
pub const CODE_MAX: u8 = 62;

/// A marker's kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Marker {
    /// The layout mode, as a [`ModeCode`].  Keystrokes made in a non-taipo mode are still
    /// logged, and this is what lets a reader exclude them.
    Mode = 0,
    /// The chord table: 0 for Taipo, 1 for Dosh.  Without this, a reader cannot attribute a
    /// chord to the table that was live when it happened, and the two layouts' statistics
    /// silently pool.
    Variant = 1,
    /// The row position on a three-row board: 0 upper, 1 lower.  Always 0 on the two-row
    /// boards, but the format does not care which board it came from.
    RowShift = 2,
    /// Logging was turned on.  A gap after a `Pause` is silence, not loss.
    Resume = 3,
    /// Logging was turned off.
    Pause = 4,
}

impl Marker {
    pub fn from_u8(v: u8) -> Option<Marker> {
        Some(match v {
            0 => Marker::Mode,
            1 => Marker::Variant,
            2 => Marker::RowShift,
            3 => Marker::Resume,
            4 => Marker::Pause,
            _ => return None,
        })
    }
}

/// The value a [`Marker::Mode`] record carries.
///
/// Deliberately its own numbering rather than `LayoutMode`'s discriminants: those shift with
/// the `qwerty` and `steno` cargo features, and a log has to mean the same thing whichever
/// firmware wrote it.
pub mod mode_code {
    pub const TAIPO: u8 = 0;
    pub const STENO: u8 = 1;
    pub const STENO_DIRECT: u8 = 2;
    pub const QWERTY: u8 = 3;
    pub const NKRO: u8 = 4;
}

/// The time since the previous record.
///
/// See the module documentation for why this is a delta, and why it has two units.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Encode, Decode)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cbor(transparent)]
pub struct Delta(#[n(0)] pub u16);

/// Deltas at or above this many milliseconds are recorded in seconds instead.
const MILLIS_LIMIT: u64 = 0x8000;

/// The largest value the seconds form can hold, about 9.1 hours.
const SECONDS_MAX: u64 = 0x7fff;

impl Delta {
    /// Narrow a millisecond gap into a record field, choosing the unit.
    ///
    /// Gaps beyond the seconds range saturate.  That is not a wrap: a saturated delta reads
    /// as "an enormous gap", which is the truth, and nothing downstream is misled.
    pub fn from_millis(ms: u64) -> Delta {
        if ms < MILLIS_LIMIT {
            Delta(ms as u16)
        } else {
            let secs = (ms / 1000).min(SECONDS_MAX);
            Delta(0x8000 | secs as u16)
        }
    }

    /// The gap in milliseconds, rounded to the recorded unit.
    pub fn as_millis(self) -> u64 {
        if self.0 & 0x8000 == 0 {
            self.0 as u64
        } else {
            (self.0 & 0x7fff) as u64 * 1000
        }
    }

    /// Whether this was recorded in seconds, and so is only accurate to one.
    pub fn is_coarse(self) -> bool {
        self.0 & 0x8000 != 0
    }

    /// Whether the gap was too large even for the seconds form.
    pub fn is_saturated(self) -> bool {
        self.0 == 0x8000 | SECONDS_MAX as u16
    }
}

/// What a record says happened.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Entry {
    /// A key went down or came up.
    ///
    /// The code is the key code *after* `translate.rs` and *before* the row shift, which is
    /// the space `SCAN_MAP` is indexed in.  That makes it board-independent, and directly
    /// replayable through the real layout engine.
    Key { code: u8, press: bool },
    /// Something about the engine's state changed.  See [`Marker`].
    Marker { marker: Marker, value: u8 },
}

/// One log record.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Record {
    pub delta: Delta,
    pub entry: Entry,
}

impl Record {
    pub fn key(delta: Delta, code: u8, press: bool) -> Record {
        Record {
            delta,
            entry: Entry::Key {
                code: if code > CODE_MAX { CODE_NONE } else { code },
                press,
            },
        }
    }

    pub fn marker(delta: Delta, marker: Marker, value: u8) -> Record {
        Record {
            delta,
            entry: Entry::Marker { marker, value },
        }
    }

    /// Pack into the four bytes that go on the wire.
    pub fn encode(&self) -> [u8; RECORD_SIZE] {
        let (tag, aux) = match self.entry {
            Entry::Key { code, press } => {
                let code = if code > CODE_MAX { CODE_NONE } else { code };
                ((press as u8) << 6 | code, 0)
            }
            Entry::Marker { marker, value } => (0x80 | marker as u8, value),
        };
        let [lo, hi] = self.delta.0.to_le_bytes();
        [tag, lo, hi, aux]
    }

    /// Unpack four bytes.  `None` for a marker kind this build does not know.
    ///
    /// An unknown marker is not corrupt data -- it is a newer firmware than the reader --
    /// so a reader should skip it and carry on rather than abandoning the stream.
    pub fn decode(bytes: [u8; RECORD_SIZE]) -> Option<Record> {
        let [tag, lo, hi, aux] = bytes;
        let delta = Delta(u16::from_le_bytes([lo, hi]));
        let entry = if tag & 0x80 == 0 {
            Entry::Key {
                code: tag & 0x3f,
                press: tag & 0x40 != 0,
            }
        } else {
            Entry::Marker {
                marker: Marker::from_u8(tag & 0x7f)?,
                value: aux,
            }
        };
        Some(Record { delta, entry })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_units() {
        // Milliseconds below the limit, exactly.
        for ms in [0u64, 1, 999, 0x7ffe, 0x7fff] {
            let d = Delta::from_millis(ms);
            assert!(!d.is_coarse(), "{ms} should be fine-grained");
            assert_eq!(d.as_millis(), ms);
        }

        // At the limit it switches to seconds and loses the remainder.
        let d = Delta::from_millis(0x8000);
        assert!(d.is_coarse());
        assert_eq!(d.as_millis(), 32_000);

        // An hour, and the largest representable gap.
        assert_eq!(Delta::from_millis(3_600_000).as_millis(), 3_600_000);
        let max = Delta::from_millis(SECONDS_MAX * 1000);
        assert!(max.is_saturated());

        // Beyond that it saturates rather than wrapping, which is the whole point.
        let huge = Delta::from_millis(u64::MAX);
        assert!(huge.is_saturated());
        assert_eq!(huge.as_millis(), SECONDS_MAX * 1000);
    }

    #[test]
    fn test_record_round_trip() {
        let cases = [
            Record::key(Delta::from_millis(0), 0, true),
            Record::key(Delta::from_millis(17), 47, false),
            Record::key(Delta::from_millis(3_600_000), CODE_MAX, true),
            Record::marker(Delta::from_millis(5), Marker::Variant, 1),
            Record::marker(Delta::from_millis(0), Marker::Mode, mode_code::STENO),
            Record::marker(Delta::from_millis(60_000), Marker::Pause, 0),
        ];
        for rec in cases {
            assert_eq!(Record::decode(rec.encode()), Some(rec), "{rec:?}");
        }
    }

    /// A key with no code is folded onto `CODE_NONE` rather than truncated into some other
    /// key's code, which would be a silent lie about which key was pressed.
    #[test]
    fn test_codeless_key() {
        let rec = Record::key(Delta::from_millis(1), 255, true);
        assert_eq!(
            rec.entry,
            Entry::Key {
                code: CODE_NONE,
                press: true
            }
        );
        assert_eq!(Record::decode(rec.encode()), Some(rec));
    }

    /// An unknown marker means a newer firmware, not corruption.
    #[test]
    fn test_unknown_marker_is_skippable() {
        assert_eq!(Record::decode([0x80 | 40, 0, 0, 0]), None);
        // And it does not disturb the records around it, because the stride is fixed.
        assert!(Record::decode([0x40, 1, 0, 0]).is_some());
    }

    /// Records are exactly four bytes, and the tag space does not overlap.
    #[test]
    fn test_tag_space() {
        let key = Record::key(Delta(0), 62, true).encode();
        let marker = Record::marker(Delta(0), Marker::Mode, 0).encode();
        assert_eq!(key.len(), RECORD_SIZE);
        assert_eq!(key[0] & 0x80, 0);
        assert_eq!(marker[0] & 0x80, 0x80);
    }
}
