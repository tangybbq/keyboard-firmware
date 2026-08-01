//! Tests for Taipo
//!
//! Taipo is a fairly unique keyboard layout that has some complexity behind it.
//! These tests drive the full `LayoutManager`, switching into taipo mode, and
//! then check the `LayoutActions` calls that result from scripted key events.
//!
//! Tests are written with the [`Script`] builder, which describes a test as a
//! sequence of steps: key events (in terms of taipo chords, not scan codes),
//! time passing, and expected actions.  Expectations are positional: each one
//! pops the next action the layout produced, so ordering is checked as well.

#![allow(dead_code)]

use std::{cell::RefCell, collections::VecDeque};

use bbq_keyboard::{
    layout::{LayoutActions, LayoutManager},
    KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode, Mods, Side,
};
use bbq_steno::Stroke;
use futures::executor::block_on;

//////////////////////////////////////////////////////////////////////////////
// Taipo keys
//
// A taipo chord is a bitmask of the 10 keys on one side of the keyboard.  The
// names here are the character each key types by itself (see TAIPO.md); the two
// thumb keys are `SP` (space) and `BK` (backspace).
//////////////////////////////////////////////////////////////////////////////

/// Bottom row, index to pinky.
const A: u16 = 0x001;
const O: u16 = 0x002;
const T: u16 = 0x004;
const E: u16 = 0x008;
/// Top row, index to pinky.
const R: u16 = 0x010;
const S: u16 = 0x020;
const N: u16 = 0x040;
const I: u16 = 0x080;
/// The thumb keys.
const SP: u16 = 0x100;
const BK: u16 = 0x200;

/// Shorthands for the two sides.
const LEFT: Side = Side::Left;
const RIGHT: Side = Side::Right;

/// The proto3 scan code for each taipo key, indexed by side and then by the bit
/// number within the chord (0 is `A`, 9 is `BK`).  This is the inverse of the
/// `SCAN_MAP` table in `layout/taipo.rs`.
static SCANS: [[u8; 10]; 2] = [
    // A   O   T   E   R  S   N   I   SP  BK
    [5, 9, 13, 17, 4, 8, 12, 16, 19, 23],
    [29, 33, 37, 41, 28, 32, 36, 40, 43, 47],
];

/// The mode selection key.
const MODE_KEY: u8 = 2;

/// One of the "taipo shift" keys, which allows taipo to be typed while in steno
/// mode.
const TAIPO_KEY: u8 = 20;

/// The scan codes of the keys making up a chord, in bit order.
fn scans(side: Side, chord: u16) -> impl Iterator<Item = u8> {
    let row = SCANS[side.index()];
    (0..10).filter_map(move |bit| {
        if chord & (1 << bit) != 0 {
            Some(row[bit])
        } else {
            None
        }
    })
}

/// How long taipo waits before deciding a chord is complete.
const CHORD_TIME: usize = 50;

//////////////////////////////////////////////////////////////////////////////
// The test harness
//////////////////////////////////////////////////////////////////////////////

/// Actions. This is essentially an encoding of LayoutActions.
#[derive(PartialEq, Eq, Debug)]
enum Actions {
    ClearSubMode(MinorMode),
    SendKey(KeyAction),
    SendRawSteno(Stroke),
    SetMode(LayoutMode),
    SetModeSelect(LayoutMode),
    SetSubMode(MinorMode),
}

/// Our Actor steps are each one of these.
enum ActorStep {
    /// Cause this much time to pass for the layout engine (in ms).
    Tick(usize),
    /// Send a key event.
    Event(KeyEvent),
    /// Expect a particular action.
    Action(Actions),
    /// Expect that nothing is pending at this point.
    Idle,
}

/// Keep track of the state of the test, as well as the state we think the
/// keyboard should be in.
struct TestActor {
    /// Actions that have been queued up.
    actions: RefCell<VecDeque<Actions>>,
}

impl TestActor {
    fn new() -> Self {
        Self {
            actions: RefCell::new(VecDeque::new()),
        }
    }
}

impl LayoutActions for TestActor {
    async fn set_mode(&self, mode: LayoutMode) {
        self.actions.borrow_mut().push_back(Actions::SetMode(mode));
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        self.actions
            .borrow_mut()
            .push_back(Actions::SetModeSelect(mode));
    }

    async fn send_key(&self, key: KeyAction) {
        self.actions.borrow_mut().push_back(Actions::SendKey(key));
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        self.actions
            .borrow_mut()
            .push_back(Actions::SetSubMode(submode));
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        self.actions
            .borrow_mut()
            .push_back(Actions::ClearSubMode(submode));
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        self.actions
            .borrow_mut()
            .push_back(Actions::SendRawSteno(stroke));
    }
}

/// A scripted test.
///
/// The builder methods all return `&mut Self` so that a test can be written as
/// a chain of stimulus and expectation.
struct Script {
    steps: Vec<ActorStep>,
}

impl Script {
    /// A new script, starting in the layout's initial (qwerty) mode.  The
    /// layout announces its mode on the first tick.
    fn new() -> Script {
        let mut script = Script { steps: Vec::new() };
        script.tick(1).mode(LayoutMode::Qwerty);
        script
    }

    /// A new script, already switched into taipo mode.
    fn taipo() -> Script {
        let mut script = Script::new();
        script.to_mode(LayoutMode::Steno).to_mode(LayoutMode::Taipo);
        script
    }

    /// A new script, already switched into steno mode.
    fn steno() -> Script {
        let mut script = Script::new();
        script.to_mode(LayoutMode::Steno);
        script
    }

    //////////////////////////////////////////////////////////////////////////
    // Stimulus
    //////////////////////////////////////////////////////////////////////////

    /// Let this many milliseconds pass.
    fn tick(&mut self, ms: usize) -> &mut Self {
        self.steps.push(ActorStep::Tick(ms));
        self
    }

    /// Press a raw scan code.
    fn press_scan(&mut self, scan: u8) -> &mut Self {
        self.steps.push(ActorStep::Event(KeyEvent::Press(scan)));
        self
    }

    /// Release a raw scan code.
    fn release_scan(&mut self, scan: u8) -> &mut Self {
        self.steps.push(ActorStep::Event(KeyEvent::Release(scan)));
        self
    }

    /// Press all of the keys of a chord, with no time in between.
    fn press(&mut self, side: Side, chord: u16) -> &mut Self {
        for scan in scans(side, chord) {
            self.press_scan(scan);
        }
        self
    }

    /// Release all of the keys of a chord, with no time in between.
    fn release(&mut self, side: Side, chord: u16) -> &mut Self {
        for scan in scans(side, chord) {
            self.release_scan(scan);
        }
        self
    }

    /// Type a chord, holding it long enough for the chord timer to expire.  The
    /// resulting events have all been processed when this returns.
    fn chord(&mut self, side: Side, chord: u16) -> &mut Self {
        self.press(side, chord)
            .tick(CHORD_TIME)
            .release(side, chord)
            .tick(1)
    }

    /// Tap a chord, releasing it well before the chord timer expires.
    fn tap(&mut self, side: Side, chord: u16) -> &mut Self {
        self.press(side, chord)
            .tick(5)
            .release(side, chord)
            .tick(1)
    }

    /// Switch to the given mode by tapping the mode key.  Only the modes in the
    /// normal cycle can be reached this way.
    fn to_mode(&mut self, mode: LayoutMode) -> &mut Self {
        self.press_scan(MODE_KEY)
            .mode_select(mode)
            .release_scan(MODE_KEY)
            .mode(mode)
    }

    //////////////////////////////////////////////////////////////////////////
    // Expectations
    //////////////////////////////////////////////////////////////////////////

    /// Expect a specific action.
    fn expect(&mut self, action: Actions) -> &mut Self {
        self.steps.push(ActorStep::Action(action));
        self
    }

    /// Expect that no actions are pending.
    fn idle(&mut self) -> &mut Self {
        self.steps.push(ActorStep::Idle);
        self
    }

    /// Expect a mode change.
    fn mode(&mut self, mode: LayoutMode) -> &mut Self {
        self.expect(Actions::SetMode(mode))
    }

    /// Expect a mode selection indication.
    fn mode_select(&mut self, mode: LayoutMode) -> &mut Self {
        self.expect(Actions::SetModeSelect(mode))
    }

    /// Expect a key press, with the given modifiers held.
    fn presses(&mut self, key: Keyboard, mods: Mods) -> &mut Self {
        self.expect(Actions::SendKey(KeyAction::KeyPress(key, mods)))
    }

    /// Expect the "everything released" report.
    fn releases(&mut self) -> &mut Self {
        self.expect(Actions::SendKey(KeyAction::KeyRelease))
    }

    /// Expect a report holding only modifiers.
    fn mod_only(&mut self, mods: Mods) -> &mut Self {
        self.expect(Actions::SendKey(KeyAction::ModOnly(mods)))
    }

    /// Expect a plain key to be typed: pressed and then released.
    fn types(&mut self, key: Keyboard) -> &mut Self {
        self.types_mods(key, Mods::empty())
    }

    /// Expect a key to be typed with modifiers held.
    fn types_mods(&mut self, key: Keyboard, mods: Mods) -> &mut Self {
        self.presses(key, mods).releases()
    }

    /// Expect a steno stroke.
    fn steno_stroke(&mut self, stroke: Stroke) -> &mut Self {
        self.expect(Actions::SendRawSteno(stroke))
    }

    //////////////////////////////////////////////////////////////////////////
    // Execution
    //////////////////////////////////////////////////////////////////////////

    /// Run the script, checking each expectation as it is reached, and that
    /// nothing extra is left over at the end.
    fn run(&mut self) {
        block_on(async {
            let mut layout = LayoutManager::new(false);
            let actor = TestActor::new();

            for (num, step) in self.steps.iter().enumerate() {
                match step {
                    ActorStep::Tick(t) => {
                        layout.tick(&actor, *t).await;
                    }
                    ActorStep::Event(e) => {
                        layout.handle_event(*e, &actor).await;
                    }
                    ActorStep::Action(expected) => {
                        let act = actor.actions.borrow_mut().pop_front();
                        match act {
                            Some(act) => {
                                assert_eq!(&act, expected, "step {}", num);
                            }
                            None => {
                                panic!("step {}: expected action {:?}, but none found", num, expected);
                            }
                        }
                    }
                    ActorStep::Idle => {
                        let pending = actor.actions.borrow();
                        assert!(
                            pending.is_empty(),
                            "step {}: expected nothing pending, but found {:?}",
                            num,
                            pending
                        );
                    }
                }
            }

            if !actor.actions.borrow().is_empty() {
                panic!(
                    "Expected no actions to be pending, but found {:?}",
                    actor.actions.borrow()
                );
            }
        });
    }
}

//////////////////////////////////////////////////////////////////////////////
// Tests
//////////////////////////////////////////////////////////////////////////////

/// Makes sure we come up in qwerty mode successfully, and can type a few
/// things.  This isn't a test of qwerty, just basic functionality.
#[test]
fn test_qwerty_basic() {
    let mut script = Script::new();

    // Press the 'Q' key.  Qwerty wants 50ms to determine keys vs chords.
    script
        .press_scan(4)
        .tick(50)
        .expect(Actions::SendKey(KeyAction::KeySet(vec![Keyboard::Q])))
        .release_scan(4)
        .expect(Actions::SendKey(KeyAction::KeySet(vec![])));

    script.run();
}

/// The mode key cycles through the modes.
#[test]
fn test_mode_switch() {
    let mut script = Script::new();
    script.to_mode(LayoutMode::Steno).to_mode(LayoutMode::Taipo);
    script.run();
}

/// Every key, by itself, on both sides.
#[test]
fn test_single_keys() {
    let mut script = Script::taipo();

    let keys = [
        (A, Keyboard::A),
        (O, Keyboard::O),
        (T, Keyboard::T),
        (E, Keyboard::E),
        (R, Keyboard::R),
        (S, Keyboard::S),
        (N, Keyboard::N),
        (I, Keyboard::I),
        (SP, Keyboard::Space),
        (BK, Keyboard::DeleteBackspace),
    ];

    for &(chord, key) in &keys {
        script.chord(LEFT, chord).types(key);
        script.chord(RIGHT, chord).types(key);
    }

    script.run();
}

/// A chord that is tapped, and released before the chord timer expires, is sent
/// as soon as the last key comes up.
#[test]
fn test_quick_tap() {
    let mut script = Script::taipo();

    script.press(LEFT, A).tick(5).idle();
    script.release(LEFT, A).tick(1).types(Keyboard::A);

    // The same, for a multi-key chord.
    script.press(RIGHT, S | N).tick(5).idle();
    script.release(RIGHT, S | N).tick(1).types(Keyboard::P);

    script.run();
}

/// Multi-key chords, including ones that only differ by the row.
#[test]
fn test_chords() {
    let mut script = Script::taipo();

    let chords = [
        (S | N, Keyboard::P),
        (O | E, Keyboard::C),
        (T | E, Keyboard::H),
        (N | A, Keyboard::J),
        (I | O, Keyboard::K),
        (S | N | I, Keyboard::Tab),
        (O | T | E, Keyboard::ReturnEnter),
    ];

    for &(chord, key) in &chords {
        script.chord(LEFT, chord).types(key);
        script.chord(RIGHT, chord).types(key);
    }

    script.run();
}

/// Adding the space thumb to a letter shifts it.
#[test]
fn test_shifted() {
    let mut script = Script::taipo();

    script
        .chord(LEFT, SP | A)
        .types_mods(Keyboard::A, Mods::SHIFT);
    script
        .chord(RIGHT, SP | S | N)
        .types_mods(Keyboard::P, Mods::SHIFT);
    // The backspace thumb gives punctuation, which is shifted for some keys.
    script
        .chord(LEFT, BK | A)
        .types_mods(Keyboard::Comma, Mods::SHIFT);
    script.chord(LEFT, BK | T).types(Keyboard::LeftBrace);

    script.run();
}

/// Both thumbs together with a chord give the function keys.
#[test]
fn test_function_keys() {
    let mut script = Script::taipo();

    script.chord(LEFT, SP | BK | O | E).types(Keyboard::F1);
    script.chord(RIGHT, SP | BK | T | E).types(Keyboard::F10);
    script.chord(LEFT, SP | BK | S | E).types(Keyboard::F11);
    script.chord(RIGHT, SP | BK | I | A).types(Keyboard::F12);

    script.run();
}

/// A modifier chord is sent immediately, and applies to the next key typed.
#[test]
fn test_oneshot_modifier() {
    let mut script = Script::taipo();

    // The shift modifier is the pinky pair.  It is sent as soon as the chord is
    // recognized, and releasing the chord doesn't send anything.
    script.press(LEFT, I | E).tick(CHORD_TIME).mod_only(Mods::SHIFT);
    script.release(LEFT, I | E).tick(1).idle();

    // The next key carries the modifier, ...
    script.chord(LEFT, A).types_mods(Keyboard::A, Mods::SHIFT);

    // ... and only that key.
    script.chord(LEFT, A).types(Keyboard::A);

    // The modifier can come from the other hand.
    script.chord(RIGHT, N | T).mod_only(Mods::CONTROL);
    script.chord(LEFT, A).types_mods(Keyboard::A, Mods::CONTROL);

    script.run();
}

/// Modifiers accumulate until a key is typed.
#[test]
fn test_oneshot_accumulate() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(LEFT, R | A).mod_only(Mods::SHIFT | Mods::GUI);
    script
        .chord(RIGHT, S | N)
        .types_mods(Keyboard::P, Mods::SHIFT | Mods::GUI);
    script.chord(RIGHT, S | N).types(Keyboard::P);

    script.run();
}

/// Both thumbs together release held modifiers, without typing anything.
#[test]
fn test_modifier_release() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases();
    // With no modifiers held, the null chord does nothing at all.
    script.chord(LEFT, SP | BK).idle();
    // And the modifier really is gone.
    script.chord(LEFT, A).types(Keyboard::A);

    script.run();
}

/// Double-pressing a modifier makes it sticky: it stays held across keys,
/// until the null chord releases everything.
#[test]
fn test_sticky_modifier() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    // The second press adds no new modifiers; it silently promotes to sticky.
    script.chord(LEFT, I | E).idle();
    // Keys carry the modifier, and their release keeps it held rather than
    // clearing the report.
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT)
        .mod_only(Mods::SHIFT);
    script
        .chord(LEFT, O)
        .presses(Keyboard::O, Mods::SHIFT)
        .mod_only(Mods::SHIFT);
    // The null chord releases everything, ...
    script.chord(LEFT, SP | BK).releases();
    // ... and the next key is unmodified.
    script.chord(LEFT, A).types(Keyboard::A);

    script.run();
}

/// The motivating case: double-press alt, then tap tab repeatedly to cycle
/// windows.  Alt never drops between the taps.
#[test]
fn test_sticky_alt_tab() {
    let mut script = Script::taipo();

    script.chord(LEFT, S | O).mod_only(Mods::ALT);
    script.chord(LEFT, S | O).idle();
    for _ in 0..3 {
        script
            .chord(LEFT, S | N | I)
            .presses(Keyboard::Tab, Mods::ALT)
            .mod_only(Mods::ALT);
    }
    script.chord(LEFT, SP | BK).releases();
    script.chord(LEFT, S | N | I).types(Keyboard::Tab);

    script.run();
}

/// The double press can also be the matching modifier chord on the other
/// hand.
#[test]
fn test_sticky_cross_hand() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(RIGHT, I | E).idle();
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT)
        .mod_only(Mods::SHIFT);
    script
        .chord(RIGHT, A)
        .presses(Keyboard::A, Mods::SHIFT)
        .mod_only(Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases();

    script.run();
}

/// A single modifier press on top of sticky modifiers is still one-shot: it
/// applies to the next key only, while the sticky modifiers stay held.
#[test]
fn test_oneshot_on_sticky() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(LEFT, I | E).idle();
    // A single ctrl press accumulates as usual.
    script.chord(RIGHT, N | T).mod_only(Mods::SHIFT | Mods::CONTROL);
    // The next key gets both, but only shift survives its release.
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT | Mods::CONTROL)
        .mod_only(Mods::SHIFT);
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT)
        .mod_only(Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases();

    script.run();
}

/// Pressing a modifier twice on top of sticky modifiers promotes everything
/// held, including the new modifier, to sticky.
#[test]
fn test_sticky_accumulate() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(LEFT, I | E).idle();
    script.chord(RIGHT, N | T).mod_only(Mods::SHIFT | Mods::CONTROL);
    script.chord(RIGHT, N | T).idle();
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT | Mods::CONTROL)
        .mod_only(Mods::SHIFT | Mods::CONTROL);
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::SHIFT | Mods::CONTROL)
        .mod_only(Mods::SHIFT | Mods::CONTROL);
    script.chord(LEFT, SP | BK).releases();

    script.run();
}

/// A `Shifted` chord while sticky modifiers are held: shift rides along for
/// the keypress, and the release falls back to just the sticky modifiers.
#[test]
fn test_sticky_shifted_action() {
    let mut script = Script::taipo();

    script.chord(LEFT, S | O).mod_only(Mods::ALT);
    script.chord(LEFT, S | O).idle();
    script
        .chord(LEFT, SP | A)
        .presses(Keyboard::A, Mods::ALT | Mods::SHIFT)
        .mod_only(Mods::ALT);
    script.chord(LEFT, SP | BK).releases();

    script.run();
}

/// Rollover with sticky modifiers held: the modifiers persist through the
/// release of the rolled-over key, both across hands and within one.
#[test]
fn test_sticky_rollover() {
    let mut script = Script::taipo();

    script.chord(LEFT, I | E).mod_only(Mods::SHIFT);
    script.chord(LEFT, I | E).idle();

    // Cross-hand rollover: the 'a' is released (to modifiers only, not a full
    // clear) to make room for the 'i'.
    script.press(LEFT, A).tick(CHORD_TIME).presses(Keyboard::A, Mods::SHIFT);
    script
        .press(RIGHT, I)
        .tick(CHORD_TIME)
        .mod_only(Mods::SHIFT)
        .presses(Keyboard::I, Mods::SHIFT);

    // Same-side rollover: 'n' on the left while the 'a' is still held.
    script
        .press(LEFT, N)
        .tick(CHORD_TIME)
        .mod_only(Mods::SHIFT)
        .presses(Keyboard::N, Mods::SHIFT);

    // The 'a' is inactive now; its release does nothing.
    script.release(LEFT, A).tick(1).idle();
    script.release(RIGHT, I).tick(1).mod_only(Mods::SHIFT);
    script.release(LEFT, N).tick(1).idle();

    script.chord(LEFT, SP | BK).releases();

    script.run();
}

/// A chord with no entry in the action table types nothing, and leaves the
/// state clean for the next chord.
#[test]
fn test_unknown_chord() {
    let mut script = Script::taipo();

    // All four keys of the bottom row is not a defined chord.
    script.chord(LEFT, A | O | T | E).idle();
    script.chord(LEFT, A).types(Keyboard::A);

    script.run();
}

/// Rollover between the two hands: the second hand's chord can be pressed
/// before the first hand's is released.  Because the HID layer only holds a
/// single key down, pressing the new key releases the old one.
#[test]
fn test_cross_hand_rollover() {
    let mut script = Script::taipo();

    // Press 'a' on the left, and hold it.
    script.press(LEFT, A).tick(CHORD_TIME).presses(Keyboard::A, Mods::empty());

    // 'i' on the right, while the left is still held.  The 'a' is released to
    // make room for it.
    script
        .press(RIGHT, I)
        .tick(CHORD_TIME)
        .releases()
        .presses(Keyboard::I, Mods::empty());

    // Releasing the left key releases the key that is down.
    script.release(LEFT, A).tick(1).releases();
    script.release(RIGHT, I).tick(1).idle();

    script.run();
}

/// The same character can be typed twice in a row by alternating hands.
#[test]
fn test_alternate_same_key() {
    let mut script = Script::taipo();

    script.press(LEFT, A).tick(CHORD_TIME).presses(Keyboard::A, Mods::empty());
    script
        .press(RIGHT, A)
        .tick(CHORD_TIME)
        .releases()
        .presses(Keyboard::A, Mods::empty());
    script.release(LEFT, A).tick(1).releases();
    script.release(RIGHT, A).tick(1).idle();

    script.run();
}

/// Same-side rollover.  Typing "captain" rolls 'a' (left), 'i' (right), 'n'
/// (left), with the 'a' still held when the 'n' goes down.  All three have to
/// be typed, in order.
#[test]
fn test_same_side_rollover() {
    let mut script = Script::taipo();

    script.press(LEFT, A).tick(CHORD_TIME).presses(Keyboard::A, Mods::empty());
    script
        .press(RIGHT, I)
        .tick(CHORD_TIME)
        .releases()
        .presses(Keyboard::I, Mods::empty());

    // The 'n' on the left, with the 'a' still held, starts a new chord, which
    // ends the one the 'a' belongs to.
    script
        .press(LEFT, N)
        .tick(CHORD_TIME)
        .releases()
        .presses(Keyboard::N, Mods::empty());

    // The 'a' is inactive now, and its release does nothing.
    script.release(LEFT, A).tick(1).idle();
    script.release(RIGHT, I).tick(1).releases();
    script.release(LEFT, N).tick(1).idle();

    script.run();
}

/// A rolled chord that is tapped, rather than held, while the previous chord is
/// still down.
#[test]
fn test_same_side_rollover_tap() {
    let mut script = Script::taipo();

    script.press(LEFT, A).tick(CHORD_TIME).presses(Keyboard::A, Mods::empty());
    // A quick 'o' while the 'a' is held is sent as soon as it comes up.
    script
        .press(LEFT, O)
        .tick(5)
        .releases()
        .release(LEFT, O)
        .tick(1)
        .types(Keyboard::O);
    script.release(LEFT, A).tick(1).idle();

    script.run();
}

/// In steno mode, taipo chords are decoded but not sent; the keys are steno
/// keys instead.
#[test]
fn test_steno_suppresses_taipo() {
    let mut script = Script::steno();

    // The left 'a' key is steno 'S'.
    script
        .press(LEFT, A)
        .tick(CHORD_TIME)
        .idle()
        .release(LEFT, A)
        .tick(1)
        .steno_stroke(bbq_steno::Stroke::from_text("S").unwrap())
        .idle();

    script.run();
}

/// Holding a taipo key while in steno mode acts as a layer shift: the chord is
/// typed as taipo, and no steno stroke is sent.
#[test]
fn test_steno_taipo_latch() {
    let mut script = Script::steno();

    script.press_scan(TAIPO_KEY);
    script
        .press(LEFT, A)
        .tick(CHORD_TIME)
        .presses(Keyboard::A, Mods::empty());
    script.release(LEFT, A).tick(1).releases();
    script.release_scan(TAIPO_KEY).tick(1).idle();

    script.run();
}
