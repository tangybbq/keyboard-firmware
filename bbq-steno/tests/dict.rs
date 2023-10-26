// Test dictionaries.

use std::{collections::BTreeMap, fs::File};

use anyhow::Result;
use bbq_steno::{dict::{MapDictBuilder, Dict, MapDict}, stroke::StenoWord};
use bbq_steno_macros::stroke;

#[test]
fn simple_dict() {
    let mut b = MapDictBuilder::new();
    b.insert(vec![stroke!("ST")], "ST".to_string());
    b.insert(vec![stroke!("ST"), stroke!("OP")], "ST/OP".to_string());
    b.insert(vec![stroke!("ST"), stroke!("OP"), stroke!("-G")], "ST/OP/-G".to_string());
    let dict = b.into_map_dict();

    assert_eq!(dict.prefix_lookup(&[]), None);
    assert_eq!(dict.prefix_lookup(&[stroke!("STO")]), None);
    assert_eq!(dict.prefix_lookup(&[stroke!("ST")]), Some((1, "ST")));
    assert_eq!(dict.prefix_lookup(&[stroke!("ST"), stroke!("AUP")]), Some((1, "ST")));
    assert_eq!(dict.prefix_lookup(&[stroke!("ST"), stroke!("OP")]), Some((2, "ST/OP")));
    assert_eq!(dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-R")]),
               Some((2, "ST/OP")));
    assert_eq!(dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-G")]),
               Some((3, "ST/OP/-G")));
    assert_eq!(dict.prefix_lookup(&[stroke!("ST"), stroke!("OP"), stroke!("-G"),
                                    stroke!("ST")]),
               Some((2, "ST/OP/-G")));
}

#[test]
fn main_dict() {
    let dict = load_dict().expect("Unable to load main dict");
    assert_eq!(dict.prefix_lookup(&[stroke!("STAPB"), stroke!("HREU"), stroke!("PHAPB")]),
               Some((2, "Stanley")));
}

/// Load the main dictionary.
fn load_dict() -> Result<MapDict> {
    let data: BTreeMap<String, String> = serde_json::from_reader(
        File::open("../dict-convert/main.json")?
    )?;
    let mut builder = MapDictBuilder::new();
    for (k, v) in data {
        let k = StenoWord::parse(&k)?;
        builder.insert(k.0, v);
    }
    Ok(builder.into_map_dict())
}
