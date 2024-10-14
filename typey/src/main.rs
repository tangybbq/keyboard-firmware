use std::{
    fs::File, io::{stdin, stdout, BufRead, BufReader, Write}, rc::Rc, str::FromStr
};

use anyhow::{anyhow, Result};
use bbq_steno::{
    dict::{Dict, Joiner, Lookup, MapDictBuilder}, memdict::MemDict, Stroke
};
use clap::{Parser, Subcommand, ValueEnum};
use regex::Regex;
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

/// The main commands available.
#[derive(Debug, Subcommand)]
enum Command {
    #[clap(name = "write")]
    /// Write, using the dictionary lookup.
    Write(WriteCommand),
}

#[derive(Debug, Parser)]
struct WriteCommand {
    #[arg(long = "dict")]
    /// The path to the dictionary to use.
    file: Option<String>,

    #[arg(long)]
    /// Style of show to use.
    show: Option<ShowStyle>,

    #[arg(long, default_value = "joiner")]
    /// Where to stop printing.
    stop: StopPoint,
}

#[derive(Debug, Parser)]
#[command(name = "typey")]
#[command(about = "Typing testing utilities")]
struct Opt {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, ValueEnum, Clone)]
enum ShowStyle {
    /// Short debugging.
    Short,
    /// Longer debugging.
    Long,
}

#[derive(Debug, Eq, PartialEq, Clone, ValueEnum)]
enum StopPoint {
    /// Stop after receiving just the raw steno.
    Steno,
    /// Stop after Lookup.
    Lookup,
    /// Stop after Joiner.
    Joiner,
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
    let opt = Opt::parse();
    println!("command: {:?}", opt);

    match opt.command {
        Command::Write(cmd) => {
            writer(&cmd)?;
        }
    }

    Ok(())
}

fn writer(cmd: &WriteCommand) -> Result<()> {
    let file = cmd.file.clone().unwrap_or_else(|| "../phoenix/phoenix.bin".to_string());
    let dict = load_dict(&file)?;
    let mut xlat = Lookup::new(dict);
    let stdin = stdin();
    let mut stdout = stdout().into_raw_mode()?;

    let mut joiner = Joiner::new();

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
                word.clear();
                writeln!(stdout, "Write: {}\r", stroke)?;
                if cmd.stop == StopPoint::Steno {
                    continue;
                }
                stdout.suspend_raw_mode()?;
                let action = xlat.add(stroke);
                match cmd.show {
                    Some(ShowStyle::Short) => xlat.show(),
                    Some(ShowStyle::Long) => xlat.show_verbose(),
                    None => (),
                }
                writeln!(stdout, "Action: {:?}", action)?;

                if cmd.stop == StopPoint::Lookup {
                    stdout.activate_raw_mode()?;
                    continue;
                }
                joiner.add(action);
                if let Some(ShowStyle::Short) = cmd.show {
                    joiner.show();
                }
                while let Some(act) = joiner.pop(0) {
                    writeln!(stdout, "Act: {:?}", act)?;
                }
                stdout.activate_raw_mode()?;
                continue;
            } else {
                writeln!(stdout, "Invalid: {:?}\r", word)?;
                word.clear();
                continue;
            }
        }
        if let Key::Char(ch) = key {
            word.push(ch);
            continue;
        }
        writeln!(stdout, "Key: {:?}\r", key)?;
    }
    Ok(())
}

/// Load the given dictionary, using the extension to determine what type it is.
/// Note that memory dictionaries are leaked, so that they are static.  This is a consequence of the
/// API of the embedded dictionary which is intended to operate on memory mapped data.
fn load_dict(name: &str) -> Result<Vec<Dict>> {
    if name.ends_with(".bin") {
        // .bin files are the internal memory mapped format used in the
        // keyboards.
        let bindict = std::fs::read(name)?;
        let bindict = bindict.leak();
        let mdict = unsafe { MemDict::from_raw_ptr(bindict.as_ptr()) };
        return Ok(mdict.into_iter().map(|d| Rc::new(d) as Dict).collect())
    }
    if name.ends_with(".txt") {
        return load_txt(name)
    }
    Err(anyhow!("Unknown dictionary extension"))
}

/// Load a .txt dictionary.  This is a simple format, which consists of entries
/// similar to those found in the typey drills.
fn load_txt(name: &str) -> Result<Vec<Dict>> {
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
    Ok(vec![Rc::new(dict.into_ram_dict()) as Dict])
}
