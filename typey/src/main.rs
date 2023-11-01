use std::{
    collections::BTreeMap,
    fs::File,
    io::{stdin, stdout, Write},
};

use anyhow::Result;
use bbq_steno::{
    dict::{MapDict, MapDictBuilder, Translator},
    stroke::StenoWord,
    Stroke,
};
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

fn main() -> Result<()> {
    let dict = load_dict().expect("Load main dict");
    let mut xlat = Translator::new(dict);
    let stdin = stdin();
    let mut stdout = stdout().into_raw_mode()?;

    let mut word = String::new();
    for key in stdin.keys() {
        let key = key?;
        if key == Key::Esc {
            writeln!(stdout, "Done\r")?;
            break;
        }
        if key == Key::Char(' ') {
            if let Ok(stroke) = Stroke::from_text(&word) {
                writeln!(stdout, "Write: {}\r", stroke)?;
                stdout.suspend_raw_mode()?;
                xlat.add(stroke);
                xlat.show();
                while let Some(action) = xlat.next_action() {
                    writeln!(stdout, ">>> Delete {} type: {:?}", action.remove, action.text)?;
                }
                stdout.activate_raw_mode()?;
            } else {
                writeln!(stdout, "Invalid: {:?}\r", word)?;
            }
            word.clear();
            continue;
        }
        if let Key::Char(ch) = key {
            word.push(ch);
            continue;
        }
        writeln!(stdout, "Key: {:?}\r", key)?;
    }
    Ok(())
}

fn load_dict() -> Result<MapDict> {
    let data: BTreeMap<String, String> =
        serde_json::from_reader(File::open("../dict-convert/main.json")?)?;
    let mut builder = MapDictBuilder::new();
    for (k, v) in data {
        let k = StenoWord::parse(&k)?;
        builder.insert(k.0, v);
    }
    Ok(builder.into_map_dict())
}