use std::{
    io::{stdin, stdout, Write}, path::Path, rc::Rc
};

use anyhow::Result;
use bbq_steno::{
    dict::{Translator, Dict},
    Stroke, memdict::MemDict,
};
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
}

#[derive(Debug, StructOpt)]
#[structopt(name = "typey", about = "Typing testing utilities")]
struct Opt {
    #[structopt(subcommand)]
    command: Command
}

// mod rtfcre;

fn main() -> Result<()> {
    let opt = Opt::from_args();
    println!("command: {:?}", opt);

    match opt.command {
        Command::Write(cmd) => {
            let file = cmd.file.unwrap_or_else(|| "../dict-convert/phoenix.bin".to_string());
            writer(&file)?;
        }
    }

    Ok(())
}

fn writer<P: AsRef<Path>>(dict: P) -> Result<()> {
    let bindict = std::fs::read(dict)?;
    let mdict = unsafe { MemDict::from_raw_ptr(bindict.as_ptr()) }.unwrap();
    let dict: Dict = Rc::new(mdict);
    // let dict = load_dict().expect("Load main dict");
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

/*
fn load_dict() -> Result<Rc<RamDict>> {
    let phoenix = rtfcre::import("../phoenix/phoenix.rtf")?;
    let mut builder = MapDictBuilder::new();
    for (k, v) in phoenix {
        // let k = StenoWord::parse(&k)?;
        builder.insert(k.0, v);
    }
    Ok(Rc::new(builder.into_ram_dict()))
}
*/
