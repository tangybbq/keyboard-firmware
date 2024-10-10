//! Memory dictionaries.
//!
//! A memory dictionary is a steno dictionary that is stored in a compact format
//! in flash memory. These will be accessed directly.
//!
//! Note that we will treat these as static lifetime. Testing might use
//! temporary arrays, and it is important to make sure they aren't moved.

extern crate alloc;

use core::slice::from_raw_parts;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use minicbor::{Decode, Encode};

use crate::{stroke::Stroke, dict::{DictImpl, Selector, BinarySelector}};
use log::warn;

pub const DICT_TAG: u64 = 0x7374656e6f646374;
pub const GROUP_TAG: u64 = 0x7374656e6f6d6c74;

/// Size allowed for the header, needs to incorporate the largest number of
/// groups used.
pub const HEADER_MAX_BYTES: usize = 512;

/// The Cbor struct for each dictionary.
///
/// This will be converted to the `MemDict` upon loading.  All of the
/// offsets are relative to the beginning of the mapped region (where the
/// CBOR starts).
#[derive(Default, Debug, Encode, Decode)]
#[cbor(tag(0x7374656e6f646374))]
pub struct RawMemDict {
    /// Number of entries in this dictionary.
    #[n(0)]
    pub size: u32,
    /// Byte position (relative to the overall dictionary header) of the keys.
    #[n(1)]
    pub keys_offset: u32,
    #[n(2)]
    pub keys_length: u32,
    /// Byte position of the key table.
    #[n(3)]
    pub key_pos_offset: u32,
    /// Byte offset of the text block.
    #[n(4)]
    pub text_offset: u32,
    #[n(5)]
    pub text_length: u32,
    /// Byte offset of the text table.
    #[n(6)]
    pub text_table_offset: u32,
}

/// This structure encodes multiple dictionaries.  We just define a fixed
/// number that are allowed.
#[derive(Debug, Encode, Decode)]
#[cbor(tag(0x7374656e6f6d6c74))]
pub struct RawDictGroup {
    #[n(0)]
    pub dicts: Vec<RawMemDict>,
}

/// The saner MemDict representation. This holds the above header, and some more
/// friendly information and has methods for better accessing the structure.
pub struct MemDict {
    /// The raw header.
    pub raw: RawMemDict,
    /// The keys are just an array of strokes in memory.
    pub keys: &'static [Stroke],
    /// The key table store in an encoded manner, the key element.
    pub key_offsets: &'static [u32],
    /// The text.
    pub text: &'static [u8],
    /// The text offset table.
    pub text_offsets: &'static [u32],
}

// TODO: Come up with error handling.

impl MemDict {
    /// Attempt to decode memory dictionaries at the given base.
    ///
    /// The dictionaries present can be either a single dictionary, or a
    /// group of them defined above.  If anything goes wrong, this returns
    /// an empty Vector, indicating no dictionaries are found.
    pub unsafe fn from_raw_ptr(ptr: *const u8) -> Vec<MemDict> {
        let header: &[u8] = from_raw_parts(ptr, HEADER_MAX_BYTES);

        // If this is a single dictionary, use that.
        let single: Result<RawMemDict, _> = minicbor::decode(header);
        if let Ok(single) = single {
            warn!("single: {:#?}", single);
            return Self::decode_single(ptr, single).into_iter().collect();
        }

        // Try decoding as a group.
        let group: Result<RawDictGroup, _> = minicbor::decode(header);
        if let Ok(group) = group {
            let mut result = Vec::new();
            warn!("group: {:#?}", group);
            for elt in group.dicts {
                if let Some(dict) = Self::decode_single(ptr, elt) {
                    result.push(dict);
                } else {
                    // Any error indicates we should not use anything from
                    // the dicts.
                    return Vec::new();
                }
            }

            return result;
        }

        Vec::new()
    }

    unsafe fn decode_single(ptr: *const u8, raw: RawMemDict) -> Option<MemDict> {
        // println!("single: {:#x?}", raw);
        let keys = core::slice::from_raw_parts(
            ptr.add(raw.keys_offset as usize) as *const Stroke,
            raw.keys_length as usize,
        );
        let key_offsets = core::slice::from_raw_parts(
            ptr.add(raw.key_pos_offset as usize) as *const u32,
            raw.size as usize,
        );
        let text = core::slice::from_raw_parts(
            ptr.add(raw.text_offset as usize) as *const u8,
            raw.text_length as usize,
        );
        let text_offsets = core::slice::from_raw_parts(
            ptr.add(raw.text_table_offset as usize) as *const u32,
            raw.size as usize,
        );

        Some(MemDict {
            raw,
            keys,
            key_offsets,
            text,
            text_offsets,
        })
    }
}

impl DictImpl for MemDict {
    fn len(&self) -> usize {
        self.key_offsets.len()
    }

    /// Get a given key by index.  Panics if the key is out of range.
    fn key(&self, n: usize) -> &'static [Stroke] {
        let code = self.key_offsets[n] as usize;
        let offset = code & ((1 << 24) - 1);
        let length = code >> 24;
        &self.keys[offset..offset + length]
    }

    /// Get the text. Panics if the key is out of range.
    fn value(&self, n: usize) -> &'static str {
        let code = self.text_offsets[n] as usize;
        let offset = code & ((1 << 24) - 1);
        let length = code >> 24;
        // println!("get text:{} (raw:{:x}) offset:{:x} len:{}", n, code, offset, length);
        let raw = &self.text[offset..offset + length];
        unsafe { core::str::from_utf8_unchecked(raw) }
    }

    fn selector(self: Rc<Self>) -> Box<dyn Selector> {
        Box::new(BinarySelector::new(self))
    }
}

/*
impl Dict for MemDict {
    /// Lookup a sequence of steno in the dictionary.
    /// TODO: This is only an exact lookup, and doesn't really handle the case
    /// of extra strokes. I think this is fine for the Plover algorithm, though.
    fn lookup(&self, key: &[Stroke]) -> Option<&'static str> {
        match self.key_offsets.binary_search_by_key(&key, |k| {
            let code = *k as usize;
            let offset = code & ((1 << 24) - 1);
            let length = code >> 24;
            &self.keys[offset..offset + length]
        }) {
            Ok(pos) => Some(self.get_text(pos)),
            Err(_) => None,
        }
    }

    /// Lookup a sequence in the steno dictionary.  Similar to `lookup()` but
    /// will return success if the matched string only returns a prefix of the
    /// input.  As such, the return result is a bit richer, as it returns the
    /// number of strokes in the match.
    fn prefix_lookup(&self, query: &[Stroke]) -> Option<(usize, &'static str)> {
        // The best result we've seen so far, as an offset.
        let mut best = None;

        // How many strokes of the query we are searching for.
        let mut used = 1;

        // Starting position for the search.  Once we find a prefix in the
        // dictionary, it is no longer necessary to search any entries before
        // this.
        let mut start = 0;

        // Perform the search of a given prefix.
        loop {
            let subdict = &self.key_offsets[start..];
            let subquery = &query[0..used];
            match subdict.binary_search_by_key(&subquery, |k| {
                let code = *k as usize;
                let offset = code & ((1 << 24) - 1);
                let length = code >> 24;
                &self.keys[offset..offset + length]
            }) {
                Ok(pos) => {
                    let pos = start + pos;

                    // This matches, so consider it a potential candidate.
                    // Longer results will replace this.
                    best = Some(pos);

                    // If we have searched our entire query, this is our best
                    // result.
                    if used == query.len() {
                        break;
                    }

                    // Otherwise, try longer searches to see if we can find a
                    // longer match.  We don't need to search for the current
                    // entry, as it is an exact match.
                    start = pos + 1;
                    used += 1;
                }
                Err(pos) => {
                    let pos = start + pos;

                    // If we have searched our entire query, we have our best
                    // result.
                    if used == query.len() {
                        break;
                    }

                    // If this input stroke is after all existing entries, there
                    // is nothing more to search for.
                    if pos >= self.key_offsets.len() {
                        break;
                    }

                    // Nothing matches, but we are at the place this entry would
                    // be inserted.  If the prefix does indeed match, then we
                    // can look for more strokes.
                    if self.get_key(pos).starts_with(subquery) {
                        // Start here, since the longer query could match this
                        // entry.
                        start = pos;
                        // But search for an additional stroke.
                        used += 1;
                    } else {
                        // There aren't any more possible matches, so return
                        // whatever best result we've seen so far.
                        break;
                    }
                }
            }
        }

        best.map(|pos| {
            let key = self.get_key(pos);
            let text = self.get_text(pos);
            (key.len(), text)
        })
    }

    fn longest_key(&self) -> usize {
        (0..self.key_offsets.len()).map(|i| self.get_key(i).len()).max().unwrap_or(0)
    }
}
*/
