#![allow(dead_code)]

use std::{fs::File, collections::BTreeMap};

use anyhow::Result;

use bbq_steno::stroke::{StenoWord, Stroke};
use rand::RngCore;

fn main() -> Result<()> {
    let data: BTreeMap<String, String>  = serde_json::from_reader(
        File::open("main.json")?
    )?;

    let mut dict: BTreeMap<StenoWord, String> = BTreeMap::new();

    for (k, v) in data.iter() {
        let k = StenoWord::parse(k)?;
        dict.insert(k, v.clone());
    }

    let mut entry = Entries::default();
    let mut spos = 0;
    let mut tpos = 0;
    let mut smax = 0;
    let mut tmax = 0;
    for (k, v) in dict.iter() {
        let slen = k.0.len();
        let tlen = v.len();

        entry.stenos.push(spos as u32);
        entry.texts.push(tpos as u32);

        entry.steno.extend(k.0.iter().cloned());
        entry.text.push_str(v);

        spos += slen;
        tpos += tlen;

        smax = smax.max(slen);
        tmax = tmax.max(tlen);
    }
    entry.stenos.push(spos as u32);
    entry.texts.push(tpos as u32);

    println!("// {} steno offsets", 4 * entry.stenos.len());
    println!("// {} text offsets", 4 * entry.texts.len());
    println!("// {} steno", 4 * entry.steno.len());
    println!("// {} text", entry.text.len());
    println!("// {} total", 4 * (entry.stenos.len() + entry.texts.len() + entry.steno.len()) + entry.text.len());
    println!("//");
    println!("// Longest steno: {}", smax);
    println!("// Longest text: {}", tmax);
    // println!("{:#?}", entry);

    // Build up the trie.
    let mut trie = TrieBuilder::new();
    for (k, v) in dict.iter() {
        trie.add(&k.0, v);
    }
    println!("");
    println!("Trie 0: {}", trie.root.children.len());
    // build_hash(&trie.root);

    // Print out the entire trie.
    if false {
        for entry in trie.nodes() {
            let strokes: Vec<_> = entry.strokes.iter().map(|s| s.to_string()).collect();
            println!("{:>5} {:>3} {:?} {:?}",
                     entry.level,
                     entry.node.children.len(),
                     strokes,
                     entry.node.text);
        }
    }
    // Some useful trie statistics.
    let defn_only = trie
        .nodes()
        .filter(|n| n.node.children.is_empty() && n.node.text.is_some())
        .count();
    println!("{:>6} defn only", defn_only);
    let child_only = trie
        .nodes()
        .filter(|n| !n.node.children.is_empty() && n.node.text.is_none())
        .count();
    println!("{:>6} child only", child_only);
    let both = trie
        .nodes()
        .filter(|n| !n.node.children.is_empty() && n.node.text.is_some())
        .count();
    println!("{:>6} both", both);

    Ok(())
}

/// Build up a hash table based on the root of the trie.
fn build_hash(node: &TrieNode) {
    let size = next_prime_number(node.children.len() * 2);
    println!("Size: {}", size);
    let mut table = Vec::with_capacity(size);
    for _ in 0..size {
        table.push(None);
    }

    let mut collisions = 0;
    for (steno, _) in &node.children {
        let h1 = steno.into_raw() as usize % size;
        let h2 = ((steno.into_raw() as u32).reverse_bits() as usize) % size;
        let mut pos = h1;
        while table[pos].is_some() {
            pos = (pos + h2) % size;
            collisions += 1;
        }
        table[pos] = Some(*steno);
    }
    println!("{} collisions", collisions);

    // Do the lookups and figure out collisions.
    let mut collisions = 0;
    let mut longest = 0;
    for (steno, _) in &node.children {
        let h1 = steno.into_raw() as usize % size;
        let h2 = ((steno.into_raw() as u32).reverse_bits() as usize) % size;
        let mut pos = h1;
        let mut this_count = 1;
        while let Some(elt) = table[pos] {
            if elt == *steno {
                break;
            }
            pos = (pos + h2) % size;
            collisions += 1;
            this_count += 1;
        }
        longest = longest.max(this_count);
    }
    println!("Lookups: {} collisions", collisions);
    println!("Max {} collisions", longest);
    println!("Ave {:.1} looks", 1.0 + (collisions as f32) / (node.children.len() as f32));

    // Do a bunch of random lookups, of entries that aren't present, to get an
    // idea of the time needed to look them up.
    let mut rng = rand::thread_rng();
    collisions = 0;
    longest = 0;
    let total = 10000;
    for _ in 0..total {
        let mut steno;
        loop {
            steno = Stroke::from_raw(rng.next_u32() % (1 << 24));
            if !node.children.contains_key(&steno) {
                break;
            }
        }

        let h1 = steno.into_raw() as usize % size;
        let h2 = ((steno.into_raw() as u32).reverse_bits() as usize) % size;
        let mut pos = h1;
        let mut this_count = 1;
        while let Some(elt) = table[pos] {
            if elt == steno {
                unreachable!();
            }
            pos = (pos + h2) % size;
            collisions += 1;
            this_count += 1;
        }
        longest = longest.max(this_count);
    }
    println!("Lookups of not-present strokes");
    println!("Lookups: {} collisions", collisions);
    println!("Max {} collisions", longest);
    println!("Ave {:.1} looks", 1.0 + (collisions as f32) / (total as f32));
}

/// A packed trie is a more compactly encoded version of the trie, using a hash
/// table with some simple hash functions. This is the alloc version. There is
/// another comparable version of this structure intended to be encoded into ROM
/// that has the same interface and operations.
struct PackedTrie {
    nodes: Vec<Option<PackedNode>>,
    hasher: Hasher,
}

struct PackedNode {
    /// The stroke this part of the lookup contains.
    stroke: Stroke,
    next: Option<PackedTrie>,
    text: Option<String>,
}

/// Hash operations are based on the number of nodes.
struct Hasher {
    size: usize,
}

impl Hasher {
    fn new(size: usize) -> Hasher {
        Hasher {
            size,
        }
    }

    /// Return the two hash values, the first is the initial position, and the
    /// second is the span to use when there is a hash collision.
    fn hash(&self, stroke: Stroke) -> (usize, usize) {
        (stroke.into_raw() as usize % self.size,
         ((stroke.into_raw() as u32).reverse_bits() as usize) % self.size)
    }
}

fn next_prime_number(num: usize) -> usize {
    for n in num.. {
        if primal::is_prime(n as u64) {
            return n;
        }
    }
    unreachable!()
}

#[derive(Debug, Default)]
struct Entries {
    stenos: Vec<u32>,
    texts: Vec<u32>,

    steno: Vec<Stroke>,
    text: String,
}

struct TrieBuilder {
    root: TrieNode,
}

impl TrieBuilder {
    fn new() -> Self {
        TrieBuilder {
            root: TrieNode::new(),
        }
    }

    // Adding is a little weird. The borrow checker would like to see recursion
    // with a recursive structure, so write it that way.
    fn add(&mut self, ks: &[Stroke], v: &str) {
        self.root.add_walk(ks, v);
    }

    pub fn nodes(&self) -> TrieIterator {
        let level = Level {
            node: &self.root,
            iter: self.root.children.keys().cloned().collect::<Vec<_>>().into_iter(),
        };
        TrieIterator {
            keys: vec![],
            nodes: vec![level],
        }
    }
}

/// A trie node.  There can be the entries so far, as well as a possible definition at this level.
#[derive(Debug)]
struct TrieNode {
    children: BTreeMap<Stroke, TrieNode>,
    text: Option<String>,
}

impl TrieNode {
    fn new() -> TrieNode {
        TrieNode {
            children: BTreeMap::new(),
            text: None,
        }
    }

    // Walk down the nodes, until we are at a leaf, and add the definition.
    fn add_walk(&mut self, ks: &[Stroke], v: &str) {
        if let Some(fst) = ks.first() {
            let node = self.children.entry(fst.clone())
                .or_insert_with(|| TrieNode::new());
            node.add_walk(&ks[1..], v);
        } else {
            if self.text.is_some() {
                println!("Warning: Duplicate node");
            }
            self.text = Some(v.to_string());
        }
    }
}

struct TrieIterator<'a> {
    keys: Vec<Stroke>,
    nodes: Vec<Level<'a>>,
}

impl<'a> Iterator for TrieIterator<'a> {
    type Item = IterEntry<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(last) = self.nodes.last_mut() {
                if let Some(next_key) = last.iter.next() {
                    let next_node = &last.node.children[&next_key];
                    let next_keys = next_node.children.keys().cloned().collect::<Vec<_>>().into_iter();
                    self.nodes.push(Level {
                        node: next_node,
                        iter: next_keys,
                    });
                    self.keys.push(next_key);
                    // return Some((self.nodes.len(), next_node));
                } else {
                    let keys = self.keys.clone();
                    self.keys.pop();
                    if let Some(last) = self.nodes.pop() {
                        return Some(IterEntry {
                            level: self.nodes.len(),
                            strokes: keys,
                            node: last.node,
                        });
                    } else {
                        unreachable!()
                    }
                }
            } else {
                return None;
            }
        }
    }
}

struct IterEntry<'a> {
    level: usize,
    strokes: Vec<Stroke>,
    node: &'a TrieNode,
}

struct Level<'a> {
    node: &'a TrieNode,
    iter: std::vec::IntoIter<Stroke>,
}
