//! Scancode translation.
//!
//! The bbq-keyboard scancodes are based on the "proto3" keyboard, which is
//! the largest keyboard I've built.  Other boards may have fewer keys, or
//! different scancodes.  This module provides a translation for scancodes
//! that is based on a Kconfig value.

pub fn get_translation(board: &str) -> fn (u8) -> u8 {
    match board {
        "proto3" => id,
        "proto4" => proto4,
        "jolt1" => id,
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
    2,     // L-Grave
    1,     // L-Fn
    4,     // L-Star
    28,    // R-T
    25,    // R-Z
    // 5
    24,    // R-D
    5,     // L-S
    8,     // L-T
    9,     // L-K
    33,    // R-G
    // 10
    32,    // R-L
    29,    // R-S
    12,    // L-P
    13,    // L-W
    16,    // L-H
    // 15
    40,    // R-F
    37,    // R-B
    36,    // R-P
    17,    // L-R
    20,    // L-S1
    // 20
    21,    // L-S2
    45,    // R-S4
    44,    // R-S3
    41,    // R-R
    18,    // L-num
    // 25
    19,    // L-A
    23,    // L-O
    47,    // R-E
    43,    // R-U
    42,    // R-Num
];

fn proto4(code: u8) -> u8 {
    *PROTO4.get(code as usize).unwrap_or(&255)
}
