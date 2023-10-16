//! Modifiers lookup
//!
//! Modified Emily's Modifier Dictionary
//!
//! I have started with Emily's Modifier Dictionary, and made two basic changes
//! to it:
//!
//! - I use '+' as the unique ending which avoids needing to have an awkward
//!   part of the stroke.
//! - Instead of '*' for the symbols, I use "-G", this is easier with putting
//!   the star on the top 'S' key.

use bbq_steno::stroke::Stroke;
use bbq_steno_macros::stroke;

extern crate alloc;
use alloc::string::String;
use crate::log::info;

pub struct Modifiers;

impl Modifiers {
    pub fn new() -> Modifiers {
        Modifiers
    }

    // The mask is mostly the Carret.
    const SELECT_MASK: Stroke = stroke!("+-G");
    const SELECT: Stroke = stroke!("+");
    const SELECT_SYMBOL: Stroke = stroke!("+-G");
    const LEFT: Stroke = stroke!("STKPWHRAOEU");

    // TODO: Improve this beyond just a linear search. We probably want to be
    // able to build simple dictionaries at compile time.
    const LETTERS: &[SimpleEntry] = &[
        SimpleEntry { stroke: stroke!("A"), text: "a" },
        SimpleEntry { stroke: stroke!("PW"), text: "b" },
        SimpleEntry { stroke: stroke!("KR"), text: "c" },
        SimpleEntry { stroke: stroke!("TK"), text: "d" },
        SimpleEntry { stroke: stroke!("E"), text: "e" },
        SimpleEntry { stroke: stroke!("TP"), text: "f" },
        SimpleEntry { stroke: stroke!("TKPW"), text: "g" },
        SimpleEntry { stroke: stroke!("H"), text: "h" },
        SimpleEntry { stroke: stroke!("EU"), text: "i" },
        SimpleEntry { stroke: stroke!("SKWR"), text: "j" },
        SimpleEntry { stroke: stroke!("K"), text: "k" },
        SimpleEntry { stroke: stroke!("HR"), text: "l" },
        SimpleEntry { stroke: stroke!("PH"), text: "m" },
        SimpleEntry { stroke: stroke!("TPH"), text: "n" },
        SimpleEntry { stroke: stroke!("O"), text: "o" },
        SimpleEntry { stroke: stroke!("P"), text: "p" },
        SimpleEntry { stroke: stroke!("KW"), text: "q" },
        SimpleEntry { stroke: stroke!("R"), text: "r" },
        SimpleEntry { stroke: stroke!("S"), text: "s" },
        SimpleEntry { stroke: stroke!("T"), text: "t" },
        SimpleEntry { stroke: stroke!("U"), text: "u" },
        SimpleEntry { stroke: stroke!("SR"), text: "v" },
        SimpleEntry { stroke: stroke!("W"), text: "w" },
        SimpleEntry { stroke: stroke!("KP"), text: "x" },
        SimpleEntry { stroke: stroke!("KWR"), text: "y" },
        SimpleEntry { stroke: stroke!("STKPW"), text: "z" },
    ];

    /// Perform a basic lookup of a single stroke.
    pub fn lookup(&self, stroke: Stroke) -> Option<String> {
        let right = stroke & Self::SELECT_MASK;
        info!("right: {}", right.into_raw());
        if right != Self::SELECT && right != Self::SELECT_SYMBOL {
            return None;
        }
        let left = stroke & Self::LEFT;

        info!("left: {}", left.into_raw());
        if right == Self::SELECT {
            if let Some(elt) = Self::LETTERS.iter().find(|l| l.stroke == left) {
                // TODO: match letters.
                return Some(String::from(elt.text));
            } else {
                return None;
            }
        }
        None
    }
}

// A simple mapping.
struct SimpleEntry {
    stroke: Stroke,
    text: &'static str,
}
