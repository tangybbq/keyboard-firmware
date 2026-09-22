//! Scancode translation.
//!
//! The bbq-keyboard scancodes are based on the "proto3" keyboard, which is
//! the largest keyboard I've built.  Other boards may have fewer keys, or
//! different scancodes.  This module provides a translation for scancodes
//! that is based on the board name from the board info block.
//!
//! The tables live here, rather than in a firmware crate, so that host tools
//! can resolve a scan code the same way the firmware does.  Nothing in here
//! needs `std`, and the translation is a plain function per board.

/// Every board name [`get_translation`] accepts.
///
/// The firmware only ever asks for the one name in its board info block; this
/// is here so that host tools can walk every board without hardcoding the
/// list.
pub const BOARDS: &[&str] = &[
    "proto3", "proto4", "mesa1", "mesa2", "mesa2b", "mesa3", "mesa3b",
    "jolt1", "jolt2", "jolt3",
];

pub fn get_translation(board: &str) -> fn(u8) -> u8 {
    match board {
        "proto3" => id,
        "proto4" => proto4,
        "mesa1" => mesa1,
        "mesa2" => mesa2,
        "mesa2b" => mesa2b,
        "mesa3" => mesa3,
        "mesa3b" => mesa3b,
        "jolt1" => id,
        "jolt2" => jolt2,
        "jolt3" => jolt3,
        xlate => panic!("Unsupported translation table {:?}", xlate),
    }
}

fn id(code: u8) -> u8 {
    code
}

/// The Proto4 is a 2-row keyboard.  We used to have a separate set of scancodes for the 2 row
/// keyboards.  Instead, we use a separate attribute to enable/disable qwerty mode.  The key here is
/// that the upper left key becomes the "Fn" key so that the lower left can be "#".
/// The Proto4 layout is a bit chaotic, due to needing to get the inter connector to only need 8
/// pins. As such, each column typically will have some keys on each side.
static PROTO4: [u8; 30] = [
    // 0
    2,  // L-Grave
    1,  // L-Fn
    4,  // L-Star
    28, // R-T
    25, // R-Z
    // 5
    24, // R-D
    5,  // L-S
    8,  // L-T
    9,  // L-K
    33, // R-G
    // 10
    32, // R-L
    29, // R-S
    12, // L-P
    13, // L-W
    16, // L-H
    // 15
    40, // R-F
    37, // R-B
    36, // R-P
    17, // L-R
    20, // L-S1
    // 20
    21, // L-S2
    45, // R-S4
    44, // R-S3
    41, // R-R
    18, // L-num
    // 25
    19, // L-A
    23, // L-O
    47, // R-E
    43, // R-U
    42, // R-Num
];

fn proto4(code: u8) -> u8 {
    *PROTO4.get(code as usize).unwrap_or(&255)
}

/// The Mesa1 has the same 30 keys as the proto4, in the same places, with the same meanings.  Only
/// the matrix wiring differs: the mesa1 shares five driven lines (`ROW_A`..`ROW_E`) between the
/// halves and gives each half three sensed lines (`COL_1`..`COL_3` left, `COL_4`..`COL_6` right),
/// so the cable between the halves needs only 8 conductors.
///
/// Beware of the naming: the diodes conduct from column to row, so the scanner drives the mesa1's
/// *columns* and senses its *rows* (see the `mesa1` module in `board.rs`).  Scan codes therefore
/// run `COL_n * 5 + ROW_x`, six groups of five, and each group holds a column's two keys from each
/// physical row plus one thumb.
static MESA1: [u8; 30] = [
    // COL_1
    2,  // LF1, the mode ("Fn") key
    12, // L-P
    1,  // LF2, steno '#'
    13, // L-W
    18, // L-num
    // COL_2
    4,  // L-Star
    16, // L-H
    5,  // L-S
    17, // L-R
    19, // L-A
    // COL_3
    8,  // L-T
    20, // L-S1
    9,  // L-K
    21, // L-S2
    23, // L-O
    // COL_4
    24, // R-D
    36, // R-P
    25, // R-Z
    37, // R-B
    42, // R-Num
    // COL_5
    28, // R-T
    40, // R-F
    29, // R-S
    41, // R-R
    43, // R-U
    // COL_6
    32, // R-L
    44, // R-S3
    33, // R-G
    45, // R-S4
    47, // R-E
];

fn mesa1(code: u8) -> u8 {
    *MESA1.get(code as usize).unwrap_or(&255)
}

/// The Mesa2 has 20 keys, exactly the Taipo set and nothing else: no mode key, no row toggle, no
/// steno keys.  Every matrix position is a real key, which makes it the first board here with no
/// holes in its table.
///
/// Five sensed lines (`COL_1`..`COL_5`) are shared by both hands, and each hand gets two driven
/// lines (`ROW_A`/`ROW_B` left, `ROW_C`/`ROW_D` right).  The two hands number their columns in
/// opposite order -- left `COL_1`..`COL_4` is pinky to index, right `COL_1`..`COL_4` is index to
/// pinky -- so a given column carries the same key pair on both hands.  `COL_5` is the thumbs.
///
/// Beware of the naming, which is the mesa1's the other way round: on the mesa2 the diodes conduct
/// from row to column, so the scanner drives the board's *rows* and senses its *columns* (see the
/// `mesa2` module in `board.rs`).  Scan codes therefore run `ROW_x * 5 + COL_n`, four groups of
/// five, and each group is one hand's row plus that hand's thumb.
static MESA2: [u8; 20] = [
    // ROW_A, the left hand's far row
    4,  // L-r
    8,  // L-s
    12, // L-n
    16, // L-i
    19, // L-Sp
    // ROW_B, the left hand's near row
    5,  // L-a
    9,  // L-o
    13, // L-t
    17, // L-e
    23, // L-Bk
    // ROW_C, the right hand's far row
    40, // R-i
    36, // R-n
    32, // R-s
    28, // R-r
    47, // R-Bk
    // ROW_D, the right hand's near row
    41, // R-e
    37, // R-t
    33, // R-o
    29, // R-a
    43, // R-Sp
];

fn mesa2(code: u8) -> u8 {
    *MESA2.get(code as usize).unwrap_or(&255)
}

/// The Mesa3 is the Mesa2 Rev B matrix split across two halves: a Tiny 2040 on the left, a passive
/// right half, and seven of the RJ-45's eight conductors carrying the five columns and the right
/// half's two rows.  One MCU still scans the whole thing, so the split is invisible here -- the
/// scan codes are the same shape as the mesa2's.
///
/// Two things changed from the mesa2, and both are in this table rather than in the pin order:
///
/// - **The outer pinky `R` keys are gone.**  The pinky column carries one key per hand instead of
///   two, leaving `COL_1` empty on the right's far row.
/// - **The columns pair by finger on both hands.**  The mesa2 numbers its right-hand columns
///   backwards, so `COL_1` is the left's pinky and the right's index; here `COL_1` is the pinky on
///   both sides, `COL_2` the ring, `COL_3` the middle and `COL_4` the index.  `COL_5` is still the
///   thumbs.
///
/// The left half then puts a mode key in the slot its pinky `R` vacated (`SW_LFN1`, `COL_1` on
/// `ROW_A`), which is what distinguishes this table from [`MESA2B`].  It is the general mode key,
/// [`crate::layout::MODE_KEY`]; the mesa2 has no key to spare for one and picks its taipo variant
/// with a chord instead.  No fabricated board has this key yet.
///
/// The column nets also moved from `GP4`..`GP7` to `GP7`..`GP4`, but that is the board's business:
/// the `mesa3` module in `board.rs` names the pins in column order, so the scan codes still run
/// `ROW_x * 5 + COL_n`.
static MESA3: [u8; 20] = [
    // ROW_A, the left hand's far row
    2,  // L-Fn, the mode key, where the pinky R used to be
    8,  // L-s
    12, // L-n
    16, // L-i
    19, // L-Sp
    // ROW_B, the left hand's near row
    5,  // L-a
    9,  // L-o
    13, // L-t
    17, // L-e
    23, // L-Bk
    // ROW_C, the right hand's far row
    255, // no key: the right half has no mirror for the mode key
    32,  // R-s
    36,  // R-n
    40,  // R-i
    47,  // R-Bk
    // ROW_D, the right hand's near row
    29, // R-a
    33, // R-o
    37, // R-t
    41, // R-e
    43, // R-Sp
];

fn mesa3(code: u8) -> u8 {
    *MESA3.get(code as usize).unwrap_or(&255)
}

/// The Mesa3 Rev B, the first to be built with Fn keys: one per hand, each in the slot its hand's
/// pinky `R` vacated -- `SW_LFN1` on `COL_1` × `ROW_A` and `SW_RFN1` on `COL_1` × `ROW_C`.  Both
/// sit physically inboard of the near-row index key, and a trace carries each over to the pinky
/// column, so matrix position and physical position disagree.  Every other key is where it is on
/// the [`MESA3`].
///
/// The Fn keys are [`crate::layout::FN_LEFT`] and [`crate::layout::FN_RIGHT`], not the mode key:
/// they switch between Dosh and Orsy, and play one hand through Dosh while the other holds Fn, and
/// neither is what the mode key does.
static MESA3B: [u8; 20] = [
    // ROW_A, the left hand's far row
    48, // L-Fn
    8,  // L-s
    12, // L-n
    16, // L-i
    19, // L-Sp
    // ROW_B, the left hand's near row
    5,  // L-a
    9,  // L-o
    13, // L-t
    17, // L-e
    23, // L-Bk
    // ROW_C, the right hand's far row
    49, // R-Fn
    32, // R-s
    36, // R-n
    40, // R-i
    47, // R-Bk
    // ROW_D, the right hand's near row
    29, // R-a
    33, // R-o
    37, // R-t
    41, // R-e
    43, // R-Sp
];

fn mesa3b(code: u8) -> u8 {
    *MESA3B.get(code as usize).unwrap_or(&255)
}

/// The Mesa2 Rev B is the unibody the [`MESA3`] was cut from, and presents the same matrix minus
/// the mode key: 18 keys in 20 slots, with `COL_1` empty on both hands' far rows.
///
/// It is a separate table rather than a name sharing the mesa3's only because of that one slot.
/// If the Rev B ever gets its own `SW_LFN1` the two boards become indistinguishable here, and this
/// should collapse into [`MESA3`] rather than being kept in step by hand.
static MESA2B: [u8; 20] = [
    // ROW_A, the left hand's far row
    255, // no key: Rev B has no mode key, and the pinky R is gone
    8,   // L-s
    12,  // L-n
    16,  // L-i
    19,  // L-Sp
    // ROW_B, the left hand's near row
    5,  // L-a
    9,  // L-o
    13, // L-t
    17, // L-e
    23, // L-Bk
    // ROW_C, the right hand's far row
    255, // no key
    32,  // R-s
    36,  // R-n
    40,  // R-i
    47,  // R-Bk
    // ROW_D, the right hand's near row
    29, // R-a
    33, // R-o
    37, // R-t
    41, // R-e
    43, // R-Sp
];

fn mesa2b(code: u8) -> u8 {
    *MESA2B.get(code as usize).unwrap_or(&255)
}

/// The Jolt4 has a different scan order that puts the keys allnicely in order.
static JOLT4: [u8; 21] = [
    // The main part is just a span of 3 instead of 4.
    0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14, 16, 17, 18, 20, 21, 22,
    // And the thumbs are after this, but from right to left.
    23, 19, 15,
];

fn jolt2(code: u8) -> u8 {
    if (code as usize) < JOLT4.len() {
        JOLT4[code as usize]
    } else if code >= 24 && ((code - 24) as usize) < JOLT4.len() {
        let code = code as usize;

        // The thumb keys on the right side are reversed.
        let code = match code - 24 {
            18 => 20,
            20 => 18,
            code => code,
        } + 24;

        // The right half is mirrored on the left.  But, we shift by 24 for the right side.
        JOLT4[code - 24] + 24
    } else {
        255
    }
}

fn jolt3(code: u8) -> u8 {
    if (code as usize) < JOLT4.len() {
        JOLT4[code as usize]
    } else if code >= 24 && ((code - 24) as usize) < JOLT4.len() {
        let code = code as usize;

        // The thumb keys on the right side are reversed.
        let code = match code - 24 {
            18 => 20,
            20 => 18,
            code => code,
        } + 24;

        // The right half is mirrored on the left.  But, we shift by 24 for the right side.
        JOLT4[code - 24] + 24
    } else {
        255
    }
}

#[cfg(test)]
mod tests {
    use super::{get_translation, BOARDS};

    /// Every name in `BOARDS` is one `get_translation` knows, and every scan
    /// code a matrix can produce lands in the 0..48 key code space, on one of
    /// the Fn keys past it, or on 255 for a position with no key.
    ///
    /// The scan codes stop at 48 because that is the largest board: `proto3`
    /// and `jolt1` translate with the identity, which would happily pass a
    /// larger code straight through.
    #[test]
    fn test_boards_translate() {
        use crate::layout::{FN_LEFT, FN_RIGHT};

        for board in BOARDS {
            let xlate = get_translation(board);
            for code in 0..48u8 {
                let key = xlate(code);
                assert!(
                    key < 48 || key == FN_LEFT || key == FN_RIGHT || key == 255,
                    "board {board}: {code} -> {key}"
                );
            }
        }
    }

    /// The mesa2's 20 positions are exactly the 20 taipo keys, once each.
    ///
    /// This is the one thing about that table a transcription error would
    /// break silently: a wrong code would still be a valid key, and the board
    /// would simply type the wrong letter.  Asking `SCAN_MAP` makes the table
    /// answer for itself instead.
    #[test]
    #[cfg(feature = "proto3")]
    fn test_mesa2_is_the_taipo_keys() {
        use crate::layout::taipo::SCAN_MAP;

        let xlate = get_translation("mesa2");
        let mut seen: Vec<u8> = (0..20u8).map(xlate).collect();

        for key in &seen {
            assert!(
                SCAN_MAP[*key as usize].is_some(),
                "mesa2 scan maps to {key}, which is not a taipo key",
            );
        }

        // Every taipo key, once: no duplicates, and nothing left out.
        seen.sort();
        let mut want: Vec<u8> = (0..SCAN_MAP.len() as u8)
            .filter(|k| SCAN_MAP[*k as usize].is_some())
            .collect();
        want.sort();
        assert_eq!(seen, want);

        // And nothing past the matrix.
        assert_eq!(xlate(20), 255);
    }

    /// The mesa3 and the mesa2 Rev B carry the Dosh key set: every taipo key
    /// except the two outer pinky `R`s, which neither board has.
    ///
    /// Same reasoning as the mesa2 test above -- a mistranscribed code would
    /// still be a valid key and would simply type the wrong letter -- with the
    /// two empty matrix slots checked as well, since these are the first tables
    /// here with holes in them.
    #[test]
    #[cfg(feature = "proto3")]
    fn test_mesa3_is_the_dosh_keys() {
        use crate::layout::taipo::SCAN_MAP;
        use crate::layout::MODE_KEY;

        let mut want: Vec<u8> = (0..SCAN_MAP.len() as u8)
            .filter(|k| SCAN_MAP[*k as usize].is_some())
            // The pinky R keys, deleted from both boards.
            .filter(|k| *k != 4 && *k != 28)
            .collect();
        want.sort();

        for board in ["mesa3", "mesa2b"] {
            let xlate = get_translation(board);
            let mut seen: Vec<u8> = (0..20u8).map(xlate).filter(|k| *k != 255).collect();

            // The mesa3 alone has the mode key, which is not a taipo key, so it
            // has to come out before the rest can be compared.
            if board == "mesa3" {
                assert!(seen.contains(&MODE_KEY), "mesa3 has no mode key");
                seen.retain(|k| *k != MODE_KEY);
            }

            seen.sort();
            assert_eq!(seen, want, "board {board}");

            // The empty matrix slots, and nothing past the matrix.
            assert_eq!(xlate(10), 255, "board {board}");
            assert_eq!(xlate(20), 255, "board {board}");
        }

        // The one slot the two boards disagree on.
        assert_eq!(get_translation("mesa3")(0), MODE_KEY);
        assert_eq!(get_translation("mesa2b")(0), 255);
    }

    /// The mesa3b is the mesa3 with its mode key swapped for the left Fn key,
    /// and the right Fn key in the slot the mesa3 leaves empty.
    ///
    /// Written against the mesa3 table rather than against `SCAN_MAP`, so
    /// that the two cannot drift apart in the eighteen keys they share.
    #[test]
    fn test_mesa3b_is_the_mesa3_with_fn() {
        use crate::layout::{FN_LEFT, FN_RIGHT};

        let mesa3 = get_translation("mesa3");
        let mesa3b = get_translation("mesa3b");
        for code in 0..=20u8 {
            let want = match code {
                0 => FN_LEFT,
                10 => FN_RIGHT,
                code => mesa3(code),
            };
            assert_eq!(mesa3b(code), want, "scan code {code}");
        }
    }
}
