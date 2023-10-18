#![allow(dead_code)]

use std::{fs::File, collections::BTreeMap, io::Write};

use anyhow::Result;

use bbq_steno::Stroke;
use bbq_steno::stroke::StenoWord;
use bbq_steno::memdict::{MAGIC1, MemDict};
use bbq_steno_macros::stroke;
use byteorder::{LittleEndian, WriteBytesExt};
// use rand::RngCore;

// Must match the endianness of our target.
type Target = LittleEndian;

fn main() -> Result<()> {
    let data: BTreeMap<String, String>  = serde_json::from_reader(
        File::open("main.json")?
    )?;

    let mut dict: BTreeMap<StenoWord, String> = BTreeMap::new();

    for (k, v) in data.iter() {
        let k = StenoWord::parse(k)?;
        dict.insert(k, v.clone());
    }

    // Print out the longest entry.
    let longest = dict.keys().map(|k| k.0.len()).max();
    println!("Longest key: {:?}", longest);

    let memory = encode_dict(&dict)?;

    File::create("main.bindict")?.write_all(&memory)?;

    // Let's map this (somewhat unsafely) and see what we get out of it.
    let mdict = unsafe { MemDict::from_raw_ptr(memory.as_ptr()).unwrap() };
    println!("Header:\n{:#?}", mdict.raw);
    println!("Keys: {}", mdict.keys.len());

    // Print out the first some number of keys.
    for k in 0 .. 12 {
        let key = mdict.get_key(k);
        let key = StenoWord(key.to_vec());
        let text = mdict.get_text(k);
        println!("   {} -> {:?}", key, text);
    }

    // Try some lookups.
    println!("lookup test");
    for stroke in TEST_STROKES {
        let text = mdict.lookup(stroke);
        println!("  {} -> {:?}", StenoWord(stroke.to_vec()), text);
        let text = mdict.prefix_lookup(stroke);
        println!("  {} -> {:?}", StenoWord(stroke.to_vec()), text);
    }

    println!("prefix lookup test");
    for stroke in PREFIX_STROKES {
        let text = mdict.prefix_lookup(stroke);
        println!("  {} -> {:?}", StenoWord(stroke.to_vec()), text);
    }
    Ok(())
}

static TEST_STROKES: &[&[Stroke]] = &[
    &[stroke!("-T")],
    &[stroke!("THE")],
    &[
        stroke!("AOE"),
        stroke!("PHRAOUR"),
        stroke!("PWUS"),
        stroke!("KWRAOU"),
        stroke!("TPHUPL"),
    ],
    &[
        stroke!("AOE"),
        stroke!("PHRAOUR"),
        stroke!("PWUS"),
        stroke!("KWRAOU"),
        stroke!("TPHUPLZ"),
    ]];

static PREFIX_STROKES: &[&[Stroke]] = &[
    &[stroke!("-T"), stroke!("S")],
    &[stroke!("-T"), stroke!("-Z")],
    &[stroke!("THE"), stroke!("S")],
    &[stroke!("THE"), stroke!("-Z")],
    &[
        stroke!("AOE"),
        stroke!("PHRAOUR"),
        stroke!("PWUS"),
        stroke!("KWRAOU"),
        stroke!("TPHUPL"),
        stroke!("S"),
    ],
    &[
        stroke!("AOE"),
        stroke!("PHRAOUR"),
        stroke!("PWUS"),
        stroke!("KWRAOU"),
        stroke!("TPHUPL"),
        stroke!("-Z"),
    ],
];

fn encode_dict(dict: &BTreeMap<StenoWord, String>) -> Result<Vec<u8>> {
    let mut result = Vec::new();

    // The header gets a placeholder for now.
    for _ in 0..48 {
        result.push(0);
    }
    let mut header = Vec::new();
    header.extend(MAGIC1);
    header.write_u32::<Target>(dict.len() as u32)?;

    // Write out the key table.
    let mut keys = Vec::new();
    let key_table = result.len();
    let mut offset = 0;
    for k in dict.keys() {
        // Record the key offset table.
        keys.push(TablePos { offset, length: k.0.len() });
        offset += k.0.len();

        // Push out the strokes to the file.
        for st in &k.0 {
            result.write_u32::<Target>(st.into_raw())?;
        }
    }
    let new_pos = result.len();
    header.write_u32::<Target>(key_table as u32)?;
    header.write_u32::<Target>((new_pos - key_table) as u32)?;

    assert_eq!(dict.len(), keys.len());

    pad(&mut result, 8);
    let keyposes = result.len();
    header.write_u32::<Target>(keyposes as u32)?;

    for pos in &keys {
        result.write_u32::<Target>(pos.encoded())?;
    }

    pad(&mut result, 8);
    // Encode all of the text strings.
    let mut texts = Vec::new();
    let text_table = result.len();
    let mut offset = 0;
    for v in dict.values() {
        texts.push(TablePos { offset, length: v.len() });
        offset += v.len();

        // Append the raw text.
        result.extend_from_slice(v.as_bytes());
    }

    pad(&mut result, 8);
    header.write_u32::<Target>(text_table as u32)?;
    header.write_u32::<Target>(offset as u32)?;

    let textposes = result.len();
    header.write_u32::<Target>(textposes as u32)?;
    for pos in &texts {
        result.write_u32::<Target>(pos.encoded())?;
    }

    pad(&mut result, 8);

    // Stamp the header in place.
    result[0..header.len()].copy_from_slice(&header);

    Ok(result)
}

fn pad(buf: &mut Vec<u8>, count: usize) {
    while (buf.len() % count) > 0 {
        buf.push(0);
    }
}

struct TablePos {
    offset: usize,
    length: usize,
}

impl TablePos {
    fn encoded(&self) -> u32 {
        // Encode by putting the length as the upper 8 bits, and the offset in the lower.
        assert!(self.length < (1 << 8));
        assert!(self.offset < (1 << 24));
        ((self.length << 24) as u32) | (self.offset as u32)
    }
}
