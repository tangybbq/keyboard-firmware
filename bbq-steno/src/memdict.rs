//! Memory dictionaries.
//!
//! A memory dictionary is a steno dictionary that is stored in a compact format
//! in flash memory. These will be accessed directly.
//!
//! Note that we will treat these as static lifetime. Testing might use
//! temporary arrays, and it is important to make sure they aren't moved.

use crate::stroke::Stroke;

pub const MAGIC1: &[u8] = b"stenodct";

/// This structure encodes the above. It is intended to be able to process the
/// directly-mapped structure, and as such, doesn't use pointers, but offsets.
#[repr(C)]
#[derive(Debug)]
pub struct RawMemDict {
    magic: [u8; 8],
    /// Number of entries in this dictionary.
    size: u32,
    /// Byte position (relative to this header) of the keys.
    keys_offset: u32,
    keys_length: u32,
    /// Byte position of the key table.
    key_pos_offset: u32,
    /// Byte offset of the text block.
    text_offset: u32,
    text_length: u32,
    /// Byte offset of the text table.
    text_table_offset: u32,
}

/// The saner MemDict representation. This holds the above header, and some more
/// friendly information and has methods for better accessing the structure.
pub struct MemDict {
    /// The raw header.
    pub raw: &'static RawMemDict,
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
    pub unsafe fn from_raw_ptr(ptr: *const u8) -> Option<MemDict> {
        let raw = &*(ptr as *const RawMemDict);
        if raw.magic != MAGIC1 {
            return None;
        }

        let keys =
            core::slice::from_raw_parts(ptr.add(raw.keys_offset as usize) as *const Stroke,
                                        raw.keys_length as usize);
        let key_offsets =
            core::slice::from_raw_parts(ptr.add(raw.key_pos_offset as usize) as *const u32,
                                        raw.size as usize);
        let text =
            core::slice::from_raw_parts(ptr.add(raw.text_offset as usize) as *const u8,
                                        raw.text_length as usize);
        let text_offsets =
            core::slice::from_raw_parts(ptr.add(raw.text_table_offset as usize) as *const u32,
                                        raw.size as usize);

        Some(MemDict { raw, keys, key_offsets, text, text_offsets })
    }

    /// Get a given key by index.  Panics if the key is out of range.
    pub fn get_key(&self, n: usize) -> &'static [Stroke] {
        let code = self.key_offsets[n] as usize;
        let offset = code & ((1 << 24) - 1);
        let length = code >> 24;
        &self.keys[offset .. offset + length]
    }

    /// Get the text. Panics if the key is out of range.
    pub fn get_text(&self, n: usize) -> &'static str {
        let code = self.text_offsets[n] as usize;
        let offset = code & ((1 << 24) - 1);
        let length = code >> 24;
        // println!("get text:{} (raw:{:x}) offset:{:x} len:{}", n, code, offset, length);
        let raw = &self.text[offset .. offset + length];
        unsafe { core::str::from_utf8_unchecked(raw) }
    }

    /// Lookup a sequence of steno in the dictionary.
    /// TODO: This is only an exact lookup, and doesn't really handle the case
    /// of extra strokes. I think this is fine for the Plover algorithm, though.
    pub fn lookup(&self, key: &[Stroke]) -> Option<&'static str> {
        match self.key_offsets.binary_search_by_key(&key, |k| {
            let code = *k as usize;
            let offset = code & ((1 << 24) - 1);
            let length = code >> 24;
            &self.keys[offset .. offset + length]
        }) {
            Ok(pos) => Some(self.get_text(pos)),
            Err(_) => None,
        }
    }
}
