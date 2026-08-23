//! The Posh chord table.
//!
//! Posh (<https://inkeys.wiki/en/keymaps/posh>) is a Taipo-style chording
//! layout from the same community, described as "a taipo style layout that
//! excludes the pinkies in order to make combos more accurate and long periods
//! of work more comfortable".  Like Taipo, each half of the keyboard is a
//! complete layout, and the two halves are freely alternated.  Unlike Taipo, it
//! uses only 6 finger keys per hand (3 columns of 2 rows) plus the 2 thumbs.
//!
//! Only the table differs from Taipo: the chord accumulation, the chord timing,
//! the modifier handling, and the scan code mapping are all shared, so the same
//! chord code bits are used:
//!
//! | bit     | finger | row    | Posh letter typed alone |
//! |---------|--------|--------|-------------------------|
//! | `0x020` | ring   | top    | `a`                     |
//! | `0x040` | middle | top    | `n`                     |
//! | `0x080` | index  | top    | `i`                     |
//! | `0x002` | ring   | bottom | `o`                     |
//! | `0x004` | middle | bottom | `t`                     |
//! | `0x008` | index  | bottom | `e`                     |
//! | `0x100` | thumb  |        | Space                   |
//! | `0x200` | thumb  |        | Backspace               |
//!
//! The pinky bits (`0x010` and `0x001`, Taipo's `r` and `a`) appear in no entry
//! here, so the pinky keys are dead in Posh, which is the entire point of the
//! layout.
//!
//! Local differences from the wiki:
//!
//! - **The thumbs are Taipo's.**  The wiki has space and backspace swapped
//!   relative to Taipo; the developer wants them where Taipo has them, so
//!   `0x100` is Space and `0x200` is Backspace.  The layers follow the thumb's
//!   *function* rather than its position, exactly as in Taipo: the wiki's
//!   "outer" (capitals) column is `+0x100`, and its "inner" (digits and
//!   symbols) column is `+0x200`.
//! - **Both thumbs alone is the null key** (`Action::Release`), as in Taipo,
//!   rather than the wiki's sticky shift.  Taipo's double-press sticky
//!   modifiers already cover that need.
//! - **`ralt` is Shift.**  [`Mods`] has no right-alt, so the wiki's `ralt`
//!   chord (`0x026`) is a plain Shift one-shot.  Its "both" variant would then
//!   be shift plus shift, so `0x326` is left unmapped.
//! - **`l` has a second chord.**  The wiki's `l` is index-top plus
//!   middle-bottom (`0x084`), a splay the developer finds hard to hit.  The
//!   otherwise unused `n+i+e` chord (`0x0c8`, middle-top plus index-top plus
//!   index-bottom) is mapped to the same four actions.  Both spellings work;
//!   this adds a chord rather than moving one.
//!
//! Left unmapped, as a future task: everything needing a consumer-control HID
//! report, which the firmware does not have — play/pause, next/previous track,
//! stop, volume up/down/mute, and brightness up/down.  The wiki's "layer 0-3"
//! chord is a QMK concept that does not apply here, and the five empty rows at
//! the bottom of the wiki's table have nothing to map.

use usbd_human_interface_device::page::Keyboard;

use crate::Mods;

use super::taipo::{Action, Entry};

/// The mapping between each Posh chord and its action.
pub(super) static POSH_ACTIONS: &[Entry] = &[
    // The thumb keys by themselves, and together as the null key, as in Taipo.
    Entry { code: 0x100, action: Action::Simple(Keyboard::Space), },
    Entry { code: 0x200, action: Action::Simple(Keyboard::DeleteBackspace), },
    Entry { code: 0x300, action: Action::Release, },

    // The six single keys: alone, +Sp (capital), +Bk (navigation), +both.
    Entry { code: 0x008, action: Action::Simple(Keyboard::E), },
    Entry { code: 0x108, action: Action::Shifted(Keyboard::E), },
    Entry { code: 0x208, action: Action::Simple(Keyboard::RightArrow), },
    Entry { code: 0x308, action: Action::Simple(Keyboard::End), },

    Entry { code: 0x004, action: Action::Simple(Keyboard::T), },
    Entry { code: 0x104, action: Action::Shifted(Keyboard::T), },
    Entry { code: 0x204, action: Action::Simple(Keyboard::DownArrow), },
    Entry { code: 0x304, action: Action::Simple(Keyboard::PageDown), },

    Entry { code: 0x020, action: Action::Simple(Keyboard::A), },
    Entry { code: 0x120, action: Action::Shifted(Keyboard::A), },
    Entry { code: 0x220, action: Action::Simple(Keyboard::Escape), },
    Entry { code: 0x320, action: Action::Simple(Keyboard::DeleteForward), },

    Entry { code: 0x002, action: Action::Simple(Keyboard::O), },
    Entry { code: 0x102, action: Action::Shifted(Keyboard::O), },
    Entry { code: 0x202, action: Action::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x302, action: Action::Simple(Keyboard::Home), },

    Entry { code: 0x080, action: Action::Simple(Keyboard::I), },
    Entry { code: 0x180, action: Action::Shifted(Keyboard::I), },
    Entry { code: 0x280, action: Action::Simple(Keyboard::ReturnEnter), },
    Entry { code: 0x380, action: Action::Simple(Keyboard::Tab), },

    Entry { code: 0x040, action: Action::Simple(Keyboard::N), },
    Entry { code: 0x140, action: Action::Shifted(Keyboard::N), },
    Entry { code: 0x240, action: Action::Simple(Keyboard::UpArrow), },
    Entry { code: 0x340, action: Action::Simple(Keyboard::PageUp), },

    // The two same-row pairs, whose symbols are the comma and the quotes.
    Entry { code: 0x0c0, action: Action::Simple(Keyboard::S), },
    Entry { code: 0x1c0, action: Action::Shifted(Keyboard::S), },
    Entry { code: 0x2c0, action: Action::Simple(Keyboard::Comma), },
    Entry { code: 0x3c0, action: Action::Simple(Keyboard::Apostrophe), },

    Entry { code: 0x00c, action: Action::Simple(Keyboard::H), },
    Entry { code: 0x10c, action: Action::Shifted(Keyboard::H), },
    Entry { code: 0x20c, action: Action::Simple(Keyboard::Dot), },
    Entry { code: 0x30c, action: Action::Shifted(Keyboard::Apostrophe), },

    // The letters whose +Bk is a digit, and whose +both is the matching
    // function key.
    Entry { code: 0x048, action: Action::Simple(Keyboard::R), },
    Entry { code: 0x148, action: Action::Shifted(Keyboard::R), },
    Entry { code: 0x248, action: Action::Simple(Keyboard::Keyboard0), },
    Entry { code: 0x348, action: Action::Simple(Keyboard::F10), },

    Entry { code: 0x060, action: Action::Simple(Keyboard::D), },
    Entry { code: 0x160, action: Action::Shifted(Keyboard::D), },
    Entry { code: 0x260, action: Action::Simple(Keyboard::Keyboard1), },
    Entry { code: 0x360, action: Action::Simple(Keyboard::F1), },

    Entry { code: 0x084, action: Action::Simple(Keyboard::L), },
    Entry { code: 0x184, action: Action::Shifted(Keyboard::L), },
    Entry { code: 0x284, action: Action::Simple(Keyboard::Keyboard2), },
    Entry { code: 0x384, action: Action::Simple(Keyboard::F2), },

    // A local alias for the above, because the index-top plus middle-bottom
    // splay is awkward to hit.  Same actions, on the free `n+i+e` chord.
    Entry { code: 0x0c8, action: Action::Simple(Keyboard::L), },
    Entry { code: 0x1c8, action: Action::Shifted(Keyboard::L), },
    Entry { code: 0x2c8, action: Action::Simple(Keyboard::Keyboard2), },
    Entry { code: 0x3c8, action: Action::Simple(Keyboard::F2), },

    Entry { code: 0x00a, action: Action::Simple(Keyboard::C), },
    Entry { code: 0x10a, action: Action::Shifted(Keyboard::C), },
    Entry { code: 0x20a, action: Action::Simple(Keyboard::Keyboard3), },
    Entry { code: 0x30a, action: Action::Simple(Keyboard::F3), },

    Entry { code: 0x006, action: Action::Simple(Keyboard::U), },
    Entry { code: 0x106, action: Action::Shifted(Keyboard::U), },
    Entry { code: 0x206, action: Action::Simple(Keyboard::Keyboard4), },
    Entry { code: 0x306, action: Action::Simple(Keyboard::F4), },

    Entry { code: 0x028, action: Action::Simple(Keyboard::M), },
    Entry { code: 0x128, action: Action::Shifted(Keyboard::M), },
    Entry { code: 0x228, action: Action::Simple(Keyboard::Keyboard5), },
    Entry { code: 0x328, action: Action::Simple(Keyboard::F5), },

    Entry { code: 0x082, action: Action::Simple(Keyboard::W), },
    Entry { code: 0x182, action: Action::Shifted(Keyboard::W), },
    Entry { code: 0x282, action: Action::Simple(Keyboard::Keyboard6), },
    Entry { code: 0x382, action: Action::Simple(Keyboard::F6), },

    Entry { code: 0x0a0, action: Action::Simple(Keyboard::F), },
    Entry { code: 0x1a0, action: Action::Shifted(Keyboard::F), },
    Entry { code: 0x2a0, action: Action::Simple(Keyboard::Keyboard7), },
    Entry { code: 0x3a0, action: Action::Simple(Keyboard::F7), },

    Entry { code: 0x042, action: Action::Simple(Keyboard::G), },
    Entry { code: 0x142, action: Action::Shifted(Keyboard::G), },
    Entry { code: 0x242, action: Action::Simple(Keyboard::Keyboard8), },
    Entry { code: 0x342, action: Action::Simple(Keyboard::F8), },

    Entry { code: 0x0c2, action: Action::Simple(Keyboard::Y), },
    Entry { code: 0x1c2, action: Action::Shifted(Keyboard::Y), },
    Entry { code: 0x2c2, action: Action::Simple(Keyboard::Keyboard9), },
    Entry { code: 0x3c2, action: Action::Simple(Keyboard::F9), },

    // The letters whose +Bk and +both are symbols.
    Entry { code: 0x0e0, action: Action::Simple(Keyboard::P), },
    Entry { code: 0x1e0, action: Action::Shifted(Keyboard::P), },
    Entry { code: 0x2e0, action: Action::Shifted(Keyboard::Equal), },      // +
    Entry { code: 0x3e0, action: Action::Simple(Keyboard::Equal), },       // =

    Entry { code: 0x00e, action: Action::Simple(Keyboard::B), },
    Entry { code: 0x10e, action: Action::Shifted(Keyboard::B), },
    Entry { code: 0x20e, action: Action::Simple(Keyboard::Minus), },       // -
    Entry { code: 0x30e, action: Action::Shifted(Keyboard::Minus), },      // _

    Entry { code: 0x04a, action: Action::Simple(Keyboard::V), },
    Entry { code: 0x14a, action: Action::Shifted(Keyboard::V), },
    Entry { code: 0x24a, action: Action::Simple(Keyboard::ForwardSlash), },// /
    Entry { code: 0x34a, action: Action::Simple(Keyboard::Backslash), },   // \

    Entry { code: 0x068, action: Action::Simple(Keyboard::K), },
    Entry { code: 0x168, action: Action::Shifted(Keyboard::K), },
    Entry { code: 0x268, action: Action::Simple(Keyboard::Semicolon), },   // ;
    Entry { code: 0x368, action: Action::Shifted(Keyboard::Backslash), },  // |

    Entry { code: 0x086, action: Action::Simple(Keyboard::J), },
    Entry { code: 0x186, action: Action::Shifted(Keyboard::J), },
    Entry { code: 0x286, action: Action::Shifted(Keyboard::Semicolon), },  // :
    Entry { code: 0x386, action: Action::Shifted(Keyboard::Keyboard8), },  // *

    Entry { code: 0x02c, action: Action::Simple(Keyboard::X), },
    Entry { code: 0x12c, action: Action::Shifted(Keyboard::X), },
    Entry { code: 0x22c, action: Action::Shifted(Keyboard::Keyboard4), },  // $
    Entry { code: 0x32c, action: Action::Shifted(Keyboard::Keyboard3), },  // #

    Entry { code: 0x02a, action: Action::Simple(Keyboard::Q), },
    Entry { code: 0x12a, action: Action::Shifted(Keyboard::Q), },
    Entry { code: 0x22a, action: Action::Shifted(Keyboard::Keyboard2), },  // @
    Entry { code: 0x32a, action: Action::Simple(Keyboard::F11), },

    Entry { code: 0x04c, action: Action::Simple(Keyboard::Z), },
    Entry { code: 0x14c, action: Action::Shifted(Keyboard::Z), },
    Entry { code: 0x24c, action: Action::Shifted(Keyboard::Keyboard7), },  // &
    Entry { code: 0x34c, action: Action::Simple(Keyboard::F12), },

    // The punctuation-only chords, which have no +both.
    Entry { code: 0x024, action: Action::Shifted(Keyboard::ForwardSlash), },// ?
    Entry { code: 0x124, action: Action::Shifted(Keyboard::Keyboard1), },   // !
    Entry { code: 0x224, action: Action::Shifted(Keyboard::Keyboard6), },   // ^

    Entry { code: 0x08a, action: Action::Simple(Keyboard::Grave), },        // `
    Entry { code: 0x18a, action: Action::Shifted(Keyboard::Grave), },       // ~
    Entry { code: 0x28a, action: Action::Shifted(Keyboard::Keyboard5), },   // %

    // The modifiers, which are the same-finger vertical pairs, plus the wiki's
    // `ralt` chord, which is Shift here.  Their layers are the brackets.
    Entry { code: 0x044, action: Action::OneShot(Mods::GUI), },
    Entry { code: 0x144, action: Action::Shifted(Keyboard::Keyboard0), },   // )
    Entry { code: 0x244, action: Action::Shifted(Keyboard::Keyboard9), },   // (
    Entry { code: 0x344, action: Action::OneShot(Mods::GUI.union(Mods::SHIFT)), },

    Entry { code: 0x088, action: Action::OneShot(Mods::CONTROL), },
    Entry { code: 0x188, action: Action::Simple(Keyboard::RightBrace), },   // ]
    Entry { code: 0x288, action: Action::Simple(Keyboard::LeftBrace), },    // [
    Entry { code: 0x388, action: Action::OneShot(Mods::CONTROL.union(Mods::SHIFT)), },

    Entry { code: 0x022, action: Action::OneShot(Mods::ALT), },
    Entry { code: 0x122, action: Action::Shifted(Keyboard::RightBrace), },  // }
    Entry { code: 0x222, action: Action::Shifted(Keyboard::LeftBrace), },   // {
    Entry { code: 0x322, action: Action::OneShot(Mods::ALT.union(Mods::SHIFT)), },

    Entry { code: 0x026, action: Action::OneShot(Mods::SHIFT), },
    Entry { code: 0x126, action: Action::Shifted(Keyboard::Dot), },         // >
    Entry { code: 0x226, action: Action::Shifted(Keyboard::Comma), },       // <

    // The two extra keys that are simple enough to send.  Their remaining
    // layers are the volume, mute and brightness keys, which need a
    // consumer-control report.
    Entry { code: 0x046, action: Action::Simple(Keyboard::PrintScreen), },
    Entry { code: 0x0a8, action: Action::Simple(Keyboard::Insert), },
];

#[cfg(test)]
mod tests {
    use super::POSH_ACTIONS;
    use crate::layout::taipo::Action;

    /// Compare two actions, which do not implement `PartialEq`.
    fn same_action(a: &Action, b: &Action) -> bool {
        match (a, b) {
            (Action::Simple(x), Action::Simple(y)) => x == y,
            (Action::Shifted(x), Action::Shifted(y)) => x == y,
            (Action::OneShot(x), Action::OneShot(y)) => x == y,
            (Action::Release, Action::Release) => true,
            _ => false,
        }
    }

    fn action_for(code: u16) -> &'static Action {
        &POSH_ACTIONS
            .iter()
            .find(|e| e.code == code)
            .unwrap_or_else(|| panic!("no entry for {:#05x}", code))
            .action
    }

    /// The local `n+i+e` alias types exactly what the wiki's `l` chord does,
    /// on all four thumb layers.
    #[test]
    fn test_l_alias() {
        for layer in [0x000, 0x100, 0x200, 0x300] {
            let wiki = action_for(layer | 0x084);
            let alias = action_for(layer | 0x0c8);
            assert!(
                same_action(wiki, alias),
                "alias {:#05x} differs from {:#05x}",
                layer | 0x0c8,
                layer | 0x084,
            );
        }
    }

    /// Every chord code appears at most once; a duplicate would silently
    /// shadow the later entry.
    #[test]
    fn test_codes_unique() {
        let mut codes: Vec<u16> = POSH_ACTIONS.iter().map(|e| e.code).collect();
        codes.sort();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "duplicate code in POSH_ACTIONS");
    }

    /// Posh excludes the pinkies, so neither pinky bit may appear anywhere.
    #[test]
    fn test_no_pinky() {
        for entry in POSH_ACTIONS {
            assert_eq!(entry.code & 0x011, 0, "code {:#05x} uses a pinky key", entry.code);
        }
    }

    /// Every entry is reachable: an empty chord is never looked up.
    #[test]
    fn test_codes_nonempty() {
        for entry in POSH_ACTIONS {
            assert_ne!(entry.code, 0);
        }
    }
}
