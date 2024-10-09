//! Dictionary assembly tool.
//!
//! This tool support parsing and encoding dictionaries in various formats, and assembling them
//! into a multi-layer combined memory-mapped dictionary.  Supported formats are:
//!
//! - json: The Plover native formatting, decoding Plover formatting instructions.
//! - cre: The RTF/CRE format, at least as used by the Phoenix dictionary.

use bbq_keyboard::Side;
use clap::{Parser, Subcommand};

use anyhow::Result;
use encode::DictBuilder;

use std::{collections::BTreeMap, fs::File};
use bbq_steno::{dict::DictImpl, memdict::MemDict, stroke::StenoWord};
use bbq_keyboard::boardinfo::BoardInfo;

mod rtfcre;
mod jsondict;
mod encode;

#[derive(Parser)]
#[command(name = "MyProgram")]
#[command(about = "A command-line tool with build and show subcommands", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the specified files into the output
    Build {
        /// Output file
        #[arg(short, long, value_name = "FILE")]
        output: String,

        /// Input files to build
        #[arg(required = true)]
        files: Vec<String>,
    },

    /// Show the contents of the specified file
    Show {
        /// The file to show
        filename: String,
    },

    /// Generate a buildinfo record.
    BoardInfo {
        /// Output file
        #[arg(short, long, value_name = "FILE")]
        output: String,

        /// The name of this build.
        #[arg(long)]
        name: String,

        /// The side info.
        #[arg(long)]
        side: Option<Side>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build { output, files } => {
            println!("Building files: {:?}", files);
            let mut build = DictBuilder::new();
            for f in files {
                let dict = load_dict(f)?;
                build.add(&dict);
            }

            println!("Output will be written to: {}", output);

            // For now, just concatenate them, but really need a helper to
            // put the group header.
            let mut fd = File::create(output)?;
            build.write_group(&mut fd)?;
        }
        Commands::Show { filename } => {
            println!("Showing file: {}", filename);
            let data = std::fs::read(filename)?;
            let dicts = unsafe { MemDict::from_raw_ptr(data.as_ptr()) };
            println!("There are {} dicts", dicts.len());
            for dict in &dicts {
                println!("{} entries", dict.len());
                println!("key offsets: {} len", dict.key_offsets.len());
                println!("offset 0: 0x{:x?}", dict.key_offsets[0]);
                println!("key 0 {:?}", dict.key(0));
                println!("text 0 {:?}", dict.value(0));
            }
        }
        Commands::BoardInfo { output, name, side } => {
            let info = BoardInfo {
                name: name.to_string(),
                side: side.clone(),
            };

            let mut fd = File::create(output)?;
            info.encode(&mut fd)?;
        }
    }

    Ok(())
}

/// Attempt to load the given dictionary.
///
/// Loads the dictionary, based on the given type.  It is up to each loader to translate from that
/// dictionary's encoding into the encoded string representing processing codes.
fn load_dict(name: &str) -> Result<BTreeMap<StenoWord, String>> {
    if name.ends_with(".json") {
        load_json(name)
    } else if name.ends_with(".rtf") {
        load_rtf(name)
    } else {
        Err(anyhow::anyhow!("Unknown dictionary file type"))
    }
}

fn load_json(name: &str) -> Result<BTreeMap<StenoWord, String>> {
    jsondict::import(name)
}

fn load_rtf(name: &str) -> Result<BTreeMap<StenoWord, String>> {
    rtfcre::import(name)
}
