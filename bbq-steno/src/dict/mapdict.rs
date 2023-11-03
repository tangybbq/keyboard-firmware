//! Simple dictionary implemented with maps.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::Dict;
use crate::Stroke;

extern crate alloc;

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
}
