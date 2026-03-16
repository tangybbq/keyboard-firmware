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
//! which includes a few dead codes.

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
