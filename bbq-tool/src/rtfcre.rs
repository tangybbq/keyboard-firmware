//! RTFCRE import.

use bbq_steno::stroke::StenoWord;
use regex::Regex;

use crate::Result;

use std::{path::Path, io::{BufReader, Read, Bytes}, fs::File, collections::BTreeMap};

struct Tokens {
    file: Bytes<BufReader<File>>,
    peeked: Option<char>,
}

impl Tokens {
    // Turns out that RTFCRE is not UTF-8. Just treat the bytes as chars, as if
    // this were "latin 1".
    fn next_char(&mut self) -> Option<Result<char>> {
        match self.file.next() {
            Some(Ok(ch)) => Some(Ok(ch as char)),
            Some(Err(e)) => Some(Err(e.into())),
            None => None,
        }
    }

    #[allow(dead_code)]
    fn not_next_char(&mut self) -> Option<Result<char>> {
        let first = self.file.next()?.ok()?;
        let mut buf = [first; 4];
        let len = utf8_char_width(first);
        for i in 1..len {
            buf[i] = match self.file.next() {
                Some(Ok(b)) => b,
                _ => return None,
            };
        }

        match std::str::from_utf8(&buf[..len]) {
            Ok(s) => Some(Ok(s.chars().next().unwrap())),
            Err(_) => None, // Better?
        }
    }
}

fn utf8_char_width(byte: u8) -> usize {
    if byte & 0x80 == 0 { 1 }
    else if byte & 0xe0 == 0xc0 { 2 }
    else if byte & 0xf0 == 0xe0 { 3 }
    else if byte & 0xf8 == 0xf0 { 4 }
    else { 1 } // Handle invalid UTF-8 as single byte.
}

#[derive(Debug)]
enum Token {
    Open,
    Close,
    Command(String),
    Text(String),
}

impl Token {
    fn is_open(&self) -> bool {
        match self {
            Token::Open => true,
            _ => false,
        }
    }

    fn is_close(&self) -> bool {
        match self {
            Token::Close => true,
            _ => false,
        }
    }

    fn is_command(&self) -> bool {
        match self {
            Token::Command(_) => true,
            _ => false,
        }
    }

    fn is_text(&self) -> bool {
        match self {
            Token::Text(_) => true,
            _ => false,
        }
    }

    fn text(&self) -> &str {
        match self {
            Token::Text(t) => t,
            Token::Command(t) => t,
            _ => panic!("Invalid token for text")
        }
    }

    fn into_text(self) -> String {
        match self {
            Token::Text(t) => t,
            Token::Command(t) => t,
            _ => panic!("Invalid token for text")
        }
    }
}

impl Iterator for Tokens {
    type Item = Result<Token>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut command = false;
        let ch = std::mem::replace(&mut self.peeked, None);
        let ch = match ch {
            Some(ch) => ch,
            None => {
                match self.next_char()? {
                    Ok(ch) => ch,
                    Err(e) => return  Some(Err(e)),
                }
            }
        };
        match ch {
            '{' => return Some(Ok(Token::Open)),
            '}' => return Some(Ok(Token::Close)),
            '\\' => command = true,
            _ => (),
        }

        // Absorb text or command until we are done.
        let mut buf = String::new();
        if !command {
            buf.push(ch);
        }

        loop {
            let ch = match self.next_char() {
                Some(Ok(ch)) => ch,
                Some(Err(e)) => return Some(Err(e)),
                None => return None, // TODO: Discarding token at end?
            };
            if ch == '{' || ch == '}' || ch == '\\' ||
                (command && (ch == ' '))
            {
                if !command || ch != ' ' {
                    self.peeked = Some(ch);
                }
                break;
            }
            buf.push(ch);
        }
        if command {
            Some(Ok(Token::Command(buf)))
        } else {
            Some(Ok(Token::Text(buf)))
        }
    }
}

pub fn import<P: AsRef<Path>>(name: P) -> Result<BTreeMap<StenoWord, String>> {
    let p = Tokens {
        file: BufReader::new(File::open(name)?).bytes(),
        peeked: None,
    };
    let mut state = 0;
    let mut dict = BTreeMap::new();
    let mut last = String::new();
    let mut defn = Vec::new();
    let mut skipped = 0;
    let encoder = Encoder::new();
    for tok in p {
        let tok = tok?;

        // Open \* \cxs Text Close Text ... until next open.
        //  1   2    3   4    5
        // In state 5, we might see 'open' that aren't followed by the \*, which
        // should continue to build this definition.
        // println!("state: {}, tok: {:?}", state, tok);
        match state {
            0 => {
                if tok.is_open() {
                    state = 1;
                } else {
                    defn.push(tok);
                }
            }
            1 => {
                if tok.is_command() && tok.text() == "*" {
                    state = 2;
                } else if tok.is_open() {
                    defn.push(tok);
                    state = 1;
                } else {
                    defn.push(tok);
                    state = 0;
                }
            }
            2 => {
                if tok.is_command() && tok.text() == "cxs" {
                    if skipped >= 2 {
                        // println!("defn: {:?} => {:?}", last, defn);
                        let last = StenoWord::parse(&last)?;
                        // dict.insert(last, format!("{:?}", defn));
                        dict.insert(last, encoder.encode(&defn));
                    }
                    skipped += 1;
                    defn.clear();
                    state = 3;
                } else if tok.is_open() {
                    state = 1;
                } else {
                    state = 0;
                }
            }
            3 => {
                if tok.is_text() {
                    last = tok.into_text();
                    state = 4;
                } else {
                    panic!("Impossible state near: {:?}", last);
                }
            }
            4 => {
                if tok.is_close() {
                    state = 0;
                } else {
                    panic!("Impossible state 2 near: {:?}", last);
                }
            }
            /*
            5 => {
                if tok.is_text() {
                    println!("defn: {:?} => {:?}", last, tok.into_text());
                    state = 0;
                } else {
                    println!("defn: {:?} => TODO", last);
                    state = 0;
                }
            }
            */
            _ => unreachable!(),
        }
    }

    // Insert the last definition.
    let last = StenoWord::parse(&last)?;
    // The parser leaves some trailing garbage.
    //println!("Trailing definition: {:?}, {:?} {:?}",
    //         last, defn, encoder.encode(&defn));
    dict.insert(last, encoder.encode(&defn));

    Ok(dict)
}

struct Encoder {
    punct: Regex,
}

impl Encoder {
    fn new() -> Encoder {
        Encoder {
            punct: Regex::new(r"^([\.\?;:,]) ?$").unwrap(),
        }
    }

// Convert the RTFCRE tokens into a dictionary entry.
    fn encode(&self, tokens: &[Token]) -> String {
        let mut result = String::new();

        for token in tokens {
            match token {
                // This shouldn't really be present, but just ignore them if
                // they are.
                Token::Open => (),
                Token::Close => (),
                Token::Command(cmd) if cmd.as_str() == "cxds" => result.push('\x01'),
                Token::Command(cmd) if cmd.as_str() == "cxfc" => {
                    // Cap next should not insert space before, and indicate caps.
                    // Now that interpretation is fixed, remove the delete space, since the space
                    // will only be inserted when there is text.
                    result.push_str("\x02");
                }
                Token::Command(cmd) => {
                    result.push('{');
                    result.push_str(cmd);
                    result.push('}');
                }
                Token::Text(text) => {
                    let text = text.trim_end_matches("\r\n");
                    if let Some(cap) = self.punct.captures(text) {
                        result.push('\x01');
                        result.push_str(&cap[1]);
                        // If the punctuation should cap next, indicate this.
                        // Only these matter here, as the others will have
                        // explicit definitions in the plover phoenix fix
                        // dictionary.
                        if cap[1].starts_with(&['.', '?']) {
                            result.push('\x02');
                        }
                    } else {
                        result.push_str(text);
                    }
                }
            }
        }
        result
    }
}
