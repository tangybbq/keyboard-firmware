//! Board information.
//!
//! This structure is encoded into a fixed page for each device, and
//! contains information about the device and particulars about the
//! configuration.

extern crate alloc;

use alloc::string::String;
use alloc::vec;

use serde::{Deserialize, Serialize};
use core::{fmt::Debug, slice::from_raw_parts};

use ciborium::tag::Required;

use crate::Side;

/// The side indicator.
///
/// This is related to, but not the same as the "Side" in the keyboard
/// crate.

/// Unchanging information about a particular board.
///
/// This is information about the current board.  At this time, these are
/// stored at a fixed offset in flash [`BOARD_INFO_OFFSET`].
#[derive(Debug, Serialize, Deserialize)]
pub struct BoardInfo {
    /// The name of this board.
    pub name: String,

    /// Which side this board occupies.
    ///
    /// If `Some(Left)` or `Some(Right)` it indicates this is a split
    /// design where the two halves have their own MCU.  `None` indicates
    /// either a non-split design, or one where a single MCU handles both
    /// sides.
    pub side: Option<Side>,
}

pub const BOARDINFO_TAG: u64 = 0x626f617264696e66;

#[cfg(feature = "std")]
mod impls {
    use ciborium::tag::Required;

    use super::{BoardInfo, BOARDINFO_TAG};
    use std::fmt::Debug;

    impl BoardInfo {
        pub fn encode<W: ciborium_io::Write>(
            &self,
            writer: W,
        ) -> Result<(), ciborium::ser::Error<W::Error>>
            where W::Error: Debug,
        {
            let tagged: Required<_, BOARDINFO_TAG> = Required(self);

            ciborium::into_writer(&tagged, writer)
        }
    }
}

impl BoardInfo {
    pub fn decode<R: ciborium_io::Read>(
        reader: R,
        scratch_buffer: &mut [u8],
    ) -> Result<BoardInfo, ciborium::de::Error<R::Error>>
        where R::Error: Debug,
    {
        let tagged: Required<_, BOARDINFO_TAG> = ciborium::from_reader_with_buffer(reader, scratch_buffer)?;
        Ok(tagged.0)
    }

    /// Attempt to decode the board information from a given fixed address in memory.  
    ///
    /// This allocates a small buffer using an allocated vec, as needed by the cbor library.  This
    /// assumes there is a block of 256 bytes at the address.
    pub unsafe fn decode_from_memory(addr: *const u8) -> Option<BoardInfo> {
        let mut scratch = vec![0u8; 256];

        let buffer: &[u8] = from_raw_parts(addr, 256);
        Self::decode(buffer, &mut scratch).ok()
    }
}
