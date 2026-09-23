//! Tests for the Orsy layout mode.
//!
//! These drive the full `LayoutManager`, switch it into Orsy, and check what
//! the `LayoutActions` calls type.  The chord rules themselves are tested in
//! `bbq-orsy`, exhaustively against the Python model; what is tested here is
//! the keyboard side: accumulation across both hands, the commands, and the
//! two escapes to Dosh, by chord and by the mesa3b's Fn keys.

#![cfg(feature = "orsy")]

use std::{cell::RefCell, collections::VecDeque};

use bbq_keyboard::layout::dosh::DOSH_ACTIONS;
use bbq_keyboard::layout::export::char_for_key;
use bbq_keyboard::layout::taipo::{Action, SCAN_MAP};
use bbq_keyboard::layout::{LayoutActions, LayoutManager, FN_LEFT, FN_RIGHT, MODE_KEY};
use bbq_keyboard::{KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode, Mods, Side};
use bbq_orsy::tables::{commands, Outer, Second, Vowel};
use futures::executor::block_on;

/// A chord from its four Series, so that the tests read as syllables.
fn syllable(s1: Outer, s2: Second, s3: Vowel, s4: Outer) -> (u16, u16) {
    (s1.bits() | s2.bits(), s3.bits() | s4.bits())
}

/// `ten`, closing the word.
const TEN: (Outer, Second, Vowel, Outer) = (Outer::FP, Second::Empty, Vowel::Ue, Outer::N);
/// `ten`, binding forward.
const TEN_: (Outer, Second, Vowel, Outer) = (Outer::FP, Second::Empty, Vowel::E, Outer::N);

fn ten() -> (u16, u16) {
    syllable(TEN.0, TEN.1, TEN.2, TEN.3)
}

fn ten_() -> (u16, u16) {
    syllable(TEN_.0, TEN_.1, TEN_.2, TEN_.3)
}

/// The proto3 scan code for a chord bit on a side, from `SCAN_MAP`.
fn scan_of(side: Side, bit: u16) -> u8 {
    (0..SCAN_MAP.len() as u8)
        .find(|code| SCAN_MAP[*code as usize] == Some((side, bit)))
        .unwrap_or_else(|| panic!("no key for {side:?} bit {bit:#x}"))
}

/// The Dosh chord for a plain key.
fn dosh_chord_for(key: Keyboard) -> u16 {
    DOSH_ACTIONS
        .iter()
        .find(|e| matches!(e.action, Action::Simple(k) if k == key))
        .unwrap_or_else(|| panic!("dosh has no {key:?}"))
        .code
}

/// The Dosh chord that types a character.
fn dosh_chord(ch: char) -> u16 {
    DOSH_ACTIONS
        .iter()
        .find(|e| match e.action {
            Action::Simple(k) => char_for_key(k, false) == Some(ch),
            Action::Shifted(k) => char_for_key(k, true) == Some(ch),
            _ => false,
        })
        .unwrap_or_else(|| panic!("dosh has no {ch:?}"))
        .code
}

#[derive(PartialEq, Eq, Debug)]
enum Actions {
    SetMode(LayoutMode),
    SendKey(KeyAction),
    SetSubMode(MinorMode),
    ClearSubMode(MinorMode),
}

#[derive(Default)]
struct Recorder {
    actions: RefCell<VecDeque<Actions>>,
}

impl LayoutActions for Recorder {
    async fn set_mode(&self, mode: LayoutMode) {
        self.actions.borrow_mut().push_back(Actions::SetMode(mode));
    }

    async fn set_mode_select(&self, _mode: LayoutMode) {}

    async fn send_key(&self, key: KeyAction) {
        self.actions.borrow_mut().push_back(Actions::SendKey(key));
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        self.actions.borrow_mut().push_back(Actions::SetSubMode(submode));
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        self.actions.borrow_mut().push_back(Actions::ClearSubMode(submode));
    }

    #[cfg(feature = "steno")]
    async fn send_raw_steno(&self, _stroke: bbq_steno::Stroke) {}
}

struct Tester {
    layout: LayoutManager,
    rec: Recorder,
}

impl Tester {
    /// A layout in Orsy mode, reached by cycling the mode key.
    fn new() -> Tester {
        let mut t = Tester {
            layout: LayoutManager::new(false),
            rec: Recorder::default(),
        };
        t.tick(1);
        let mut mode = match t.drain().pop() {
            Some(Actions::SetMode(mode)) => mode,
            other => panic!("no initial mode: {other:?}"),
        };
        while mode != LayoutMode::Orsy {
            t.event(KeyEvent::Press(MODE_KEY));
            t.event(KeyEvent::Release(MODE_KEY));
            mode = match t.drain().pop() {
                Some(Actions::SetMode(mode)) => mode,
                other => panic!("mode key did not change mode: {other:?}"),
            };
        }
        t
    }

    fn tick(&mut self, ms: usize) {
        for _ in 0..ms {
            block_on(self.layout.tick(&self.rec, 1));
        }
    }

    fn event(&mut self, event: KeyEvent) {
        block_on(self.layout.handle_event(event, &self.rec));
    }

    fn press(&mut self, side: Side, chord: u16) {
        for bit in 0..10 {
            if chord & (1 << bit) != 0 {
                self.event(KeyEvent::Press(scan_of(side, 1 << bit)));
            }
        }
    }

    fn release(&mut self, side: Side, chord: u16) {
        for bit in 0..10 {
            if chord & (1 << bit) != 0 {
                self.event(KeyEvent::Release(scan_of(side, 1 << bit)));
            }
        }
    }

    /// Strike a two-hand chord: press it all, release it all, and let a
    /// tick go by.
    fn stroke(&mut self, (left, right): (u16, u16)) {
        self.press(Side::Left, left);
        self.press(Side::Right, right);
        self.tick(1);
        self.release(Side::Left, left);
        self.release(Side::Right, right);
        self.tick(1);
    }

    /// Strike a two-hand chord with an Fn key in it, pressed and released
    /// among the other keys, as a chord is struck.
    fn fn_stroke(&mut self, key: u8, (left, right): (u16, u16)) {
        self.press(Side::Left, left);
        self.event(KeyEvent::Press(key));
        self.press(Side::Right, right);
        self.tick(1);
        self.release(Side::Left, left);
        self.event(KeyEvent::Release(key));
        self.release(Side::Right, right);
        self.tick(1);
    }

    /// Tap a key that is not a chord key, such as an Fn key, and let a tick
    /// go by.
    fn tap(&mut self, key: u8) {
        self.event(KeyEvent::Press(key));
        self.event(KeyEvent::Release(key));
        self.tick(1);
    }

    fn drain(&mut self) -> Vec<Actions> {
        self.rec.actions.borrow_mut().drain(..).collect()
    }

    /// What has been typed since the last call, with a backspace as `\u{8}`.
    /// Every key must be pressed and then released.
    fn typed(&mut self) -> String {
        let mut out = String::new();
        let mut down = false;
        for action in self.drain() {
            match action {
                Actions::SendKey(KeyAction::KeyPress(key, mods)) => {
                    assert!(!down, "key pressed while another is down");
                    down = true;
                    if key == Keyboard::DeleteBackspace {
                        out.push('\u{8}');
                    } else {
                        let shifted = mods.contains(Mods::SHIFT);
                        out.push(char_for_key(key, shifted).expect("printable"));
                    }
                }
                Actions::SendKey(KeyAction::KeyRelease) => {
                    assert!(down, "release with nothing down");
                    down = false;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(!down, "key left down");
        out
    }
}

/// A syllable is typed, and words get a space between them.
#[test]
fn test_types_words() {
    let mut t = Tester::new();
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
    t.stroke(ten_());
    assert_eq!(t.typed(), " ten");
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
}

/// The stroke goes on the first release, with everything that was held.
#[test]
fn test_first_up() {
    let mut t = Tester::new();
    let (left, right) = ten();
    t.press(Side::Left, left);
    t.press(Side::Right, right);
    t.tick(5);
    assert_eq!(t.typed(), "");
    // Releasing one key of the right hand sends the whole stroke.
    t.event(KeyEvent::Release(scan_of(Side::Right, 0x008)));
    assert_eq!(t.typed(), "ten");
    t.release(Side::Left, left);
    t.release(Side::Right, right & !0x008);
    t.tick(1);
    assert_eq!(t.typed(), "");
}

/// A press while releasing starts a new stroke from what is still held: a
/// held onset with the right hand re-struck.
#[test]
fn test_held_hand_restrikes() {
    let mut t = Tester::new();
    let (left, right) = ten();
    t.press(Side::Left, left);
    t.press(Side::Right, right);
    t.release(Side::Right, right);
    assert_eq!(t.typed(), "ten");
    t.press(Side::Right, right);
    t.release(Side::Right, right);
    assert_eq!(t.typed(), " ten");
    t.release(Side::Left, left);
    assert_eq!(t.typed(), "");
}

/// A chord that is not a syllable types nothing.
#[test]
fn test_unknown_chord() {
    let mut t = Tester::new();
    t.stroke((0x001, commands::CAP_NEXT | 0x001));
    assert_eq!(t.typed(), "");
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
}

/// The upper pinky is not an Orsy key, and a chord holding it types
/// nothing.
#[test]
fn test_upper_pinky_ignored() {
    let mut t = Tester::new();
    let (left, right) = ten();
    t.stroke((left | 0x010, right));
    assert_eq!(t.typed(), "ten");
}

/// Undo backspaces over the last stroke.
#[test]
fn test_undo() {
    let mut t = Tester::new();
    t.stroke(ten());
    t.stroke(ten());
    assert_eq!(t.typed(), "ten ten");
    t.stroke((commands::UNDO, 0));
    assert_eq!(t.typed(), "\u{8}\u{8}\u{8}\u{8}");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
}

/// The space and capitalise commands.
#[test]
fn test_space_and_cap() {
    let mut t = Tester::new();
    t.stroke((0, commands::SPACE));
    assert_eq!(t.typed(), " ");
    t.stroke((0, commands::CAP_NEXT));
    assert_eq!(t.typed(), "");
    t.stroke(ten());
    assert_eq!(t.typed(), "Ten");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
}

/// The spacing punctuation is native: it attaches to the word before it, closed or
/// not, gives the next word its space, and the sentence-enders capitalise.
#[test]
fn test_punctuation() {
    use bbq_orsy::tables::punctuation;
    let mut t = Tester::new();
    let mark = |text: &str| {
        punctuation::ALL.iter().find(|m| m.text == text).expect("a mark").bits
    };
    let full_stop = mark(".");
    let comma = mark(",");

    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
    t.stroke((0, full_stop));
    assert_eq!(t.typed(), ".");
    // The next word gets its space, and its capital.
    t.stroke(ten());
    assert_eq!(t.typed(), " Ten");

    // After a word left open, where an escaped mark used to let the next word run on.
    t.stroke(ten_());
    assert_eq!(t.typed(), " ten");
    t.stroke((0, comma));
    assert_eq!(t.typed(), ",");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");

    // And undo takes the mark back rather than stranding it.
    t.stroke((commands::UNDO, 0));
    assert_eq!(t.typed(), "\u{8}\u{8}\u{8}\u{8}");
    t.stroke((commands::UNDO, 0));
    assert_eq!(t.typed(), "\u{8}");

    // The apostrophe binds forward, so the coda that follows joins the same word.
    let mut t = Tester::new();
    t.stroke(ten_());
    assert_eq!(t.typed(), "ten");
    t.stroke((0, mark("'")));
    assert_eq!(t.typed(), "'");
    t.stroke((0, 0x020));
    assert_eq!(t.typed(), "s");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
}

/// The one-shot escape plays the right hand's chord through the Dosh table,
/// and the layout stays in Orsy.
#[test]
fn test_dosh_oneshot() {
    let mut t = Tester::new();
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
    t.stroke((commands::DOSH_ONESHOT, dosh_chord(',')));
    assert_eq!(t.typed(), ",");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
    // Sentence-ending punctuation capitalises the next word.
    t.stroke((commands::DOSH_ONESHOT, dosh_chord('.')));
    assert_eq!(t.typed(), ".");
    t.stroke(ten());
    assert_eq!(t.typed(), " Ten");
}

/// The escaped chord is typed by the release that finishes the stroke, with
/// no tick after it.
#[test]
fn test_dosh_oneshot_without_tick() {
    let mut t = Tester::new();
    let (left, right) = (commands::DOSH_ONESHOT, dosh_chord(','));
    t.press(Side::Left, left);
    t.press(Side::Right, right);
    t.release(Side::Left, left);
    t.release(Side::Right, right);
    assert_eq!(t.typed(), ",");
}

/// A backspace played through the escape is taken off the output stage's
/// record, so an undo afterwards takes back only what is still there, and
/// an erased space is owed again.
#[test]
fn test_dosh_backspace_keeps_up() {
    let mut t = Tester::new();
    t.stroke(ten());
    t.stroke(ten());
    assert_eq!(t.typed(), "ten ten");
    let backspace = dosh_chord_for(Keyboard::DeleteBackspace);
    t.stroke((commands::DOSH_ONESHOT, backspace));
    assert_eq!(t.typed(), "\u{8}");
    t.stroke((commands::UNDO, 0));
    assert_eq!(t.typed(), "\u{8}\u{8}\u{8}");
    // Back to "ten", and the next word gets its space.
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
    // Erase the whole of " ten" by hand: the space is owed again.
    for _ in 0..4 {
        t.stroke((commands::DOSH_ONESHOT, backspace));
    }
    assert_eq!(t.typed(), "\u{8}\u{8}\u{8}\u{8}");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
}

/// The toggle switches to Dosh, and the same chord there switches back.
#[test]
fn test_dosh_toggle() {
    let mut t = Tester::new();
    t.stroke((commands::DOSH_TOGGLE, 0));
    // The engine comes up in Dosh, so only the mode is reported.
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
    // Dosh types.
    t.press(Side::Left, 0x008);
    t.release(Side::Left, 0x008);
    t.tick(1);
    assert_eq!(t.typed(), "e");
    // And back.  This is a taipo chord now, so it needs the chord window
    // or a full release to commit; a tap does.
    t.press(Side::Right, commands::DOSH_TOGGLE);
    t.release(Side::Right, commands::DOSH_TOGGLE);
    t.tick(1);
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Orsy)]);
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
}

/// The Orsy chord in Dosh switches as soon as it is released, with no tick
/// after it.
#[test]
fn test_dosh_toggle_without_tick() {
    let mut t = Tester::new();
    t.stroke((commands::DOSH_TOGGLE, 0));
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
    t.press(Side::Right, commands::DOSH_TOGGLE);
    t.release(Side::Right, commands::DOSH_TOGGLE);
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Orsy)]);
}

/// Toggling from the taipo table lands in Dosh, and says so.
#[test]
fn test_toggle_selects_dosh() {
    let mut t = Tester::new();
    t.stroke((commands::DOSH_TOGGLE, 0));
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
    // Select the taipo table with its chord, the whole top row.
    t.press(Side::Left, 0x0f0);
    t.release(Side::Left, 0x0f0);
    t.tick(1);
    assert_eq!(t.drain(), vec![Actions::ClearSubMode(MinorMode::Dosh)]);
    // In taipo, the toggle chord means nothing, so go round by the mode key.
    let mut t2 = Tester { layout: t.layout, rec: t.rec };
    loop {
        t2.event(KeyEvent::Press(MODE_KEY));
        t2.event(KeyEvent::Release(MODE_KEY));
        if t2.drain().pop() == Some(Actions::SetMode(LayoutMode::Orsy)) {
            break;
        }
    }
    t2.stroke((commands::DOSH_TOGGLE, 0));
    assert_eq!(
        t2.drain(),
        vec![
            Actions::SetMode(LayoutMode::Taipo),
            Actions::SetSubMode(MinorMode::Dosh),
        ]
    );
}

/// Left Fn struck in a chord with right-hand keys plays those keys through
/// Dosh, and the layout stays in Orsy, just as the one-shot chord does.
#[test]
fn test_fn_left_oneshot() {
    let mut t = Tester::new();
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
    t.fn_stroke(FN_LEFT, (0, dosh_chord(',')));
    assert_eq!(t.typed(), ",");
    t.stroke(ten());
    assert_eq!(t.typed(), " ten");
}

/// Right Fn does the same for the left hand, through the same table.
#[test]
fn test_fn_right_oneshot() {
    let mut t = Tester::new();
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
    t.fn_stroke(FN_RIGHT, (dosh_chord('.'), 0));
    assert_eq!(t.typed(), ".");
    // And it counts as sentence-ending punctuation.
    t.stroke(ten());
    assert_eq!(t.typed(), " Ten");
}

/// Fn is one of the chord's keys, so where it falls in the chord does not
/// matter: pressed first or last, and released first or last.
#[test]
fn test_fn_order_in_chord() {
    let mut t = Tester::new();
    let comma = dosh_chord(',');

    // Fn first down and first up: the chord commits on Fn's release.
    t.event(KeyEvent::Press(FN_LEFT));
    t.press(Side::Right, comma);
    t.tick(1);
    t.event(KeyEvent::Release(FN_LEFT));
    t.release(Side::Right, comma);
    t.tick(1);
    assert_eq!(t.typed(), ",");

    // Fn last down and last up.
    t.press(Side::Right, comma);
    t.event(KeyEvent::Press(FN_LEFT));
    t.tick(1);
    t.release(Side::Right, comma);
    t.event(KeyEvent::Release(FN_LEFT));
    t.tick(1);
    assert_eq!(t.typed(), ",");
}

/// Fn in a chord with keys on its own hand, or on both, or with the other
/// Fn, is dead.
#[test]
fn test_fn_dead() {
    let mut t = Tester::new();
    t.fn_stroke(FN_LEFT, (dosh_chord(','), 0));
    t.fn_stroke(FN_LEFT, ten());
    t.fn_stroke(FN_RIGHT, ten());
    t.event(KeyEvent::Press(FN_LEFT));
    t.event(KeyEvent::Press(FN_RIGHT));
    t.event(KeyEvent::Release(FN_LEFT));
    t.event(KeyEvent::Release(FN_RIGHT));
    t.tick(1);
    assert_eq!(t.typed(), "");
    // And the layout is still in Orsy.
    t.stroke(ten());
    assert_eq!(t.typed(), "ten");
}

/// A solo tap of either Fn key toggles between Orsy and Dosh, both ways.
#[test]
fn test_fn_toggle() {
    for key in [FN_LEFT, FN_RIGHT] {
        let mut t = Tester::new();
        t.tap(key);
        assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
        // Dosh types.
        t.press(Side::Left, 0x008);
        t.release(Side::Left, 0x008);
        t.tick(1);
        assert_eq!(t.typed(), "e");
        // And back.
        t.tap(key);
        assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Orsy)]);
        t.stroke(ten());
        assert_eq!(t.typed(), "ten");
    }
}

/// In Dosh, Fn struck in a chord is ignored: the chord types as it would
/// without it, and the layout does not switch to Orsy.
#[test]
fn test_fn_in_dosh_chord() {
    let mut t = Tester::new();
    t.tap(FN_LEFT);
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
    t.fn_stroke(FN_LEFT, (0, 0x008));
    assert_eq!(t.typed(), "e");
}

/// Toggling from the taipo table with Fn lands in Dosh, and says so, like
/// the chord form of the toggle.
#[test]
fn test_fn_toggle_selects_dosh() {
    let mut t = Tester::new();
    t.tap(FN_LEFT);
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Taipo)]);
    // Select the taipo table with its chord, the whole top row.
    t.press(Side::Left, 0x0f0);
    t.release(Side::Left, 0x0f0);
    t.tick(1);
    assert_eq!(t.drain(), vec![Actions::ClearSubMode(MinorMode::Dosh)]);
    // Fn still reaches Orsy from the taipo table, where the toggle chord does not.
    t.tap(FN_LEFT);
    assert_eq!(t.drain(), vec![Actions::SetMode(LayoutMode::Orsy)]);
    t.tap(FN_LEFT);
    assert_eq!(
        t.drain(),
        vec![
            Actions::SetMode(LayoutMode::Taipo),
            Actions::SetSubMode(MinorMode::Dosh),
        ]
    );
}
