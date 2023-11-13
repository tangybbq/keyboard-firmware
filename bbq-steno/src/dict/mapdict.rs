//! Simple dictionary implemented with maps.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::{Dict, DictImpl};
use crate::Stroke;

extern crate alloc;

/// A loaded memory dictionary is similar to the compacted dictionary, but with
/// a bit more simple of an encoding.
pub struct RamDict {
    strokes: Vec<Stroke>,
    // The keys, each entry is a range in the strokes of the source.
    keys: Vec<(u32, u32)>,
    text: String,
    // This is, likewise, a selection for each result.
    values: Vec<(u32, u32)>,
}

impl DictImpl for RamDict {
    /// The number of entries in the dictionary.
    fn len(&self) -> usize {
        self.keys.len()
    }

    /// Return the given key.
    fn key(&self, index: usize) -> &[Stroke] {
        let (a, b) = self.keys[index];
        let a = a as usize;
        let b = b as usize;
        &self.strokes[a..b]
    }

    /// Return the given text.
    fn value(&self, index: usize) -> &str {
        let (a, b) = self.values[index];
        let a = a as usize;
        let b = b as usize;
        &self.text[a..b]
    }
}

/// A simple dictionary implementation.  This is implemented by storing Vecs of
/// the keys, and Strings as the values.
pub struct MapDict {
    map: BTreeMap<Vec<Stroke>, String>,
    longest: usize,
}

/// A dictionary builder.
pub struct MapDictBuilder {
    map: BTreeMap<Vec<Stroke>, String>,
}

impl Dict for MapDict {
    fn lookup<'a>(&'a self, query: &[Stroke]) -> Option<&'a str> {
        self.map.get(query).map(|s| s.as_ref())
    }

    fn longest_key(&self) -> usize {
        self.longest
    }
}

impl MapDict {
    /// Iterate over the keys in the dictionary.
    pub fn keys(&self) -> impl Iterator<Item = &[Stroke]> {
        self.map.keys().map(|k| k.as_slice())
    }
}

impl MapDictBuilder {
    pub fn new() -> MapDictBuilder {
        MapDictBuilder {
            map: BTreeMap::new(),
        }
    }

    /// Insert a definition.
    pub fn insert(&mut self, key: Vec<Stroke>, definition: String) {
        self.map.insert(key, definition);
    }

    /// Freeze the dictionary.
    pub fn into_map_dict(self) -> MapDict {
        let longest = self.map.keys().map(|k| k.len()).max().unwrap_or(0);
        MapDict {
            map: self.map,
            longest,
        }
    }

    /// Freeze into a RamDict.
    pub fn into_ram_dict(self) -> RamDict {
        let mut strokes = Vec::new();
        let mut text = String::new();
        let mut keys = Vec::new();
        let mut values = Vec::new();

        for (k, v) in self.map {
            let a = strokes.len();
            strokes.extend_from_slice(&k);
            let b = strokes.len();
            keys.push((a as u32, b as u32));

            let a = text.len();
            text.push_str(&v);
            let b = text.len();
            values.push((a as u32, b as u32));
        }

        RamDict { strokes, keys, text, values }
    }
}
