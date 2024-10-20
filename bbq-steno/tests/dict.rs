// Test dictionaries.

use std::{collections::BTreeMap, fs::File, rc::Rc};

use anyhow::Result;
use bbq_steno::{
    dict::{RamDict, MapDictBuilder, DictImpl},
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
    let dict = Rc::new(b.into_ram_dict());
    let pos = dict.clone().selector();
    // println!("full: {:?}", pos);
    let (posb, text) = pos.lookup_step(stroke!("ST")).unwrap();
    // println!("ST: {:?}", posb);
    assert_eq!(text, Some("ST".to_string()));
    let (_posc, text) = posb.lookup_step(stroke!("OP")).unwrap();
    assert_eq!(text, Some("ST/OP".to_string()));
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

// #[test]
fn main_dict() {
    let dict = load_dict().expect("Unable to load main dict");
    let pos = dict.clone().selector();
    let (pos, text) = pos.lookup_step(stroke!("1257B")).unwrap();
    assert_eq!(text, Some("Stan".to_string()));
    let (pos, text) = pos.lookup_step(stroke!("HREU")).unwrap();
    assert_eq!(text, Some("Stanley".to_string()));
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
fn load_dict() -> Result<Rc<RamDict>> {
    let data: BTreeMap<String, String> =
        serde_json::from_reader(File::open("../phoenix/phoenix_fix.json")?)?;
    let mut builder = MapDictBuilder::new();
    for (k, v) in data {
        let k = StenoWord::parse(&k)?;
        builder.insert(k.0, v);
    }
    Ok(Rc::new(builder.into_ram_dict()))
}
