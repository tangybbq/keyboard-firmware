//! Keys on my keyboards
//!
//! There have been a few different layouts of keys across my keyboards, the primary difference
//! being 42-key keyboards, with 3 rows of finger keys, and the 30-key variants with 2 rows of
//! finger keys.
//!
//! Instead of having different scan codes for these, we will treat them with the same codes, just
//! with the lower alphabetic row missing on the 2 row keyboard.

/// All of the scancodes fit within this.
///
/// The key mapping comes from the matrix layout on the "proto3", which was a full 42-key layout.
/// Other keyboards should have a mapping table to translate to this layout.
pub const NKEYS: usize = 48;

// First are the qwerty names.
pub const KEY_GRAVE: usize = 0;
pub const KEY_ESC: usize = 1;
pub const KEY_FUNC: usize = 2;

pub const KEY_Q: usize = 4;
pub const KEY_A: usize = 5;
pub const KEY_Z: usize = 6;

pub const KEY_W: usize = 8;
pub const KEY_S: usize = 9;
pub const KEY_X: usize = 10;

pub const KEY_E: usize = 12;
pub const KEY_D: usize = 13;
pub const KEY_C: usize = 14;
pub const KEY_LBR: usize = 15;

pub const KEY_R: usize = 16;
pub const KEY_F: usize = 17;
pub const KEY_V: usize = 18;
pub const KEY_TAB: usize = 19;

pub const KEY_T: usize = 20;
pub const KEY_G: usize = 21;
pub const KEY_B: usize = 22;
pub const KEY_DEL: usize = 23;

pub const KEY_MINUS: usize = 24;
pub const KEY_APOST: usize = 25;
pub const KEY_EQUAL: usize = 26;

pub const KEY_P: usize = 28;
pub const KEY_SEMI: usize = 28;
pub const KEY_SLASH: usize = 30;

pub const KEY_O: usize = 32;
pub const KEY_L: usize = 33;
pub const KEY_DOT: usize = 34;

pub const KEY_I: usize = 36;
pub const KEY_K: usize = 37;
pub const KEY_COMMA: usize = 38;
pub const KEY_RBR: usize = 39;

pub const KEY_U: usize = 40;
pub const KEY_J: usize = 41;
pub const KEY_M: usize = 42;
pub const KEY_ENTER: usize = 43;

pub const KEY_Y: usize = 44;
pub const KEY_H: usize = 45;
pub const KEY_N: usize = 46;
pub const KEY_SPACE: usize = 47;

// Steno names for the keys.
pub const KEY_ST_LNUM: usize = 1;
pub const KEY_ST_L_STAR: usize = 4;
pub const KEY_ST_L_S: usize = 5;
pub const KEY_ST_N1: usize = 6;
pub const KEY_ST_LP: usize = 8;
pub const KEY_ST_LW: usize = 9;
pub const KEY_ST_N2: usize = 10;
