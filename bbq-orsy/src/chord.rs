//! A chord: the keys of one stroke, and its division into the four Series.
//!
//! Each hand has nine keys, numbered as the Dosh table numbers them (the
//! upper pinky, `0x010`, is absent on the mesa3):
//!
//! | key            | bit     |
//! |----------------|---------|
//! | `a` pinky      | `0x001` |
//! | `o` ring lower | `0x002` |
//! | `t` mid lower  | `0x004` |
//! | `e` idx lower  | `0x008` |
//! | `s` ring upper | `0x020` |
//! | `n` mid upper  | `0x040` |
//! | `i` idx upper  | `0x080` |
//! | `Sp` thumb     | `0x100` |
//! | `Bk` thumb     | `0x200` |
//!
//! The four Series are positions in the syllable, and run left to right
//! across the board in the order the letters come out:
//!
//! ```text
//!     a o s t n   e i Sp Bk   |   e i Sp Bk   a o s t n
//!     Series 1    Series 2    |   Series 3    Series 4
//!     onset       second      |   vowel       coda
//! ```
//!
//! So the outer five keys of the left hand are the onset, the inner four the
//! second character, and mirrored on the right: the inner four are the vowel
//! and the outer five the coda.

/// The outer five keys of a hand: `a o s t n`.  Series 1 on the left, Series
/// 4 on the right.
pub const OUTER_MASK: u16 = 0x067;

/// The inner four keys of a hand: `e i Sp Bk`.  Series 2 on the left, Series
/// 3 on the right.
pub const INNER_MASK: u16 = 0x388;

/// Every key this layout knows about on one hand.
pub const HAND_MASK: u16 = OUTER_MASK | INNER_MASK;

/// The keys of one stroke, as a bitmask per hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Chord {
    pub left: u16,
    pub right: u16,
}

impl Chord {
    pub const fn new(left: u16, right: u16) -> Chord {
        Chord { left, right }
    }

    /// Whether any key is down.
    pub const fn is_empty(self) -> bool {
        self.left == 0 && self.right == 0
    }

    /// The Series 1 keys: the left hand's outer five.
    pub const fn series1(self) -> u16 {
        self.left & OUTER_MASK
    }

    /// The Series 2 keys: the left hand's inner four.
    pub const fn series2(self) -> u16 {
        self.left & INNER_MASK
    }

    /// The Series 3 keys: the right hand's inner four.
    pub const fn series3(self) -> u16 {
        self.right & INNER_MASK
    }

    /// The Series 4 keys: the right hand's outer five.
    pub const fn series4(self) -> u16 {
        self.right & OUTER_MASK
    }

    /// How many keys the chord uses.
    pub const fn keys(self) -> u32 {
        self.left.count_ones() + self.right.count_ones()
    }
}
