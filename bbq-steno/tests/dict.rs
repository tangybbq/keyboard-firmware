// Test dictionaries.

use std::rc::Rc;

use bbq_steno::{
    dict::{MapDictBuilder, DictImpl},
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
    let (_, text) = posb.lookup_step(stroke!("OP")).unwrap();
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
