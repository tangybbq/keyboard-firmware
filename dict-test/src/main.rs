//! Steno dictionary lookup testing.

use std::{path::Path, io::BufRead, io::BufReader, fs::File, rc::Rc};

use anyhow::{Result, anyhow};
use bbq_steno::{memdict::MemDict, Stroke, dict::{Dict, Translator}};
use regex::Regex;

fn main() -> Result<()> {
    // Pull the entire dictionary in.
    let bindict = std::fs::read("../dict-convert/phoenix.bin")?;
    let mdict = unsafe { MemDict::from_raw_ptr(bindict.as_ptr()) }.unwrap();
    let dict: Dict = Rc::new(mdict);

    let base = Path::new("/home/davidb/steno/steno-drill/phoenix");
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

        drill.check(dict.clone());
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

        Ok(Exercise { name: name, entries: entries })
    }

    pub fn check(&self, dict: Dict) {
        println!("Check: {}", self.name);
        for entry in &self.entries {
            entry.check(dict.clone());
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
    pub fn check(&self, dict: Dict) {
        // The exercises insert a visible space to make it possible to tell if
        // this should be a phrase. Remove that here, as we want to compare with
        // real spaces.
        let src = self.text.replace("␣", " ");
        let mut xlat = Translator::new(dict);

        let mut text = String::new();

        for stroke in &self.steno {
            // println!("stroke: {}", stroke);
            xlat.add(*stroke);
            while let Some(act) = xlat.next_action() {
                // println!("Act: {:?}", act);
                for _ in 0..act.remove {
                    text.pop();
                }
                text.push_str(&act.text);
            }
        }
        if src != text {
            println!("good: {:?}", src);
            println!("      {:?}", text);
        }
    }
}
