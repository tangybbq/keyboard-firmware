//! Dictionary assembly tool.
//!
//! This tool support parsing and encoding dictionaries in various formats, and assembling them
//! into a multi-layer combined memory-mapped dictionary.  Supported formats are:
//!
//! - json: The Plover native formatting, decoding Plover formatting instructions.
//! - cre: The RTF/CRE format, at least as used by the Phoenix dictionary.

use clap::{Parser, Subcommand};

use anyhow::Result;

use std::{collections::BTreeMap, fs::File};
use bbq_steno::stroke::StenoWord;

#[path = "bbq-build/rtfcre.rs"]
mod rtfcre;

#[path = "bbq-build/jsondict.rs"]
mod jsondict;

#[path = "bbq-build/encode.rs"]
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build { output, files } => {
            println!("Building files: {:?}", files);
            let mut dicts = Vec::new();
            for f in files {
                let dict = load_dict(f)?;
                dicts.push(encode::encode_dict(&dict)?);
            }

            println!("Output will be written to: {}", output);

            // For now, just concatenate them, but really need a helper to
            // put the group header.
            let mut fd = File::create(output)?;

            let slices: Vec<_> = dicts.iter().map(|d| d.as_slice()).collect();
            encode::write_group(&mut fd, &slices)?;
        }
        Commands::Show { filename } => {
            println!("Showing file: {}", filename);
            // Add logic to display the file contents here
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
