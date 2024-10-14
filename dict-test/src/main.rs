//! Steno dictionary lookup testing.

use std::{path::Path, io::BufRead, io::BufReader, fs::File, rc::Rc};

use anyhow::{Result, anyhow};
use bbq_steno::{dict::{Dict, Joined, Joiner, Lookup}, memdict::MemDict, Stroke};
use regex::Regex;

fn main() -> Result<()> {
    // Pull in the user dictionary.
    let bindict = std::fs::read("../bbq-tool/dicts.bin")?;
    let mdict = unsafe { MemDict::from_raw_ptr(bindict.as_ptr()) };
    let dicts: Vec<_> = mdict.into_iter().map(|d| Rc::new(d) as Dict).collect();

    let base = dirs::home_dir().unwrap().join("steno").join("steno-drill").join("phoenix");
    let mut names = Vec::new();
    for entry in base.read_dir()? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        names.push(entry.file_name());
    }
    names.sort();
    for name in &names {
        let path = base.join(name);
        let drill = Exercise::load(&path)?;
        // println!("Checking: {}", drill.name);
        // println!("{:#?}", drill);
        // drill.entries[0].check(dict.clone());

        drill.check(dicts.clone());
    }

    Ok(())
}

#[derive(Debug)]
pub struct Exercise {
    name: String,
    entries: Vec<Entry>,
}

#[derive(Debug)]
pub struct Entry {
    text: String,
    steno: Vec<Stroke>,
}

impl Exercise {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Exercise> {
        let re = Regex::new(r"^'(.*)': ([A-Z0-9/^+*-]+)$")?;
        let mut lines = BufReader::new(File::open(path)?).lines();
        let name = lines.next().unwrap()?;
        let blank = lines.next().unwrap()?;
        if !blank.is_empty() {
            return Err(anyhow!("Second line must be blank"));
        }
        let mut entries = Vec::new();

        for line in lines {
            let line = line?;
            let caps = match re.captures(&line) {
                Some(caps) => caps,
                None => {
                    println!("Unparsed line: {:?}", line);
                    continue;
                }
            };
            let text = caps[1].to_string();
            let mut steno = Vec::new();
            for stroke in caps[2].split("/") {
                steno.push(Stroke::from_text(stroke)?);
            }
            // The engine always capitalizes at the start, but we have numerous
            // exercises that aren't capitalized. Work around this by just
            // capitalizing the beginning.
            let text = Self::capitalize(&text);
            entries.push(Entry { text, steno });
        }

        Ok(Exercise { name, entries })
    }

    pub fn check(&self, dicts: Vec<Dict>) {
        let mut wrongs = Vec::new();
        for entry in &self.entries {
            let mut lwrongs = entry.check(dicts.clone());
            wrongs.append(&mut lwrongs);
        }
        if !wrongs.is_empty() {
            println!("In: {}", self.name);
            for (good, bad) in wrongs {
                println!("    expect: {:?}", good);
                println!("       got: {:?}", bad);
            }
        }
    }

    // Capitalize utility.
    fn capitalize(text: &str) -> String {
        let mut c = text.chars();
        match c.next() {
            None => String::new(),
            Some(ch) => {
                let mut result: String = ch.to_uppercase().collect();
                result.extend(c);
                result
            }
        }
    }
}

impl Entry {
    /// Determine if the given entry translates the same using the dictionary as the given text.
    /// Returns the incorrect entries, with the expected and actual values in a tuple.
    pub fn check(&self, dicts: Vec<Dict>) -> Vec<(String, String)> {
        let mut wrongs = Vec::new();

        // The exercises insert a visible space to make it possible to tell if
        // this should be a phrase. Remove that here, as we want to compare with
        // real spaces.
        let src = self.text.replace("␣", " ");
        let mut lookup = Lookup::new(dicts);
        let mut joiner = Joiner::new();

        let mut text = String::new();

        for stroke in &self.steno {
            // println!("stroke: {}", stroke);
            let action = lookup.add(*stroke);
            joiner.add(action);
            while let Some(act) = joiner.pop(0) {
                match act {
                    Joined::Type { remove, append } => {
                        for _ in 0..remove {
                            text.pop();
                        }
                        text.push_str(&append);
                    }
                }
            }
        }
        if src != text {
            wrongs.push((src, text));
        }
        wrongs
    }
}
