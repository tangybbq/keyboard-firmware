use std::{
    fs::File, io::{stdin, stdout, BufRead, BufReader, Write}, rc::Rc, str::FromStr
};

use anyhow::{anyhow, Result};
use bbq_steno::{
    dict::{Dict, DictImpl, MapDictBuilder, Selector, Translator}, memdict::MemDict, Stroke
};
use regex::Regex;
use structopt::StructOpt;
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

/// The main commands available.
#[derive(Debug, StructOpt)]
enum Command {
    #[structopt(name = "write")]
    /// Write, using the dictionary lookup.
    Write(WriteCommand),
}

#[derive(Debug, StructOpt)]
struct WriteCommand {
    #[structopt(long = "dict")]
    /// The path to the dictionary to use.
    file: Option<String>,

    #[structopt(long = "show")]
    /// Style of show to use.
    show: Option<ShowStyle>,
}

#[derive(Debug, StructOpt)]
#[structopt(name = "typey", about = "Typing testing utilities")]
struct Opt {
    #[structopt(subcommand)]
    command: Command
}

#[derive(Debug)]
enum ShowStyle {
    Short,
    Long,
}

/// Allow ShowStyle as argument
impl FromStr for ShowStyle {
    type Err = anyhow::Error;
    fn from_str(text: &str) -> Result<Self> {
        match text {
            "short" => Ok(ShowStyle::Short),
            "long" => Ok(ShowStyle::Long),
            _ => Err(anyhow!("Unknown show style")),
        }
    }
}

// mod rtfcre;

fn main() -> Result<()> {
    let opt = Opt::from_args();
    println!("command: {:?}", opt);

    match opt.command {
        Command::Write(cmd) => {
            let file = cmd.file.unwrap_or_else(|| "../dict-convert/phoenix.bin".to_string());
            writer(&file, cmd.show)?;
        }
    }

    Ok(())
}

fn writer(dict: &str, show: Option<ShowStyle>) -> Result<()> {
    let dict = load_dict(dict)?;
    let mut xlat = Translator::new(dict);
    let stdin = stdin();
    let mut stdout = stdout().into_raw_mode()?;

    let mut word = String::new();
    writeln!(stdout, "Begin.\r")?;
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
                match show {
                    Some(ShowStyle::Short) => xlat.show(),
                    Some(ShowStyle::Long) => xlat.show_verbose(),
                    None => (),
                }
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

/// The normal MemDict is an unsafe pointer to something.  To at least give this
/// a resemblance of something usable dynamically, this struct implements dict,
/// keeping the data around.
struct KeptDict {
    _kept: Vec<u8>,
    dict: Rc<MemDict>,
}

impl KeptDict {
    /// Take ownership of a block of data and build a memdict out of it.
    pub fn from_data(data: Vec<u8>) -> KeptDict {
        // `Vec` does not move the data as long as there a no allocations.
        let mdict = unsafe { MemDict::from_raw_ptr(data.as_ptr()) }.unwrap();
        KeptDict {
            _kept: data,
            dict: Rc::new(mdict),
        }
    }
}

/// Passthrough implementation.
impl DictImpl for KeptDict {
    fn len(&self) -> usize {
        self.dict.len()
    }

    fn key(&self, index: usize) -> &[Stroke] {
        self.dict.key(index)
    }

    fn value(&self, index: usize) -> &str {
        self.dict.value(index)
    }

    fn selector(self: Rc<Self>) -> Box<dyn Selector> {
        self.dict.clone().selector()
    }

    fn scan(&self, a: usize, b: usize, pos: usize, needle: Stroke) -> usize {
        self.dict.scan(a, b, pos, needle)
    }
}

/// Load the given dictionary, using the extension to determine what type it is.
fn load_dict(name: &str) -> Result<Dict> {
    if name.ends_with(".bin") {
        // .bin files are the internal memory mapped format used in the
        // keyboards.
        let bindict = std::fs::read(name)?;
        return Ok(Rc::new(KeptDict::from_data(bindict)));
    }
    if name.ends_with(".txt") {
        return load_txt(name)
    }
    Err(anyhow!("Unknown dictionary extension"))
}

/// Load a .txt dictionary.  This is a simple format, which consists of entries
/// similar to those found in the typey drills.
fn load_txt(name: &str) -> Result<Dict> {
    let re = Regex::new(r"^'(.*)': ([A-Z0-9/^+*-]+)$")?;
    let lines = BufReader::new(File::open(name)?).lines();
    let mut dict = MapDictBuilder::new();
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
        dict.insert(steno, text);
    }
    Ok(Rc::new(dict.into_ram_dict()))
}
