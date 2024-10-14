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

static PROTO4: [u8; 30] = [
    // 0
    13,    // L-F1
    14,    // L-F2
    11,    // L-Star
    11+15, // R-T
    14+15, // R-Z
    // 5
    13+15, // R-D
    12,    // L-S
    9,     // L-T
    10,    // L-K
    10+15, // R-G
    // 10
    9+15,  // R-L
    12+15, // R-S
    8,     // L-P
    7,     // L-W
    6,     // L-H
    // 15
    6+15,  // R-F
    7+15,  // R-B
    8+15,  // R-P
    5,     // L-R
    3,     // L-S1
    // 20
    4,     // L-S2
    4+15,  // R-S4
    3+15,  // R-S3
    5+15,  // R-R
    2,     // L-num
    // 25
    1,     // L-A
    0,     // L-O
    0+15,  // R-E
    1+15,  // R-U
    2+15,  // R-Num
];

fn proto4(code: u8) -> u8 {
    *PROTO4.get(code as usize).unwrap_or(&255)
}
