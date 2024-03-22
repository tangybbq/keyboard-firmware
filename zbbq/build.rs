// Prototype for getting kconfig and DT information.

use std::io::{BufRead, BufReader, Write};
use std::env;
use std::fs::File;
use std::path::Path;

use regex::Regex;

fn main() {
    let dotconfig = env::var("DOTCONFIG").expect("DOTCONFIG must be set by wrapper");
    let outdir = env::var("OUT_DIR").expect("OUT_DIR must be set");

    // Ensure that the build script is rerun when the dotconfig changes.
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    println!("cargo:rerun-if-changed={}", dotconfig);

    let config_y = Regex::new(r"^(CONFIG_.*)=y$").unwrap();

    let file = File::open(&dotconfig).expect("Unable to open dotconfig");
    for line in BufReader::new(file).lines() {
        let line = line.expect("reading line from dotconfig");
        if let Some(caps) = config_y.captures(&line) {
            println!("cargo:rustc-cfg={}", &caps[1]);
        }
    }

    // Capture all of the numeric and string settings as constants in a
    // generated module.
    let config_num = Regex::new(r"^(CONFIG_.*)=([1-9][0-9]*|0x[0-9]+)$").unwrap();
    // It is unclear what quoting might be available in the .config
    let config_str = Regex::new(r#"^(CONFIG_.*)=(".*")$"#).unwrap();
    let gen_path = Path::new(&outdir).join("kconfig.rs");

    let mut f = File::create(&gen_path).unwrap();
    writeln!(&mut f, "mod kconfig {{").unwrap();

    let file = File::open(&dotconfig).expect("Unable to open dotconfig");
    for line in BufReader::new(file).lines() {
        let line = line.expect("reading line from dotconfig");
        if let Some(caps) = config_num.captures(&line) {
            writeln!(&mut f, "    #[allow(dead_code)]").unwrap();
            writeln!(&mut f, "    pub const {}: usize = {};",
                     &caps[1], &caps[2]).unwrap();
        }
        if let Some(caps) = config_str.captures(&line) {
            writeln!(&mut f, "    #[allow(dead_code)]").unwrap();
            writeln!(&mut f, "    pub const {}: &'static str = {};",
                     &caps[1], &caps[2]).unwrap();
        }
    }
    writeln!(&mut f, "}}").unwrap();
}
