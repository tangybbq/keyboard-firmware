//! Replacements and encodings.
//!
//! The replacement actions and codings are encoding as simple low-value binary values in the
//! dictionary strings.
//!
//! Currently supported values are:
//! - 0x01 - Delete Space
//! - 0x02 - Cap next
//! - 0x03 - Stitch
//! - 0x04 - Force Space
//! - 0x05C - Capitalize the previous 'c' words
//! - 0x06C - Lowerize the previous 'c' words
//! - 0x07C - UPCASE the previous 'c' words
//! - 0x08C - Delete previous 'c' spaces
//! - 0x09Cx - Replace previous 'c' spaces with character 'x'
//! - 0x0a - NL
//! - 0x10xxx-x0b - Raw press, described by 'x' characters
//! - 0x0c - retroactive break
//! - 0x0d - CR
//! - 0x0exxxx0x0b - Number format, template in xxxx.
//! - 0x0f - Upcase next

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

const DELETE_SPACE: char = '\u{0001}';
const CAP_NEXT: char = '\u{0002}';
const STITCH: char = '\u{0003}';
const FORCE_SPACE: char = '\u{0004}';
const UPCASE_NEXT: char = '\u{0005}';
const CAP_PREV: char = '\u{e001}';
const LOWER_PREV: char = '\u{e002}';
const UPPER_PREV: char = '\u{e003}';
const DEL_SPACES: char = '\u{e004}';
const REPL_SPACES: char = '\u{e005}';
const RAW: char = '\u{e006}';
const RETRO_BREAK: char = '\u{e007}';
const BREAK: char = '\u{e008}';
const RETRO_NUM: char = '\u{e00a}';
const RETRO_CURRENCY: char = '\u{e00b}';
const NOCAP_NEXT: char = '\u{e00c}';

const TERM_TEXT: char = '\u{0000}';

#[derive(Debug)]
pub enum Replacement {
    /// Just insert text.
    Text(String),
    /// Delete Space.
    DeleteSpace,
    /// Cap Next
    CapNext,
    /// Don't cap next
    NoCapNext,
    /// Stitch
    Stitch,
    /// Force a space, overrides adjacent deletion of space
    ForceSpace,
    /// Act upon previous items.
    Previous(u32, Previous),
    /// Raw action
    Raw(String),
    /// Retroactive break
    RetroBreak,
    /// Upcase next word.
    UpNext,
}

/// Previous actions
#[derive(Debug)]
pub enum Previous {
    Capitalize,
    Lowerize,
    Upcase,
    DeleteSpace,
    ReplaceSpace(char),
    Number(String),
    Currency(String),
}

impl Replacement {
    /// Attempt to build a replacement.  Returns None if there are errors in the replacement,
    /// otherwise it is the decoded string as a vector of replacements.
    pub fn decode(text: &str) -> Option<Vec<Replacement>> {
        let mut result = Vec::new();

        let mut chars = text.chars();

        while let Some(c) = chars.next() {
            match c {
                DELETE_SPACE => result.push(Replacement::DeleteSpace),
                CAP_NEXT => result.push(Replacement::CapNext),
                STITCH => result.push(Replacement::Stitch),
                FORCE_SPACE => result.push(Replacement::ForceSpace),
                UPCASE_NEXT => result.push(Replacement::UpNext),
                NOCAP_NEXT => result.push(Replacement::NoCapNext),
                CAP_PREV => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Capitalize));
                }
                LOWER_PREV => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Lowerize));
                }
                UPPER_PREV => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Upcase));
                }
                DEL_SPACES => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::DeleteSpace));
                }
                REPL_SPACES => {
                    let count = chars.next()?;
                    let next = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::ReplaceSpace(next)));
                }
                RAW | RETRO_NUM | RETRO_CURRENCY => {
                    let mut raw = String::new();
                    loop {
                        let ch = chars.next()?;
                        if ch == TERM_TEXT {
                            break;
                        }
                        raw.push(ch);
                    }
                    if c == RAW {
                        result.push(Replacement::Raw(raw));
                    } else if c == RETRO_CURRENCY {
                        result.push(Replacement::Previous(1, Previous::Currency(raw)));
                    } else {
                        result.push(Replacement::Previous(1, Previous::Number(raw)));
                    }
                }
                BREAK => return None,
                RETRO_BREAK => result.push(Replacement::RetroBreak),
                ch => {
                    // Add this character to an existing Text, if there is one, otherwise create a
                    // new text with just this character.
                    if let Some(mut r) = result.pop() {
                        if let Replacement::Text(ref mut text) = r {
                            text.push(ch);
                            result.push(r);
                        } else {
                            // Not text.
                            result.push(r);
                            result.push(Replacement::Text(ch.to_string()));
                        }
                    } else {
                        // Nothing yet, add one character.
                        result.push(Replacement::Text(ch.to_string()));
                    }
                }
            }
        }

        Some(result)
    }

    /// Encode the given replacement into a string.  This panics if the counts are out of bounds for
    /// a char.
    pub fn encode(slice: &[Self]) -> String {
        let mut result = String::new();

        for elt in slice {
            match elt {
                Replacement::DeleteSpace => result.push(DELETE_SPACE),
                Replacement::CapNext => result.push(CAP_NEXT),
                Replacement::NoCapNext => result.push(NOCAP_NEXT),
                Replacement::Stitch => result.push(STITCH),
                Replacement::ForceSpace => result.push(FORCE_SPACE),
                Replacement::RetroBreak => result.push(RETRO_BREAK),
                Replacement::UpNext => result.push(UPCASE_NEXT),
                Replacement::Previous(count, kind) => {
                    match kind {
                        Previous::Capitalize => {
                            result.push(CAP_PREV);
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::Lowerize => {
                            result.push(LOWER_PREV);
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::Upcase => {
                            result.push(UPPER_PREV);
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::DeleteSpace => {
                            result.push(DEL_SPACES);
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::ReplaceSpace(with) => {
                            result.push(REPL_SPACES);
                            result.push(char::from_u32(*count).unwrap());
                            result.push(*with);
                        }
                        Previous::Number(text) => {
                            // TODO: Previous number format, not supported?
                            result.push(RETRO_NUM);
                            result.push_str(&text);
                            result.push(TERM_TEXT);
                        }
                        Previous::Currency(text) => {
                            // TODO: Previous number format, not supported?
                            result.push(RETRO_CURRENCY);
                            result.push_str(&text);
                            result.push(TERM_TEXT);
                        }
                    }
                }
                Replacement::Raw(raw) => {
                    result.push(RAW);
                    result.push_str(&raw);
                    result.push(TERM_TEXT);
                }
                Replacement::Text(text) => result.push_str(text),
            }
        }

        result
    }
}

#[cfg(test)]
mod testing {
    use crate::Replacement;

    fn roundtrip(text: &str) {
        let repl = Replacement::decode(text).unwrap();
        println!("{:?} -> {:?}", text, repl);
        let text2 = Replacement::encode(&repl);
        assert_eq!(text2, text);
    }

    #[test]
    fn test_roundtrip() {
        roundtrip("This is plain text.");
        roundtrip("aa \x01 bb \x02 cc \x03 dd \x04 ee \x05\x01 ff \x06\x02 gg \x07\x03 hh \x08\x04 ii");
        roundtrip("aa \x09\x01_ bb \x0aS-w\x0b cc");
    }
}
