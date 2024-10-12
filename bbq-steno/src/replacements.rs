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
//! - 0x05c - Capitalize the previous 'c' words
//! - 0x06c - Lowerize the previous 'c' words
//! - 0x07c - UPCASE the previous 'c' words
//! - 0x08c - Delete previous 'c' spaces
//! - 0x09cx - Replace previous 'c' spaces with character 'x'
//! - 0x0axxx-x0b - Raw press, described by 'x' characters

extern crate alloc;

use alloc::vec::Vec;

#[derive(Debug)]
pub enum Replacement {
    /// Just insert text.
    Text(String),
    /// Delete Space.
    DeleteSpace,
    /// Cap Next
    CapNext,
    /// Stitch
    Stitch,
    /// Force a space, overrides adjacent deletion of space
    ForceSpace,
    /// Act upon previous items.
    Previous(u32, Previous),
    /// Raw action
    Raw(String),
}

/// Previous actions
#[derive(Debug)]
pub enum Previous {
    Capitalize,
    Lowerize,
    Upcase,
    DeleteSpace,
    ReplaceSpace(char),
}

/// Check if the given text contains a raw keypress.
pub fn has_raw(text: &str) -> bool {
    text.contains('\x0a')
}

impl Replacement {
    /// Attempt to build a replacement.  Returns None if there are errors in the replacement,
    /// otherwise it is the decoded string as a vector of replacements.
    pub fn decode(text: &str) -> Option<Vec<Replacement>> {
        let mut result = Vec::new();

        let mut chars = text.chars();

        while let Some(c) = chars.next() {
            match c {
                '\x01' => result.push(Replacement::DeleteSpace),
                '\x02' => result.push(Replacement::CapNext),
                '\x03' => result.push(Replacement::Stitch),
                '\x04' => result.push(Replacement::ForceSpace),
                '\x05' => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Capitalize));
                }
                '\x06' => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Lowerize));
                }
                '\x07' => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::Upcase));
                }
                '\x08' => {
                    let count = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::DeleteSpace));
                }
                '\x09' => {
                    let count = chars.next()?;
                    let next = chars.next()?;
                    result.push(Replacement::Previous(count as u32, Previous::ReplaceSpace(next)));
                }
                '\x0a' => {
                    let mut raw = String::new();
                    loop {
                        let ch = chars.next()?;
                        if ch == '\x0b' {
                            break;
                        }
                        raw.push(ch);
                    }
                    result.push(Replacement::Raw(raw));
                }
                '\x0b' => return None,
                '\x0c' ..= '\x1f' => return None,
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
                Replacement::DeleteSpace => result.push('\x01'),
                Replacement::CapNext => result.push('\x02'),
                Replacement::Stitch => result.push('\x03'),
                Replacement::ForceSpace => result.push('\x04'),
                Replacement::Previous(count, kind) => {
                    match kind {
                        Previous::Capitalize => {
                            result.push('\x05');
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::Lowerize => {
                            result.push('\x06');
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::Upcase => {
                            result.push('\x07');
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::DeleteSpace => {
                            result.push('\x08');
                            result.push(char::from_u32(*count).unwrap());
                        }
                        Previous::ReplaceSpace(with) => {
                            result.push('\x09');
                            result.push(char::from_u32(*count).unwrap());
                            result.push(*with);
                        }
                    }
                }
                Replacement::Raw(raw) => {
                    result.push('\x0a');
                    result.push_str(&raw);
                    result.push('\x0b');
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
