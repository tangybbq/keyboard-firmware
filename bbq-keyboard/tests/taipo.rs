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
