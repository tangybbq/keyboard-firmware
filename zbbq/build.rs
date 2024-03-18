// Prototype for getting kconfig and DT information.

use std::io::{BufRead, BufReader};
use std::env;
use std::fs::File;

use regex::Regex;

fn main() {
    let dotconfig = env::var("DOTCONFIG").expect("DOTCONFIG must be set by wrapper");

    // Ensure that the build script is rerun when the dotconfig changes.
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    println!("cargo:rerun-if-changed={}", dotconfig);

    let config_y = Regex::new(r"^(CONFIG_.*)=y$").unwrap();

    let file = File::open(dotconfig).expect("Unable to open dotconfig");
    for line in BufReader::new(file).lines() {
        let line = line.expect("reading line from dotconfig");
        if let Some(caps) = config_y.captures(&line) {
            println!("cargo:rustc-cfg={}", &caps[1]);
        }
    }
}
