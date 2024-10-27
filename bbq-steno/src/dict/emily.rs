//! Port of Emily's symbols to Rust.
//!
//! Emily's Symbols are effectively a single entry mapping between strokes and definitions.  It is
//! more efficient to implement, though, programmatically than to generate a dictionary with this
//! entries.  Much of this is about masking and treating different parts of the stroke as different
//! commands.
//!
//! I have modified the stock Emily's symbols to use the '^' key as the distinguisher.  Normally,
//! these use a somewhat difficult to type left hand prefix of "SKWH", as it needs to be distinct
//! from any entry used in the user's dictionary.
//!
//! A symbol stroke consists of a stroke where the left side of the stroke, not counting the vowels,
//! consists of just one of the starters.
//!
//! The AO keys, respectively, indicate which side of the symbol should have a space added.  This is
//! configured in emily's, but I have selected "space" in this code.  This shouldn't be too
//! difficult to add if that is needed.
//!
//! The EU keys select between four variants of each symbol.  Note that many of these variants are
//! defined as as unicode characters which have mixed support in the keyboard firmware, and are
//! generally dead.
//!
//! The '*' key added will ask for the following text to be capitalized.
//!
//! Lastly, the TS keys indicate from 1-4 repititions of the symbol.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// Unfortunately, we can't use the bbq-steno-macros crate, as that would create a circular
// dependency.  Instead, these constants are auto-generated, and placed in the consts module beneath
// this one.
use consts::*;
use log::warn;

use crate::{Replacement, Stroke};

use super::{DictImpl, Selector};

mod consts;

#[derive(Debug)]
struct Decoded {
    // The translation.
    xlat: String,
}

impl Decoded {
    /// Attempt to decode this stroke as an Emily symbol.
    ///
    /// Will return the decoded information structure when the stroke matches, and None if this is
    /// not a stroke for that.
    fn decode(stroke: Stroke) -> Option<Decoded> {
        let starter = stroke & STARTER;
        // warn!("Emily lookup {}: Starter: {}", stroke, starter);
        if starter != START1 && starter != START2 && starter != START3 {
            return None
        }

        let pre_space = (stroke & PRE_SPACE) == PRE_SPACE;
        let post_space = (stroke & POST_SPACE) == POST_SPACE;

        let variant = match stroke & VARIANT_MASK {
            VARIANT_0 => 0,
            VARIANT_1 => 1,
            VARIANT_2 => 2,
            VARIANT_3 => 3,
            _ => unreachable!(),
        };

        let repeat = match stroke & REPEAT_MASK {
            REPEAT_1 => 1,
            REPEAT_2 => 2,
            REPEAT_3 => 3,
            REPEAT_4 => 4,
            _ => unreachable!(),
        };

        let cap_next = (stroke & CAP_NEXT) == CAP_NEXT;

        let mut body = None;
        for ent in &CODES {
            if (stroke & CODE_MASK) == ent.from {
                body = Some(&ent.to[variant]);
            }
        }

        let mut buf = Vec::new();

        if pre_space {
            buf.push(Replacement::ForceSpace);
        } else {
            buf.push(Replacement::DeleteSpace);
        }

        match body {
            // No match, don't match the entire stroke.
            None => return None,
            Some(Kind::Text(txt)) => {
                let mut text = String::new();
                for _ in 0..repeat {
                    text.push_str(txt);
                }
                buf.push(Replacement::Text(text));
            }
            Some(Kind::Raw(txt)) => {
                for _ in 0..repeat {
                    buf.push(Replacement::Raw(txt.to_string()));
                }
            }
        }

        if post_space {
            buf.push(Replacement::ForceSpace);
        } else {
            buf.push(Replacement::DeleteSpace);
        }

        if cap_next {
            buf.push(Replacement::CapNext);
        }

        Some(Decoded {
            xlat: Replacement::encode(&buf),
        })
    }

    fn translation(&self) -> String {
        self.xlat.clone()
    }
}

static CODES: [Entry; 34] = [
    Entry { from: CODE_TAB, to: [
        Kind::Raw("Tab"), Kind::Raw("BackSpace"), Kind::Raw("Delete"), Kind::Raw("Escape"),
    ]},
    Entry { from: CODE_UP, to: [
        Kind::Raw("Up"), Kind::Raw("Left"), Kind::Raw("Right"), Kind::Raw("Down"),
    ]},
    Entry { from: CODE_PGUP, to: [
        Kind::Raw("PageUp"), Kind::Raw("Home"), Kind::Raw("End"), Kind::Raw("PageDown"),
    ]},
    Entry { from: CODE_AUDIOPLAY, to: [
        Kind::Raw("AudioPlay"), Kind::Raw("AudioPrev"), Kind::Raw("AudioNext"), Kind::Raw("AudioStop"),
    ]},
    Entry { from: CODE_AUDIOMUTE, to: [
        Kind::Raw("AudioMute"), Kind::Raw("AudioLowerVolume"), Kind::Raw("AudioRaiseVolume"), Kind::Raw("Eject"),
    ]},
    Entry { from: CODE_SPACE, to: [
        Kind::Text(""), Kind::Text("{*!}"), Kind::Text("{:*}"), Kind::Text("Space"),
    ]},
    Entry { from: CODE_CAPS, to: [
        Kind::Text("{*-|}"), Kind::Text("{*<}"), Kind::Text("{<}"), Kind::Text("*>"),
    ]},

    Entry { from: CODE_BANG, to: [
        Kind::Text("!"), Kind::Text("¬"), Kind::Text("↦"), Kind::Text("¡")
    ]},
    Entry { from: CODE_QUOTE, to: [
        Kind::Text("\""), Kind::Text("“"), Kind::Text("”"), Kind::Text("„")
    ]},
    Entry { from: CCODE_HASH, to: [
        Kind::Text("#"), Kind::Text("©"), Kind::Text("®"), Kind::Text("™")
    ]},
    Entry { from: CODE_DOLLAR, to: [
        Kind::Text("$"), Kind::Text("¥"), Kind::Text("€"), Kind::Text("£")
    ]},
    Entry { from: CODE_PERCENT, to: [
        Kind::Text("%"), Kind::Text("‰"), Kind::Text("‱"), Kind::Text("φ")
    ]},
    Entry { from: CODE_AND, to: [
        Kind::Text("&"), Kind::Text("∩"), Kind::Text("∧"), Kind::Text("∈")
    ]},
    Entry { from: CODE_APOST, to: [
        Kind::Text("'"), Kind::Text("‘"), Kind::Text("’"), Kind::Text("‚")
    ]},
    Entry { from: CODE_LPAREN, to: [
        Kind::Text("("), Kind::Text("["), Kind::Text("<"), Kind::Text("{")
    ]},
    Entry { from: CODE_RPAREN, to: [
        Kind::Text(")"), Kind::Text("]"), Kind::Text(">"), Kind::Text("}")
    ]},
    Entry { from: CODE_STAR, to: [
        Kind::Text("*"), Kind::Text("∏"), Kind::Text("§"), Kind::Text("×")
    ]},
    Entry { from: CODE_PLUS, to: [
        Kind::Text("+"), Kind::Text("∑"), Kind::Text("¶"), Kind::Text("±")
    ]},
    Entry { from: CODE_COMMA, to: [
        Kind::Text(","), Kind::Text("∪"), Kind::Text("∨"), Kind::Text("∉")
    ]},
    Entry { from: CODE_MINUS, to: [
        Kind::Text("-"), Kind::Text("−"), Kind::Text("–"), Kind::Text("—")
    ]},
    Entry { from: CODE_DOT, to: [
        Kind::Text("."), Kind::Text("•"), Kind::Text("·"), Kind::Text("…")
    ]},
    Entry { from: CODE_SLASH, to: [
        Kind::Text("/"), Kind::Text("⇒"), Kind::Text("⇔"), Kind::Text("÷")
    ]},
    Entry { from: CODE_COLON, to: [
        Kind::Text(":"), Kind::Text("∋"), Kind::Text("∵"), Kind::Text("∴")
    ]},
    Entry { from: CODE_SEMI, to: [
        Kind::Text(";"), Kind::Text("∀"), Kind::Text("∃"), Kind::Text("∄")
    ]},
    Entry { from: CODE_EQUAL, to: [
        Kind::Text("="), Kind::Text("≡"), Kind::Text("≈"), Kind::Text("≠")
    ]},
    Entry { from: CODE_QUEST, to: [
        Kind::Text("?"), Kind::Text("¿"), Kind::Text("∝"), Kind::Text("‽")
    ]},
    Entry { from: CODE_AT, to: [
        Kind::Text("@"), Kind::Text("⊕"), Kind::Text("⊗"), Kind::Text("∅")
    ]},
    Entry { from: CODE_BSLASH, to: [
        Kind::Text("\\"), Kind::Text("Δ"), Kind::Text("√"), Kind::Text("∞")
    ]},
    Entry { from: CODE_CARET, to: [
        Kind::Text("^"), Kind::Text("«"), Kind::Text("»"), Kind::Text("°")
    ]},
    Entry { from: CODE_UNDER, to: [
        Kind::Text("_"), Kind::Text("≤"), Kind::Text("≥"), Kind::Text("µ")
    ]},
    Entry { from: CODE_GRAVE, to: [
        Kind::Text("`"), Kind::Text("⊂"), Kind::Text("⊃"), Kind::Text("π")
    ]},
    Entry { from: CODE_PIPE, to: [
        Kind::Text("|"), Kind::Text("⊤"), Kind::Text("⊥"), Kind::Text("¦")
    ]},
    Entry { from: CODE_TILDE, to: [
        Kind::Text("~"), Kind::Text("⊆"), Kind::Text("⊇"), Kind::Text("˜")
    ]},
    Entry { from: CODE_ARROW, to: [
        Kind::Text("↑"), Kind::Text("←"), Kind::Text("→"), Kind::Text("↓")
    ]},
];

struct Entry {
    from: Stroke,
    to: [Kind; 4],
}

enum Kind {
    Text(&'static str),
    Raw(&'static str),
}

impl Selector for Decoded {
    fn lookup_step(&self, _key: Stroke) -> Option<(Box<dyn Selector>, Option<String>)> {
        // There are never additional strokes.
        warn!("Decoded lookup step");
        None
    }

    fn unique(&self) -> bool {
        warn!("Decoded lookup");
        // There is always a single result
        true
    }

    fn count(&self) -> usize {
        warn!("Decoded count");
        // And always 1 result.
        1
    }

    fn dump(&self) {
    }
}

/// Emily's dictionary itself is just a marker.
pub struct EmilySymbols;

impl DictImpl for EmilySymbols {
    fn len(&self) -> usize {
        // Entirely faked
        500
    }

    fn key(&self, _index: usize) -> &[Stroke] {
        todo!()
    }

    fn value(&self, _index: usize) -> &str {
        todo!()
    }

    fn selector(self: Rc<Self>) -> Box<dyn Selector> {
        Box::new(RootSelector)
    }

    // Scan shouldn't be called.
    fn scan(&self, _a: usize, _b: usize, _pos: usize, _needle: Stroke) -> usize {
        unreachable!()
    }
}

#[derive(Debug)]
struct RootSelector;

impl Selector for RootSelector {
    fn lookup_step(&self, key: Stroke) -> Option<(Box<dyn Selector>, Option<String>)> {
        match Decoded::decode(key) {
            None => None,
            Some(x) => {
                let translation = x.translation();
                Some((Box::new(x), Some(translation)))
            }
        }
    }

    fn unique(&self) -> bool {
        false
    }

    fn count(&self) -> usize {
        500
    }

    fn dump(&self) {
        todo!()
    }
}
