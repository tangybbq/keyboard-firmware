//! The Dosh chord table.
//!
//! Dosh is a local layout, derived from Posh
//! (<https://inkeys.wiki/en/keymaps/posh>), a Taipo-style chording layout from
//! the same community, described there as "a taipo style layout that excludes
//! the pinkies in order to make combos more accurate and long periods of work
//! more comfortable".  Like Taipo, each half of the keyboard is a complete
//! layout, and the two halves are freely alternated.
//!
//! Where Posh leaves both pinky keys out, Dosh keeps the *lower* one, so it
//! has 7 finger keys per hand plus the 2 thumbs.  What the pinky buys is not
//! the extra chords but the agreement with Taipo: every letter Taipo types
//! without its upper pinky can sit on exactly the chord Taipo has it on, so
//! nineteen of the twenty-six letters are the same in both layouts and only
//! seven have to be learned twice.
//!
//! Only the table differs from Taipo: the chord accumulation, the chord timing,
//! the modifier handling, and the scan code mapping are all shared, so the same
//! chord code bits are used.  Each key now types the letter its bit is named
//! after in [`crate::layout::export::BIT_NAMES`], so a Dosh chord can be
//! spelled in the same alphabet as a Taipo one:
//!
//! | bit     | finger | row    | Dosh letter typed alone |
//! |---------|--------|--------|-------------------------|
//! | `0x001` | pinky  | bottom | `a`                     |
//! | `0x002` | ring   | bottom | `o`                     |
//! | `0x004` | middle | bottom | `t`                     |
//! | `0x008` | index  | bottom | `e`                     |
//! | `0x010` | pinky  | top    | (unused)                |
//! | `0x020` | ring   | top    | `s`                     |
//! | `0x040` | middle | top    | `n`                     |
//! | `0x080` | index  | top    | `i`                     |
//! | `0x100` | thumb  |        | Space                   |
//! | `0x200` | thumb  |        | Backspace               |
//!
//! The upper pinky bit (`0x010`, Taipo's `r`) appears in no entry here but the
//! `rsni` variant selection chord.
//!
//! # Where the letters came from
//!
//! Six letters — `a`, `l`, `q`, `d`, `j`, `w` — are the lower pinky paired
//! with each of the other keys, and are Taipo's chords exactly.  Moving them
//! there freed the chords Posh had them on, which is what let `s`, `y`, `p`
//! and `k` move onto Taipo's chords in turn.
//!
//! Each letter took its whole column of thumb layers with it, so a digit or a
//! symbol is still found on the letter it was learned on: `1` is still on `d`
//! and `2` on `l`, even though both letters moved.  The navigation cluster
//! keeps its shape as well — up and down are still the middle finger's two
//! keys, left and right the index and ring of the bottom row — with Escape the
//! one key that moved, from the ring top to the lower pinky, following `a`.
//!
//! Seven chords are left with nothing on them by the move: `s+n+i`, `t+i`,
//! `o+t+i`, `o+n+i`, `e+n+i`, `o+e+s` and `o+e+n`.  They are deliberately
//! unmapped rather than filled.
//!
//! Seven letters still differ from Taipo.  Six of them — `b`, `g`, `m`, `r`,
//! `x`, `z` — are on Taipo's upper pinky and so out of reach.  The seventh,
//! `v`, is only blocked because Dosh has `m` on the chord Taipo types `v`
//! with; it sits on `e+s+n` instead, one of the chords the move freed.  See
//! DOSH.md.
//!
//! # The modifiers
//!
//! Also Taipo's: the same-finger vertical pairs, with the index pair Shift,
//! the middle pair Control and the ring pair Alt.  Taipo's GUI is the pinky
//! pair, which Dosh cannot reach, so it is the Alt chord plus the lower pinky.
//!
//! The brackets stay on the chords they were learned on rather than following
//! the modifier that moved, so the mnemonic is still positional: `()` on the
//! middle finger, `[]` on the index, `{}` on the ring.  `<>` keep the chord
//! that used to be the wiki's `ralt`, whose base is now unmapped.
//!
//! Differences from the Posh wiki, beyond the letters that moved:
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
//! - **The modifiers are Taipo's**, as above.  The wiki's `ralt` chord
//!   (`0x026`) has no modifier at all: [`Mods`] has no right-alt, and Shift is
//!   on Taipo's chord.  Shift's own "both" variant would be shift plus shift,
//!   so `0x388` is left unmapped.
//! - **`s` and `h` have their punctuation swapped.**  The wiki puts the comma
//!   and apostrophe on the chord that types `s` and the period and quote on
//!   the one that types `h`; the developer wants them the other way around.
//! - **`o` and `e` have their navigation swapped.**  The wiki puts left and
//!   home on `o` and right and end on `e`, matching the order the keys sit on
//!   the left hand; the developer finds the mirrored sense more intuitive, so
//!   `o` is right and end, and `e` is left and home.
//!
//! Left unmapped, as a future task: everything needing a consumer-control HID
//! report, which the firmware does not have — play/pause, next/previous track,
//! stop, volume up/down/mute, and brightness up/down.  The wiki's "layer 0-3"
//! chord is a QMK concept that does not apply here, and the five empty rows at
//! the bottom of the wiki's table have nothing to map.

use usbd_human_interface_device::page::Keyboard;

use crate::Mods;

use super::taipo::{Action, Entry, TaipoVariant};

/// The mapping between each Dosh chord and its action.
pub static DOSH_ACTIONS: &[Entry] = &[
    // The thumb keys by themselves, and together as the null key, as in Taipo.
    Entry { code: 0x100, action: Action::Simple(Keyboard::Space), },
    Entry { code: 0x200, action: Action::Simple(Keyboard::DeleteBackspace), },
    Entry { code: 0x300, action: Action::Release, },

    // The seven single keys: alone, +Sp (capital), +Bk (navigation), +both.
    // These are Taipo's seven, less `r` on the upper pinky, which Dosh does
    // not use; each key types the letter its bit is named after.
    Entry { code: 0x001, action: Action::Simple(Keyboard::A), },
    Entry { code: 0x101, action: Action::Shifted(Keyboard::A), },
    Entry { code: 0x201, action: Action::Simple(Keyboard::Escape), },
    Entry { code: 0x301, action: Action::Simple(Keyboard::DeleteForward), },

    Entry { code: 0x002, action: Action::Simple(Keyboard::O), },
    Entry { code: 0x102, action: Action::Shifted(Keyboard::O), },
    Entry { code: 0x202, action: Action::Simple(Keyboard::RightArrow), },
    Entry { code: 0x302, action: Action::Simple(Keyboard::End), },

    Entry { code: 0x004, action: Action::Simple(Keyboard::T), },
    Entry { code: 0x104, action: Action::Shifted(Keyboard::T), },
    Entry { code: 0x204, action: Action::Simple(Keyboard::DownArrow), },
    Entry { code: 0x304, action: Action::Simple(Keyboard::PageDown), },

    Entry { code: 0x008, action: Action::Simple(Keyboard::E), },
    Entry { code: 0x108, action: Action::Shifted(Keyboard::E), },
    Entry { code: 0x208, action: Action::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x308, action: Action::Simple(Keyboard::Home), },

    Entry { code: 0x020, action: Action::Simple(Keyboard::S), },
    Entry { code: 0x120, action: Action::Shifted(Keyboard::S), },
    Entry { code: 0x220, action: Action::Simple(Keyboard::Dot), },
    Entry { code: 0x320, action: Action::Shifted(Keyboard::Apostrophe), },

    Entry { code: 0x040, action: Action::Simple(Keyboard::N), },
    Entry { code: 0x140, action: Action::Shifted(Keyboard::N), },
    Entry { code: 0x240, action: Action::Simple(Keyboard::UpArrow), },
    Entry { code: 0x340, action: Action::Simple(Keyboard::PageUp), },

    Entry { code: 0x080, action: Action::Simple(Keyboard::I), },
    Entry { code: 0x180, action: Action::Shifted(Keyboard::I), },
    Entry { code: 0x280, action: Action::Simple(Keyboard::ReturnEnter), },
    Entry { code: 0x380, action: Action::Simple(Keyboard::Tab), },

    // `h`, the one same-row pair left carrying punctuation now that `s` is a
    // single key.
    Entry { code: 0x00c, action: Action::Simple(Keyboard::H), },
    Entry { code: 0x10c, action: Action::Shifted(Keyboard::H), },
    Entry { code: 0x20c, action: Action::Simple(Keyboard::Comma), },
    Entry { code: 0x30c, action: Action::Simple(Keyboard::Apostrophe), },

    // The letters whose +Bk is a digit, and whose +both is the matching
    // function key.
    Entry { code: 0x048, action: Action::Simple(Keyboard::R), },
    Entry { code: 0x148, action: Action::Shifted(Keyboard::R), },
    Entry { code: 0x248, action: Action::Simple(Keyboard::Keyboard0), },
    Entry { code: 0x348, action: Action::Simple(Keyboard::F10), },

    Entry { code: 0x009, action: Action::Simple(Keyboard::D), },
    Entry { code: 0x109, action: Action::Shifted(Keyboard::D), },
    Entry { code: 0x209, action: Action::Simple(Keyboard::Keyboard1), },
    Entry { code: 0x309, action: Action::Simple(Keyboard::F1), },

    Entry { code: 0x003, action: Action::Simple(Keyboard::L), },
    Entry { code: 0x103, action: Action::Shifted(Keyboard::L), },
    Entry { code: 0x203, action: Action::Simple(Keyboard::Keyboard2), },
    Entry { code: 0x303, action: Action::Simple(Keyboard::F2), },

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

    Entry { code: 0x081, action: Action::Simple(Keyboard::W), },
    Entry { code: 0x181, action: Action::Shifted(Keyboard::W), },
    Entry { code: 0x281, action: Action::Simple(Keyboard::Keyboard6), },
    Entry { code: 0x381, action: Action::Simple(Keyboard::F6), },

    Entry { code: 0x0a0, action: Action::Simple(Keyboard::F), },
    Entry { code: 0x1a0, action: Action::Shifted(Keyboard::F), },
    Entry { code: 0x2a0, action: Action::Simple(Keyboard::Keyboard7), },
    Entry { code: 0x3a0, action: Action::Simple(Keyboard::F7), },

    Entry { code: 0x042, action: Action::Simple(Keyboard::G), },
    Entry { code: 0x142, action: Action::Shifted(Keyboard::G), },
    Entry { code: 0x242, action: Action::Simple(Keyboard::Keyboard8), },
    Entry { code: 0x342, action: Action::Simple(Keyboard::F8), },

    Entry { code: 0x0c0, action: Action::Simple(Keyboard::Y), },
    Entry { code: 0x1c0, action: Action::Shifted(Keyboard::Y), },
    Entry { code: 0x2c0, action: Action::Simple(Keyboard::Keyboard9), },
    Entry { code: 0x3c0, action: Action::Simple(Keyboard::F9), },

    // The letters whose +Bk and +both are symbols.
    Entry { code: 0x060, action: Action::Simple(Keyboard::P), },
    Entry { code: 0x160, action: Action::Shifted(Keyboard::P), },
    Entry { code: 0x260, action: Action::Shifted(Keyboard::Equal), },      // +
    Entry { code: 0x360, action: Action::Simple(Keyboard::Equal), },       // =

    Entry { code: 0x00e, action: Action::Simple(Keyboard::B), },
    Entry { code: 0x10e, action: Action::Shifted(Keyboard::B), },
    Entry { code: 0x20e, action: Action::Simple(Keyboard::Minus), },       // -
    Entry { code: 0x30e, action: Action::Shifted(Keyboard::Minus), },      // _

    Entry { code: 0x068, action: Action::Simple(Keyboard::V), },
    Entry { code: 0x168, action: Action::Shifted(Keyboard::V), },
    Entry { code: 0x268, action: Action::Simple(Keyboard::ForwardSlash), },// /
    Entry { code: 0x368, action: Action::Simple(Keyboard::Backslash), },   // \

    Entry { code: 0x082, action: Action::Simple(Keyboard::K), },
    Entry { code: 0x182, action: Action::Shifted(Keyboard::K), },
    Entry { code: 0x282, action: Action::Simple(Keyboard::Semicolon), },   // ;
    Entry { code: 0x382, action: Action::Shifted(Keyboard::Backslash), },  // |

    Entry { code: 0x041, action: Action::Simple(Keyboard::J), },
    Entry { code: 0x141, action: Action::Shifted(Keyboard::J), },
    Entry { code: 0x241, action: Action::Shifted(Keyboard::Semicolon), },  // :
    Entry { code: 0x341, action: Action::Shifted(Keyboard::Keyboard8), },  // *

    Entry { code: 0x02c, action: Action::Simple(Keyboard::X), },
    Entry { code: 0x12c, action: Action::Shifted(Keyboard::X), },
    Entry { code: 0x22c, action: Action::Shifted(Keyboard::Keyboard4), },  // $
    Entry { code: 0x32c, action: Action::Shifted(Keyboard::Keyboard3), },  // #

    Entry { code: 0x005, action: Action::Simple(Keyboard::Q), },
    Entry { code: 0x105, action: Action::Shifted(Keyboard::Q), },
    Entry { code: 0x205, action: Action::Shifted(Keyboard::Keyboard2), },  // @
    Entry { code: 0x305, action: Action::Simple(Keyboard::F11), },

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

    // The modifiers, which are Taipo's: the same-finger vertical pairs, with
    // the index pair Shift, the middle pair Control and the ring pair Alt.
    // Taipo's GUI is the pinky pair, which needs the upper pinky, so here it
    // is the Alt chord plus the lower pinky instead.
    //
    // Their layers are the brackets, which stay on the chords they were
    // learned on rather than following the modifier that moved.  Shift has no
    // both-thumbs variant, as it would be shift plus shift.
    Entry { code: 0x088, action: Action::OneShot(Mods::SHIFT), },
    Entry { code: 0x188, action: Action::Simple(Keyboard::RightBrace), },   // ]
    Entry { code: 0x288, action: Action::Simple(Keyboard::LeftBrace), },    // [

    Entry { code: 0x044, action: Action::OneShot(Mods::CONTROL), },
    Entry { code: 0x144, action: Action::Shifted(Keyboard::Keyboard0), },   // )
    Entry { code: 0x244, action: Action::Shifted(Keyboard::Keyboard9), },   // (
    Entry { code: 0x344, action: Action::OneShot(Mods::CONTROL.union(Mods::SHIFT)), },

    Entry { code: 0x022, action: Action::OneShot(Mods::ALT), },
    Entry { code: 0x122, action: Action::Shifted(Keyboard::RightBrace), },  // }
    Entry { code: 0x222, action: Action::Shifted(Keyboard::LeftBrace), },   // {
    Entry { code: 0x322, action: Action::OneShot(Mods::ALT.union(Mods::SHIFT)), },

    Entry { code: 0x023, action: Action::OneShot(Mods::GUI), },
    Entry { code: 0x323, action: Action::OneShot(Mods::GUI.union(Mods::SHIFT)), },

    // The angle brackets, on the chord that used to be the wiki's `ralt`.
    // Its base is unmapped now that Shift is on Taipo's chord.
    Entry { code: 0x126, action: Action::Shifted(Keyboard::Dot), },         // >
    Entry { code: 0x226, action: Action::Shifted(Keyboard::Comma), },       // <

    // The two extra keys that are simple enough to send.  Their remaining
    // layers are the volume, mute and brightness keys, which need a
    // consumer-control report.
    Entry { code: 0x046, action: Action::Simple(Keyboard::PrintScreen), },
    Entry { code: 0x0a8, action: Action::Simple(Keyboard::Insert), },

    // Selecting the chord table, the same pair as in `TAIPO_ACTIONS`: `rsni`
    // (the whole top row) selects Taipo, `aote` (the whole bottom row) selects
    // Dosh.
    //
    // `rsni` uses the upper pinky, which Dosh never touches, so it cannot
    // collide with anything here.  `aote` now uses a key Dosh does type with,
    // so its safety is only that nothing else claims that chord; `a+o`, `a+t`
    // and `a+e` are `l`, `q` and `d`, but all four together spell nothing.
    Entry { code: 0x0f0, action: Action::Variant(TaipoVariant::Taipo), },
    Entry { code: 0x00f, action: Action::Variant(TaipoVariant::Dosh), },
];

#[cfg(test)]
mod tests {
    use super::DOSH_ACTIONS;
    use crate::layout::taipo::{Action, TAIPO_ACTIONS};
    use crate::Mods;

    fn action_for(code: u16) -> &'static Action {
        &DOSH_ACTIONS
            .iter()
            .find(|e| e.code == code)
            .unwrap_or_else(|| panic!("no entry for {:#05x}", code))
            .action
    }

    /// Every chord code appears at most once; a duplicate would silently
    /// shadow the later entry.
    #[test]
    fn test_codes_unique() {
        let mut codes: Vec<u16> = DOSH_ACTIONS.iter().map(|e| e.code).collect();
        codes.sort();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "duplicate code in DOSH_ACTIONS");
    }

    /// Dosh uses the lower pinky key but not the upper one, so the upper pinky
    /// bit may not appear in anything Dosh types.
    ///
    /// The variant selection chords are the deliberate exception: `rsni` is a
    /// shared shape rather than a Dosh chord, and the upper pinky is exactly
    /// what keeps it from ever colliding with one.
    #[test]
    fn test_no_upper_pinky() {
        for entry in DOSH_ACTIONS {
            if matches!(entry.action, Action::Variant(_)) {
                continue;
            }
            assert_eq!(
                entry.code & 0x010,
                0,
                "code {:#05x} uses the upper pinky key",
                entry.code
            );
        }
    }

    /// Both variant selection chords are present.
    #[test]
    fn test_variant_chords() {
        use crate::layout::taipo::TaipoVariant;

        for (code, want) in [(0x0f0u16, TaipoVariant::Taipo), (0x00f, TaipoVariant::Dosh)] {
            match action_for(code) {
                Action::Variant(v) => assert_eq!(*v, want),
                _ => panic!("code {code:#05x} is not a variant selection"),
            }
        }
    }

    /// Every entry is reachable: an empty chord is never looked up.
    #[test]
    fn test_codes_nonempty() {
        for entry in DOSH_ACTIONS {
            assert_ne!(entry.code, 0);
        }
    }

    /// The chord a table types a given letter with, if it types it at all.
    fn letter_chord(table: &'static [super::Entry], letter: char) -> Option<u16> {
        let want = crate::usb_typer::key_for_char(letter).expect("letter has a key").0;
        table
            .iter()
            .find(|e| matches!(e.action, Action::Simple(k) if k == want))
            .map(|e| e.code)
    }

    /// The letters Dosh and Taipo agree on: nineteen of the twenty-six.
    ///
    /// This is the point of the layout, so it is pinned: a letter dropping out
    /// of this list is a change in what has to be learned twice.
    #[test]
    fn test_shared_letters_match_taipo() {
        for letter in "acdefhijklnopqstuwy".chars() {
            assert_eq!(
                letter_chord(DOSH_ACTIONS, letter),
                letter_chord(TAIPO_ACTIONS, letter),
                "{letter} is not on Taipo's chord",
            );
        }
    }

    /// The chord a table sends a given modifier with.
    fn mod_chord(table: &'static [super::Entry], want: Mods) -> Option<u16> {
        table
            .iter()
            .find(|e| matches!(e.action, Action::OneShot(m) if m == want))
            .map(|e| e.code)
    }

    /// Shift, Control and Alt are on Taipo's chords.  GUI cannot be: Taipo
    /// has it on the pinky pair, and Dosh has no upper pinky, so it is the Alt
    /// chord plus the lower pinky.
    #[test]
    fn test_modifiers_match_taipo() {
        for m in [Mods::SHIFT, Mods::CONTROL, Mods::ALT] {
            assert_eq!(
                mod_chord(DOSH_ACTIONS, m),
                mod_chord(TAIPO_ACTIONS, m),
                "{m:?} is not on Taipo's chord",
            );
        }

        assert_eq!(mod_chord(TAIPO_ACTIONS, Mods::GUI), Some(0x011));
        assert_eq!(
            mod_chord(DOSH_ACTIONS, Mods::GUI),
            Some(mod_chord(DOSH_ACTIONS, Mods::ALT).unwrap() | 0x001),
        );
    }

    /// And the seven they do not agree on, for the same reason.  Six of them
    /// are on Taipo's upper pinky and so are out of reach; `v` is only blocked
    /// because Dosh has `m` on Taipo's `v` chord.  See DOSH.md.
    #[test]
    fn test_differing_letters() {
        for letter in "bgmrvxz".chars() {
            assert_ne!(
                letter_chord(DOSH_ACTIONS, letter),
                letter_chord(TAIPO_ACTIONS, letter),
                "{letter} now matches Taipo; move it to the shared list",
            );
        }
    }
}
