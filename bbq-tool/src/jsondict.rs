//! JSON dictionary loading.

use std::{collections::BTreeMap, fs::File, path::Path};

use bbq_steno::{replacements::Previous, stroke::StenoWord, Replacement};
use regex::Regex;

use crate::Result;

pub fn import<P: AsRef<Path>>(name: P) -> Result<BTreeMap<StenoWord, String>> {
    let new: BTreeMap<String, String> = serde_json::from_reader(
        File::open(name)?
    )?;
    let fixer = JsonFixer::new();

    let mut dict = BTreeMap::new();

    for (k, v) in new.iter() {
        let key = StenoWord::parse(k)?;
        dict.insert(key, fixer.fix(k, v));
    }

    Ok(dict)
}

struct JsonFixer {
    pat: Regex,
    stitch: Regex,
    command: Regex,
    unspaced: Regex,
    raw: Regex,
}

impl JsonFixer {
    fn new() -> JsonFixer {
        JsonFixer {
            pat: Regex::new(r"^([^\\{]|\\\{)+|(\{([^{}\\]|\\[{}\\])*\})").unwrap(),
            stitch: Regex::new(r"^\{\&(.*)\}$").unwrap(),
            command: Regex::new(r"^\{(:?[a-zA-Z_]+):(.*)\}$").unwrap(),
            unspaced: Regex::new(r"(?s)^\{(\^)?(~\|)?([^^]*)(\^)?\}$").unwrap(),
            raw: Regex::new(r"^\{#(.*)\}$").unwrap(),
        }
    }

    fn fix(&self, k: &str, text: &str) -> String {
        let mut work = Vec::new();

        let mut start = 0;
        while start < text.len() {
            if let Some(mat) = self.pat.find(&text[start..]) {
                let piece = mat.as_str();
                if piece.starts_with('{') {
                    self.control(&mut work, piece);
                } else {
                    work.push(Replacement::Text(piece.to_string()));
                }
                // println!("  {:?} ({})", mat.as_str(), mat.len());
                start += mat.len();
            } else {
                println!("!!! Mismatch {:?} ({:?})", &text[start..], k);
                break;
            }
        }

        Replacement::encode(&work)
    }

    fn control(&self, work: &mut Vec<Replacement>, text: &str) {
        if text == "{*}" {
            work.push(Replacement::RetroBreak);
            return;
        }
        if text == "{^ ^}" {
            // This needs some thought, as this is usually preceeded by a literal space, so a
            // no-space directive is probably the right thing to do. I'm not sure the purpose of
            // this whole directive in plover.
            work.push(Replacement::DeleteSpace);
            return;
        }

        if text == "{^}" {
            work.push(Replacement::DeleteSpace);
            return;
        }

        if text == "{*-|}" {
            work.push(Replacement::Previous(1, Previous::Capitalize));
            return;
        }

        if text == "{*>}" {
            work.push(Replacement::Previous(1, Previous::Lowerize));
            return;
        }

        if text == "{?}" {
            work.push(Replacement::DeleteSpace);
            work.push(Replacement::Text("?".to_string()));
            work.push(Replacement::CapNext);
            return;
        }

        if text == "{-|}" {
            work.push(Replacement::CapNext);
            return;
        }

        if text == "{*<}" {
            work.push(Replacement::Previous(1, Previous::Upcase));
            return;
        }

        if let Some(caps) = self.stitch.captures(text) {
            // println!("stitch: {:?}", &caps[1]);
            work.push(Replacement::Stitch);
            work.push(Replacement::Text(caps[1].to_string()));
            return;
        }

        if let Some(caps) = self.command.captures(text) {
            let command = &caps[1];
            let arg = &caps[2];
            match command {
                ":retro_title" => {
                    let arg: u32 = arg.parse().unwrap();
                    work.push(Replacement::Previous(arg, Previous::Capitalize));
                }
                ":retro_lower" => {
                    let arg: u32 = arg.parse().unwrap();
                    work.push(Replacement::Previous(arg, Previous::Lowerize));
                }
                ":retro_upper" => {
                    let arg: u32 = arg.parse().unwrap();
                    work.push(Replacement::Previous(arg, Previous::Upcase));
                }
                ":retro_replace_space" => {
                    if let Some((count, text)) = arg.split_once(':') {
                        let count: u32 = count.parse().unwrap();
                        let ch = text.chars().next().unwrap_or('\u{0000}');
                        work.push(Replacement::Previous(count, Previous::ReplaceSpace(ch)));
                    }
                }
                ":number_format_insert" => {
                    work.push(Replacement::Previous(1, Previous::Number(arg.to_string())));
                }
                ":number_format_roman" => {
                    work.push(Replacement::Text("<TODO:ROMAN>".to_string()));
                }
                ":retro_insert_currency" => {
                    work.push(Replacement::Previous(1, Previous::Currency(arg.to_string())));
                }

                // Do we want to support any PLOVER or MODE commands?
                "PLOVER" | "MODE" => {
                    work.push(Replacement::Text(format!("#<{}:{}>", command, arg)));
                }
                command => {
                    work.push(Replacement::Text(format!("#<{}:{}>", command, arg)));
                    println!("  cmd:{:?}, arg:{:?}", command, arg);
                }
            }
            // TODO: Do different things.
            return;
        }

        if let Some(caps) = self.raw.captures(text) {
            let body = &caps[1];
            work.push(Replacement::Raw(body.to_string()));
            return;
        }


        if let Some(caps) = self.unspaced.captures(text) {
            if caps.get(1).is_some() {
                work.push(Replacement::DeleteSpace);
            }
            let body = &caps[3];
            work.push(Replacement::Text(body.to_string()));
            if caps.get(4).is_some() {
                work.push(Replacement::DeleteSpace);
            }
            return;
        }

        println!("Control: {:?}", text);
        work.push(Replacement::Text(text.to_string()));
    }
}
