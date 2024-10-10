//! Board information.
//!
//! This structure is encoded into a fixed page for each device, and
//! contains information about the device and particulars about the
//! configuration.

extern crate alloc;

use alloc::string::String;
use minicbor::{Decode, Encode};

use core::{fmt::Debug, slice::from_raw_parts};

use crate::Side;
use crate::log::warn;

/// The side indicator.
///
/// This is related to, but not the same as the "Side" in the keyboard
/// crate.

/// Unchanging information about a particular board.
///
/// This is information about the current board.  At this time, these are
/// stored at a fixed offset in flash [`BOARD_INFO_OFFSET`].
#[derive(Debug, Encode, Decode)]
#[cbor(tag(0x626f617264696e66))]
#[cbor(map)]
pub struct BoardInfo {
    /// The name of this board.
    #[n(1)]
    pub name: String,

    /// Which side this board occupies.
    ///
    /// If `Some(Left)` or `Some(Right)` it indicates this is a split
    /// design where the two halves have their own MCU.  `None` indicates
    /// either a non-split design, or one where a single MCU handles both
    /// sides.
    #[n(2)]
    pub side: Option<Side>,
}

pub const BOARDINFO_TAG: u64 = 0x626f617264696e66;

/*
#[cfg(feature = "std")]
mod impls {
    use super::{BoardInfo, BOARDINFO_TAG};

    impl BoardInfo {
        pub fn encode<W: minicbor::encode::write::Write>(
            &self,
            writer: W,
        ) -> Result<(), minicbor::encode::Error<W::Error>> {
            minicbor::encode(self, writer)
        }
    }
}
*/

impl BoardInfo {
    /// Attempt to decode the board information from a given fixed address in memory.  
    ///
    /// This allocates a small buffer using an allocated vec, as needed by the cbor library.  This
    /// assumes there is a block of 256 bytes at the address.
    pub unsafe fn decode_from_memory(addr: *const u8) -> Option<BoardInfo> {
        let buffer: &[u8] = from_raw_parts(addr, 256);
        match minicbor::decode(buffer) {
            Ok(info) => Some(info),
            Err(e) => {
                warn!("Fail to read BoardInfo: {:?}", e);
                None
            }
        }
    }
}
