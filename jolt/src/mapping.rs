//! Mappings between various keyboards
//!
//! The Jolt keyboard code uses a somewhat unified set of mappings, defined in
//! the bbq-keyboard code. Eventually we will remove the "proto2" support, and
//! use the proto3 layout for everything. All other keyboards translate their
//! codes to this mapping.
//!
//! The proto3 layout is as follows:
//!    0  4  8 12 16 20    44 40 36 32 28 24
//!    1  5  9 13 17 21    45 41 37 33 29 25
//!    2  6 10 14 18 22    46 42 38 34 30 26
//!            15 19 23    47 43 39
//! which includes a few dead codes (3, 7, 11, etc. are unused).

/// The translation from the proto4 keyboard.  The proto4 layout is as follows:
///    0  2  7 12 14 19    22 15 17 10  3  5
///    1  6  8 13 18 20    21 23 16  9 11  4
///            24 25 26    27 28 29
///  Due to the numerous missing keys, we need to map the mode switch, which is
///  '2', to one of our pressable keys.
pub const PROTO4_MAPPING: [u8; 30] = [
    2,  // 0
    2,  // 1
    4,  // 2
    28, // 3
    25, // 4
    24, // 5
    5,  // 6
    8,  // 7
    9,  // 8
    33, // 9
    32, // 10
    29, // 11
    12, // 12
    13, // 13
    16, // 14
    40, // 15
    37, // 16
    36, // 17
    17, // 18
    20, // 19
    21, // 20
    45, // 21
    44, // 22
    41, // 23
    15, // 24
    19, // 25
    23, // 26
    47, // 27
    43, // 28
    39, // 29
];

/// The translation from the left half of a jolt3 keyboard.
///
/// The jolt3 is a split keyboard where each half runs its own firmware on an
/// RP2040. The matrix is 6 rows × 4 columns = 24 positions, of which 21 are
/// actual keys. Positions 21–23 are dead (no physical switch).
///
/// Scan order: code = col * 6 + row.  The key positions follow the same
/// column-first order as the archive JOLT4 mapping:
///    col0 (3 main rows, stagger): codes 0–5
///    col1:                        codes 6–11
///    col2:                        codes 12–17
///    col3 (+ 3 thumb keys):       codes 18–20
///
/// Dead positions (no physical key): 21, 22, 23 → 255.
///
/// NOTE: This is for the LEFT side only.  A right-side mapping will be added
/// when i2c inter-half communication is implemented and board_info.side is used.
pub const JOLT3_LEFT_MAPPING: [u8; 24] = [
    0,   // 0  → L col0 row0
    1,   // 1  → L col0 row1
    2,   // 2  → L col0 row2
    4,   // 3  → L col1 row0
    5,   // 4  → L col1 row1
    6,   // 5  → L col1 row2
    8,   // 6  → L col2 row0
    9,   // 7  → L col2 row1
    10,  // 8  → L col2 row2
    12,  // 9  → L col3 row0
    13,  // 10 → L col3 row1
    14,  // 11 → L col3 row2
    16,  // 12 → L col4 row0
    17,  // 13 → L col4 row1
    18,  // 14 → L col4 row2
    20,  // 15 → L col5 row0
    21,  // 16 → L col5 row1
    22,  // 17 → L col5 row2
    23,  // 18 → L thumb (rightmost)
    19,  // 19 → L thumb (middle)
    15,  // 20 → L thumb (leftmost)
    255, // 21 → dead
    255, // 22 → dead
    255, // 23 → dead
];
