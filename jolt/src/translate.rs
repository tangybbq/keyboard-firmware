//! Scancode translation.
//!
//! The bbq-keyboard scancodes are based on the "proto3" keyboard, which is
//! the largest keyboard I've built.  Other boards may have fewer keys, or
//! different scancodes.  This module provides a translation for scancodes
//! that is based on a Kconfig value.

pub fn get_translation(board: &str) -> fn(u8) -> u8 {
    match board {
        "proto3" => id,
        "proto4" => proto4,
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
