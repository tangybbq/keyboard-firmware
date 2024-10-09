//! JSON dictionary loading.

use std::{collections::BTreeMap, fs::File, path::Path};

use bbq_steno::stroke::StenoWord;
use regex::Regex;

use crate::Result;

pub fn import<P: AsRef<Path>>(name: P) -> Result<BTreeMap<StenoWord, String>> {
    let new: BTreeMap<String, String> = serde_json::from_reader(
        File::open(name)?
    )?;
    let fixer = JsonFixer::new();

    let mut dict = BTreeMap::new();

    for (k, v) in new.iter() {
        let k = StenoWord::parse(k)?;
        dict.insert(k, fixer.fix(v));
    }

    Ok(dict)
}

struct JsonFixer {
    stitch: Regex,
}

impl JsonFixer {
    fn new() -> JsonFixer {
        JsonFixer {
            stitch: Regex::new(r"^\{\&(.*)\}$").unwrap(),
        }
    }

    fn fix(&self, text: &str) -> String {
        if text == "{?}" {
            return "\x01?\x02".to_string();
        }
        if let Some(caps) = self.stitch.captures(text) {
            return format!("\x03{}", &caps[1]);
        }
        text.to_string()
    }
}

