// Test dictionaries.

use std::{collections::BTreeMap, fs::File};

use anyhow::Result;
use bbq_steno::{
    dict::{RamDict, MapDictBuilder, Selector},
    stroke::StenoWord,
};
use bbq_steno_macros::stroke;

#[test]
fn ramdict() {
    let mut b = MapDictBuilder::new();
    b.insert(vec![stroke!("S")], "S".to_string());
    b.insert(vec![stroke!("ST")], "ST".to_string());
    b.insert(vec![stroke!("ST"), stroke!("OP")], "ST/OP".to_string());
    b.insert(
        vec![stroke!("ST"), stroke!("OP"), stroke!("-G")],
        "ST/OP/-G".to_string(),
    );
    // b.insert(vec![stroke!("ST-Z")], "S".to_string());
    let dict = b.into_ram_dict();

    let pos = Selector::new(&dict);
    // println!("full: {:?}", pos);
    let (posb, text) = pos.lookup_step(&dict, stroke!("ST")).unwrap();
    // println!("ST: {:?}", posb);
    assert_eq!(text, Some("ST"));
    let (_posc, text) = posb.lookup_step(&dict, stroke!("OP")).unwrap();
    assert_eq!(text, Some("ST/OP"));
    // println!("ST/OP: {:?}", posc);
}

/*
#[test]
fn simple_dict() {
let mut b = MapDictBuilder::new();
b.insert(vec![stroke!("ST")], "ST".to_string());
b.insert(vec![stroke!("ST"), stroke!("OP")], "ST/OP".to_string());
b.insert(
vec![stroke!("ST"), stroke!("OP"), stroke!("-G")],
"ST/OP/-G".to_string(),
    );
    let dict = b.into_map_dict();

    assert_eq!(dict.prefix_lookup(&[]), None);
    assert_eq!(dict.prefix_lookup(&[stroke!("STO")]), None);
    assert_eq!(dict.prefix_lookup(&[stroke!("ST")]), Some((1, "ST")));
    assert_eq!(
        dict.prefix_lookup(&[stroke!("ST"), stroke!("AUP")]),
        Some((1, "ST"))
    );
    assert_eq!(
        dict.prefix_lookup(&[stroke!("ST"), stroke!("OP")]),
        Some((2, "ST/OP"))
    );
    assert_eq!(
        dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-R")]),
        Some((2, "ST/OP"))
    );
    assert_eq!(
        dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-G")]),
        Some((3, "ST/OP/-G"))
    );
    assert_eq!(
        dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-G"), stroke!("ST")]),
        Some((2, "ST/OP/-G"))
    );
}
*/

#[test]
fn main_dict() {
    let dict = load_dict().expect("Unable to load main dict");
    let pos = Selector::new(&dict);
    let (pos, text) = pos.lookup_step(&dict, stroke!("1257B")).unwrap();
    assert!(text.is_none());
    let (pos, text) = pos.lookup_step(&dict, stroke!("HREU")).unwrap();
    assert_eq!(text, Some("Stanley"));
    assert!(!pos.unique());
}

/*
#[test]
fn test_translator() {
    let dict = load_dict().expect("Load main dict");
    let mut xlat = Translator::new(&dict);

    for st in [
        stroke!("A"),
        stroke!("ABT"),
        stroke!("-G"),
        stroke!("A"),
        stroke!("ABT"),
        stroke!("AG"),
        // Asia, first stroke doesn't translate.
        stroke!("AEURB"),
        stroke!("SHA"),
    ] {
        println!("Add: {}", st);
        xlat.add(st);
        xlat.show();
    }

    todo!();
}
*/

/// Load the main dictionary.
fn load_dict() -> Result<RamDict> {
    let data: BTreeMap<String, String> =
        serde_json::from_reader(File::open("../dict-convert/lapwing-base.json")?)?;
    let mut builder = MapDictBuilder::new();
    for (k, v) in data {
        let k = StenoWord::parse(&k)?;
        builder.insert(k.0, v);
    }
    Ok(builder.into_ram_dict())
}
