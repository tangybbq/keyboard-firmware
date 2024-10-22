use std::{
    fs::{File, OpenOptions}, io::{stdin, stdout, BufRead, BufReader, Write}, rc::Rc, str::FromStr
};

use anyhow::{anyhow, Result};
use bbq_steno::{
    dict::{Dict, Joined, Joiner, Lookup, MapDictBuilder}, memdict::MemDict, stroke::StenoWord, Stroke
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
    #[clap(name = "exbuild")]
    /// Build a steno dictionary.
    Exbuild(ExbuildCommand),
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
struct ExbuildCommand {
    #[arg(long = "dict")]
    /// The path to the dictionary to use.
    file: String,

    #[arg(long)]
    /// Where to output the finished exercise file.
    output: String,
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
        Command::Exbuild(cmd) => exbuild(&cmd)?,
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

fn exbuild(cmd: &ExbuildCommand) -> Result<()> {
    let mut out = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&cmd.output)?;

    writeln!(out, "Exercise {}", cmd.output)?;
    writeln!(out, "")?;

    let dict = load_dict(&cmd.file)?;

    let mut xlat = Lookup::new(dict);
    let mut joiner = Joiner::new();

    let mut strokes = Vec::new();
    let mut text = String::new();

    let stdin = stdin();
    let mut stdout = stdout().into_raw_mode()?;

    println!("Begin\r");
    let mut word = String::new();
    for key in stdin.keys() {
        let key = key?;
        if key == Key::Esc {
            break;
        }
        if key == Key::Char(' ') {
            if let Ok(stroke) = Stroke::from_text(&word) {
                // println!("s:{}", stroke);
                // Handle the newline specially.
                if word == "R-RPB" {
                    let stenoword = StenoWord(strokes);
                    // println!("\r\n'{}': {}\r", text, stenoword);
                    println!("\r");
                    writeln!(out, "'{}': {}", text, stenoword)?;
                    strokes = Vec::new();
                    text = String::new();
                    word.clear();

                    // Ideally, the translator could just continue, but we don't yet carry over caps
                    // yet, so just create a new one.  This breaks delete before the newline.
                    joiner = Joiner::new();
                    continue;
                }
                word.clear();
                if stroke.is_star() {
                    strokes.pop();
                } else {
                    strokes.push(stroke);
                }
                let action = xlat.add(stroke);
                joiner.add(action);
                // TODO: Handle raw and other types.
                while let Some(Joined::Type { remove, append }) = joiner.pop(0) {
                    for _ in 0..remove {
                        write!(stdout, "\u{0008} \u{0008}")?;
                        text.pop();
                    }
                    let mut appendit = append.chars();
                    if let Some(ch) = appendit.next() {
                        text.push(ch);
                    }
                    // For exercises, replace explicit spaces with the Unicode visible space.
                    text.push_str(&appendit.as_str().replace(' ', "␣"));
                    write!(stdout, "{}", append)?;
                    stdout.flush()?;
                }
                continue;
            } else {
                // Print invalid.
                println!("#<{}>", word);
                word.clear();
                continue;
            }
        }
        if let Key::Char(ch) = key {
            word.push(ch);
            continue;
        }
    }

    Ok(())
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
