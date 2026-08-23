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
    layout::{LayoutActions, LayoutManager, TAIPO_CHORD_TIME},
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

/// Bottom row, pinky to index.
const A: u16 = 0x001;
const O: u16 = 0x002;
const T: u16 = 0x004;
const E: u16 = 0x008;
/// Top row, pinky to index.
const R: u16 = 0x010;
const S: u16 = 0x020;
const N: u16 = 0x040;
const I: u16 = 0x080;
/// The thumb keys.
const SP: u16 = 0x100;
const BK: u16 = 0x200;

/// The same chord bits, named by the letter the *Posh* table types with them
/// (see POSH.md).  Posh leaves the pinkies out, so the two pinky bits (taipo's
/// `R` and `A`) have no name here.
mod posh {
    /// Top row: ring, middle, index.
    pub const A: u16 = 0x020;
    pub const N: u16 = 0x040;
    pub const I: u16 = 0x080;
    /// Bottom row: ring, middle, index.
    pub const O: u16 = 0x002;
    pub const T: u16 = 0x004;
    pub const E: u16 = 0x008;
    /// The thumbs, which are taipo's.
    pub const SP: u16 = super::SP;
    pub const BK: u16 = super::BK;
}

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

/// The same keys, one physical row further down, which is where they live when
/// the layout is in the "lower" row position.  The thumb keys don't move.
static SCANS_LOWER: [[u8; 10]; 2] = [
    // A   O   T   E   R  S   N   I   SP  BK
    [6, 10, 14, 18, 5, 9, 13, 17, 19, 23],
    [30, 34, 38, 42, 29, 33, 37, 41, 43, 47],
];

/// The mode selection key.
const MODE_KEY: u8 = 2;

/// One of the "taipo shift" keys, which allows taipo to be typed while in steno
/// mode.
const TAIPO_KEY: u8 = 20;

/// The same taipo shift key when the layout is in the lower row position.
const TAIPO_KEY_LOWER: u8 = 21;

/// The dead top-left key, which toggles the row position.
const ROW_TOGGLE_KEY: u8 = 0;

/// The steno `#` key of the outer left column, which toggles between the taipo
/// and posh chord tables while in taipo mode.
const POSH_TOGGLE_KEY: u8 = 1;

/// The scan codes of the keys making up a chord, in bit order.
fn scans(side: Side, chord: u16, lower: bool) -> impl Iterator<Item = u8> {
    let row = if lower {
        SCANS_LOWER[side.index()]
    } else {
        SCANS[side.index()]
    };
    (0..10).filter_map(move |bit| {
        if chord & (1 << bit) != 0 {
            Some(row[bit])
        } else {
            None
        }
    })
}

/// How long taipo waits before deciding a chord is complete.  Taken from the
/// layout itself, so that changing the chord window doesn't silently invalidate
/// every test that waits for it.
const CHORD_TIME: usize = TAIPO_CHORD_TIME as usize;

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
    /// Expect the modifier indicator to show this `(oneshot, sticky)` state.
    ModState(Mods, Mods),
}

/// Keep track of the state of the test, as well as the state we think the
/// keyboard should be in.
struct TestActor {
    /// Actions that have been queued up.
    actions: RefCell<VecDeque<Actions>>,

    /// The latest `(oneshot, sticky)` modifier state reported for the
    /// indicator.  This is kept out of `actions` because it is reported
    /// whenever it changes, and the existing tests aren't written to expect it.
    mod_state: RefCell<(Mods, Mods)>,
}

impl TestActor {
    fn new() -> Self {
        Self {
            actions: RefCell::new(VecDeque::new()),
            mod_state: RefCell::new((Mods::empty(), Mods::empty())),
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

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        *self.mod_state.borrow_mut() = (oneshot, sticky);
    }
}

/// A scripted test.
///
/// The builder methods all return `&mut Self` so that a test can be written as
/// a chain of stimulus and expectation.
struct Script {
    steps: Vec<ActorStep>,

    /// Build the layout as if the board only has two rows.
    two_row: bool,

    /// Which physical keys the chord builders use.  This tracks the row
    /// position the layout is expected to be in.
    lower: bool,

    /// Whether the taipo engine is expected to be using the posh table.  This
    /// tracks which sub-mode action `toggle_posh` should expect.
    posh: bool,
}

impl Script {
    /// A new script, starting in the layout's initial (qwerty) mode.  The
    /// layout announces its mode on the first tick.
    fn new() -> Script {
        let mut script = Script {
            steps: Vec::new(),
            two_row: false,
            lower: false,
            posh: false,
        };
        script.tick(1).mode(LayoutMode::Qwerty);
        script
    }

    /// A new script on a two-row board, which comes up in taipo mode.
    fn two_row() -> Script {
        let mut script = Script {
            steps: Vec::new(),
            two_row: true,
            lower: false,
            posh: false,
        };
        script.tick(1).mode(LayoutMode::Taipo);
        script
    }

    /// A new script, already switched into taipo mode.
    fn taipo() -> Script {
        let mut script = Script::new();
        script.to_mode(LayoutMode::Steno).to_mode(LayoutMode::Taipo);
        script
    }

    /// A new script, already in taipo mode with the posh table selected.
    fn posh() -> Script {
        let mut script = Script::taipo();
        script.toggle_posh();
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
        for scan in scans(side, chord, self.lower) {
            self.press_scan(scan);
        }
        self
    }

    /// Release all of the keys of a chord, with no time in between.
    fn release(&mut self, side: Side, chord: u16) -> &mut Self {
        for scan in scans(side, chord, self.lower) {
            self.release_scan(scan);
        }
        self
    }

    /// Tap the row toggle key by itself, which moves the 2-row layouts between
    /// the upper and lower rows.  Subsequent chords use the new scan codes.
    fn toggle_rows(&mut self) -> &mut Self {
        self.press_scan(ROW_TOGGLE_KEY)
            .release_scan(ROW_TOGGLE_KEY)
            .tick(1);
        self.lower = !self.lower;
        self
    }

    /// Tap the variant toggle key by itself, which switches the taipo engine
    /// between the taipo and posh chord tables, and expect the sub-mode report
    /// that goes with the new variant.
    fn toggle_posh(&mut self) -> &mut Self {
        self.posh = !self.posh;
        let action = if self.posh {
            Actions::SetSubMode(MinorMode::Posh)
        } else {
            Actions::ClearSubMode(MinorMode::Posh)
        };
        self.press_scan(POSH_TOGGLE_KEY)
            .release_scan(POSH_TOGGLE_KEY)
            .expect(action)
            .tick(1)
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

    /// Expect the modifier indicator to be showing this state.
    fn mod_state(&mut self, oneshot: Mods, sticky: Mods) -> &mut Self {
        self.steps.push(ActorStep::ModState(oneshot, sticky));
        self
    }

    /// Expect the modifier indicator to be showing nothing.
    fn no_mod_state(&mut self) -> &mut Self {
        self.mod_state(Mods::empty(), Mods::empty())
    }

    //////////////////////////////////////////////////////////////////////////
    // Execution
    //////////////////////////////////////////////////////////////////////////

    /// Run the script, checking each expectation as it is reached, and that
    /// nothing extra is left over at the end.
    fn run(&mut self) {
        block_on(async {
            let mut layout = LayoutManager::new(self.two_row);
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
                    ActorStep::ModState(oneshot, sticky) => {
                        assert_eq!(
                            *actor.mod_state.borrow(),
                            (*oneshot, *sticky),
                            "step {}",
                            num
                        );
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

//////////////////////////////////////////////////////////////////////////////
// Row position
//
// On a 3-row board, the 2-row layouts (taipo and steno) can be moved between
// the top two and the bottom two rows by tapping the otherwise dead top-left
// key.
//////////////////////////////////////////////////////////////////////////////

/// After toggling, taipo chords are typed on the lower two rows, and the old
/// scan codes no longer produce anything.  Toggling back restores the original
/// mapping.
#[test]
fn test_row_toggle_taipo() {
    let mut script = Script::taipo();

    script.toggle_rows();

    // The lower-row scan codes now type normally.
    script.chord(LEFT, A).types(Keyboard::A);
    script.chord(RIGHT, N | I).types(Keyboard::Y);

    // The top physical row is dead for taipo (it holds the steno `#` codes).
    script
        .press_scan(4)
        .press_scan(8)
        .tick(CHORD_TIME)
        .release_scan(4)
        .release_scan(8)
        .tick(1)
        .idle();

    // Toggling back returns to the upper rows, where the same chords type the
    // same letters.
    script.toggle_rows();
    script.chord(LEFT, A).types(Keyboard::A);
    script.chord(RIGHT, N | I).types(Keyboard::Y);

    script.run();
}

/// The toggle key produces nothing of its own in taipo mode.
#[test]
fn test_row_toggle_is_consumed_taipo() {
    let mut script = Script::taipo();

    script.toggle_rows().idle();
    script.toggle_rows().idle();

    script.run();
}

/// In steno mode the toggle key used to send an empty stroke; it is now
/// consumed entirely.
#[test]
fn test_row_toggle_is_consumed_steno() {
    let mut script = Script::steno();

    script.toggle_rows().idle();
    script.toggle_rows().idle();

    script.run();
}

/// The toggle only happens when the key is tapped by itself.  Pressing it while
/// another key is held leaves the mapping alone.
#[test]
fn test_row_toggle_needs_solo_tap() {
    let mut script = Script::taipo();

    // Press it in the middle of a chord.
    script
        .press(LEFT, A)
        .press_scan(ROW_TOGGLE_KEY)
        .release_scan(ROW_TOGGLE_KEY)
        .tick(CHORD_TIME)
        .presses(Keyboard::A, Mods::empty())
        .release(LEFT, A)
        .tick(1)
        .releases();

    // Press it first, but release it after another key has gone down.
    script
        .press_scan(ROW_TOGGLE_KEY)
        .press(LEFT, O)
        .release_scan(ROW_TOGGLE_KEY)
        .tick(CHORD_TIME)
        .presses(Keyboard::O, Mods::empty())
        .release(LEFT, O)
        .tick(1)
        .releases();

    // Still on the upper rows.
    script.chord(LEFT, T).types(Keyboard::T);

    script.run();
}

/// Steno strokes move down a row as well, with the physical top row picking up
/// the `#` keys.
#[test]
fn test_row_toggle_steno() {
    let mut script = Script::steno();

    script.toggle_rows();

    // Physical middle and bottom of the third column are steno 'T' and 'K'.
    script
        .press_scan(9)
        .press_scan(10)
        .tick(CHORD_TIME)
        .idle()
        .release_scan(9)
        .steno_stroke(Stroke::from_text("TK").unwrap())
        .release_scan(10)
        .tick(1)
        .idle();

    // The physical top row of that column is now the number key.
    script
        .press_scan(8)
        .tick(CHORD_TIME)
        .idle()
        .release_scan(8)
        .steno_stroke(Stroke::from_text("#").unwrap())
        .tick(1)
        .idle();

    // The right outer column rotates too, so the physical middle key there is
    // '-D'.
    script
        .press_scan(25)
        .tick(CHORD_TIME)
        .idle()
        .release_scan(25)
        .steno_stroke(Stroke::from_text("-D").unwrap())
        .tick(1)
        .idle();

    script.run();
}

/// The taipo shift keys move down along with everything else, and still allow
/// taipo to be typed from steno mode.
#[test]
fn test_row_toggle_steno_taipo_latch() {
    let mut script = Script::steno();

    script.toggle_rows();

    script.press_scan(TAIPO_KEY_LOWER);
    script
        .press(LEFT, A)
        .tick(CHORD_TIME)
        .presses(Keyboard::A, Mods::empty());
    script.release(LEFT, A).tick(1).releases();
    script.release_scan(TAIPO_KEY_LOWER).tick(1).idle();

    script.run();
}

/// The mode select chords stay on fixed physical keys; they are not remapped
/// with the row position.
#[test]
fn test_row_toggle_mode_select() {
    let mut script = Script::taipo();

    script.toggle_rows();

    // Mode key plus the physical qwerty 's' selects steno.
    script
        .press_scan(MODE_KEY)
        .mode_select(LayoutMode::Qwerty)
        .press_scan(9)
        .mode_select(LayoutMode::Steno)
        .release_scan(9)
        .mode_select(LayoutMode::Steno)
        .release_scan(MODE_KEY)
        .mode(LayoutMode::Steno);

    // The row position survived the mode change.
    script
        .press_scan(9)
        .tick(CHORD_TIME)
        .idle()
        .release_scan(9)
        .steno_stroke(Stroke::from_text("T").unwrap())
        .tick(1)
        .idle();

    script.run();
}

/// A two-row board has no third row to move to, so the toggle does nothing.
#[test]
fn test_row_toggle_two_row() {
    let mut script = Script::two_row();

    script
        .press_scan(ROW_TOGGLE_KEY)
        .release_scan(ROW_TOGGLE_KEY)
        .tick(1)
        .idle();

    // The original scan codes still work,
    script.chord(LEFT, A).types(Keyboard::A);

    // and the shifted ones do not.
    script
        .press_scan(6)
        .tick(CHORD_TIME)
        .release_scan(6)
        .tick(1)
        .idle();

    script.run();
}

/// The modifier state reported for the indicator tracks the one-shot
/// modifiers, and clears when a key consumes them.
#[test]
fn test_mod_state_oneshot() {
    let mut script = Script::taipo();

    script.no_mod_state();

    script
        .chord(LEFT, I | E)
        .mod_only(Mods::SHIFT)
        .mod_state(Mods::SHIFT, Mods::empty());

    // Accumulating a second modifier shows both, still one-shot.
    script
        .chord(LEFT, R | A)
        .mod_only(Mods::SHIFT | Mods::GUI)
        .mod_state(Mods::SHIFT | Mods::GUI, Mods::empty());

    // Typing a key consumes them, and the indicator goes out.
    script
        .chord(RIGHT, S | N)
        .types_mods(Keyboard::P, Mods::SHIFT | Mods::GUI)
        .no_mod_state();

    script.run();
}

/// A sticky modifier is reported as sticky, and survives the keys it
/// modifies.
#[test]
fn test_mod_state_sticky() {
    let mut script = Script::taipo();

    script
        .chord(LEFT, S | O)
        .mod_only(Mods::ALT)
        .mod_state(Mods::ALT, Mods::empty());

    // The double press promotes it.
    script.chord(LEFT, S | O).idle().mod_state(Mods::ALT, Mods::ALT);

    // Typing doesn't clear it.
    script
        .chord(LEFT, S | N | I)
        .presses(Keyboard::Tab, Mods::ALT)
        .mod_only(Mods::ALT)
        .mod_state(Mods::ALT, Mods::ALT);

    // A one-shot on top of sticky is reported as held but not sticky, ...
    script
        .chord(RIGHT, N | T)
        .mod_only(Mods::ALT | Mods::CONTROL)
        .mod_state(Mods::ALT | Mods::CONTROL, Mods::ALT);

    // ... and only it goes away with the next key.
    script
        .chord(LEFT, A)
        .presses(Keyboard::A, Mods::ALT | Mods::CONTROL)
        .mod_only(Mods::ALT)
        .mod_state(Mods::ALT, Mods::ALT);

    // The null chord clears everything.
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    script.run();
}

//////////////////////////////////////////////////////////////////////////////
// Posh
//
// Posh is a second chord table for the same taipo engine, selected by tapping
// the otherwise dead steno `#` key of the outer left column while in taipo
// mode.  It uses the ring, middle and index fingers only, leaving the pinky
// keys dead.
//////////////////////////////////////////////////////////////////////////////

/// The six finger keys and the two thumbs, on both hands, alone and with the
/// space thumb for the capital.
#[test]
fn test_posh_single_keys() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    for side in [LEFT, RIGHT] {
        script.chord(side, A).types(Keyboard::A);
        script.chord(side, N).types(Keyboard::N);
        script.chord(side, I).types(Keyboard::I);
        script.chord(side, O).types(Keyboard::O);
        script.chord(side, T).types(Keyboard::T);
        script.chord(side, E).types(Keyboard::E);

        script.chord(side, SP).types(Keyboard::Space);
        script.chord(side, BK).types(Keyboard::DeleteBackspace);

        script.chord(side, A | SP).types_mods(Keyboard::A, Mods::SHIFT);
        script.chord(side, N | SP).types_mods(Keyboard::N, Mods::SHIFT);
        script.chord(side, I | SP).types_mods(Keyboard::I, Mods::SHIFT);
        script.chord(side, O | SP).types_mods(Keyboard::O, Mods::SHIFT);
        script.chord(side, T | SP).types_mods(Keyboard::T, Mods::SHIFT);
        script.chord(side, E | SP).types_mods(Keyboard::E, Mods::SHIFT);
    }

    script.run();
}

/// The backspace thumb turns the single keys into the navigation cluster, and
/// both thumbs into the far-motion keys.
#[test]
fn test_posh_navigation() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    script.chord(LEFT, E | BK).types(Keyboard::RightArrow);
    script.chord(LEFT, T | BK).types(Keyboard::DownArrow);
    script.chord(LEFT, A | BK).types(Keyboard::Escape);
    script.chord(LEFT, O | BK).types(Keyboard::LeftArrow);
    script.chord(LEFT, I | BK).types(Keyboard::ReturnEnter);
    script.chord(LEFT, N | BK).types(Keyboard::UpArrow);

    script.chord(RIGHT, E | SP | BK).types(Keyboard::End);
    script.chord(RIGHT, T | SP | BK).types(Keyboard::PageDown);
    script.chord(RIGHT, A | SP | BK).types(Keyboard::DeleteForward);
    script.chord(RIGHT, O | SP | BK).types(Keyboard::Home);
    script.chord(RIGHT, I | SP | BK).types(Keyboard::Tab);
    script.chord(RIGHT, N | SP | BK).types(Keyboard::PageUp);

    script.run();
}

/// A sample of the multi-key chords: the two same-row pairs, a few three-key
/// chords, and some cross-row ones.
#[test]
fn test_posh_chords() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    // The same-row pairs.
    script.chord(LEFT, N | I).types(Keyboard::S);
    script.chord(RIGHT, T | E).types(Keyboard::H);

    // Whole-row chords.
    script.chord(LEFT, A | N | I).types(Keyboard::P);
    script.chord(RIGHT, O | T | E).types(Keyboard::B);

    // Cross-row chords.
    script.chord(LEFT, N | E).types(Keyboard::R);
    script.chord(RIGHT, A | N | E).types(Keyboard::K);
    script.chord(LEFT, A | T | E).types(Keyboard::X);
    script.chord(RIGHT, A | N).types(Keyboard::D);

    // And their capitals.
    script.chord(LEFT, N | I | SP).types_mods(Keyboard::S, Mods::SHIFT);
    script.chord(RIGHT, N | E | SP).types_mods(Keyboard::R, Mods::SHIFT);

    script.run();
}

/// The backspace thumb gives the digits and the symbols, and both thumbs the
/// function keys and the rest of the symbols.
#[test]
fn test_posh_digits_and_symbols() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    // Digits.
    script.chord(LEFT, N | E | BK).types(Keyboard::Keyboard0);
    script.chord(LEFT, A | N | BK).types(Keyboard::Keyboard1);
    script.chord(LEFT, I | T | BK).types(Keyboard::Keyboard2);
    script.chord(RIGHT, N | I | O | BK).types(Keyboard::Keyboard9);

    // Symbols on the backspace thumb.
    script.chord(LEFT, A | N | I | BK).types_mods(Keyboard::Equal, Mods::SHIFT);
    script.chord(LEFT, O | T | E | BK).types(Keyboard::Minus);
    script.chord(RIGHT, A | N | E | BK).types(Keyboard::Semicolon);
    script.chord(LEFT, A | T | E | BK).types_mods(Keyboard::Keyboard4, Mods::SHIFT);
    script.chord(LEFT, N | I | BK).types(Keyboard::Comma);

    // Function keys on both thumbs.
    script.chord(LEFT, A | N | SP | BK).types(Keyboard::F1);
    script.chord(RIGHT, N | T | E | SP | BK).types(Keyboard::F12);
    script.chord(LEFT, A | O | E | SP | BK).types(Keyboard::F11);

    // And the rest of the symbols.
    script.chord(LEFT, A | N | I | SP | BK).types(Keyboard::Equal);
    script.chord(RIGHT, A | N | E | SP | BK).types_mods(Keyboard::Backslash, Mods::SHIFT);
    script.chord(LEFT, T | E | SP | BK).types_mods(Keyboard::Apostrophe, Mods::SHIFT);
    script.chord(RIGHT, N | I | SP | BK).types(Keyboard::Apostrophe);

    // The punctuation-only chords, which have no both-thumbs variant.
    script.chord(LEFT, A | T).types_mods(Keyboard::ForwardSlash, Mods::SHIFT);
    script.chord(LEFT, A | T | SP).types_mods(Keyboard::Keyboard1, Mods::SHIFT);
    script.chord(LEFT, A | T | BK).types_mods(Keyboard::Keyboard6, Mods::SHIFT);
    script.chord(RIGHT, I | O | E).types(Keyboard::Grave);
    script
        .press(RIGHT, I | O | E | SP | BK)
        .tick(CHORD_TIME)
        .release(RIGHT, I | O | E | SP | BK)
        .tick(1)
        .idle();

    script.run();
}

/// The modifier chords, which behave exactly as they do in taipo.  The wiki's
/// `ralt` chord is a plain shift here.
#[test]
fn test_posh_modifiers() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    // One-shot gui, consumed by the next key.
    script.chord(LEFT, N | T).mod_only(Mods::GUI);
    script.chord(RIGHT, A).types_mods(Keyboard::A, Mods::GUI);

    // Control and alt.
    script.chord(LEFT, I | E).mod_only(Mods::CONTROL);
    script.chord(LEFT, A | O).mod_only(Mods::CONTROL | Mods::ALT);

    // The `ralt` chord is shift, and accumulates like the others.
    script.chord(RIGHT, A | O | T).mod_only(Mods::CONTROL | Mods::ALT | Mods::SHIFT);

    // Both thumbs alone releases everything.
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    // The both-thumbs variants are pairs of modifiers.
    script.chord(LEFT, N | T | SP | BK).mod_only(Mods::GUI | Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    script.chord(RIGHT, I | E | SP | BK).mod_only(Mods::CONTROL | Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    script.chord(LEFT, A | O | SP | BK).mod_only(Mods::ALT | Mods::SHIFT);
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    // A double press makes the held modifiers sticky, as in taipo.
    script
        .chord(LEFT, A | O)
        .mod_only(Mods::ALT)
        .mod_state(Mods::ALT, Mods::empty());
    script.chord(RIGHT, A | O).idle().mod_state(Mods::ALT, Mods::ALT);
    script
        .chord(LEFT, I | SP | BK)
        .presses(Keyboard::Tab, Mods::ALT)
        .mod_only(Mods::ALT);
    script.chord(LEFT, SP | BK).releases().no_mod_state();

    // The bracket layers of the modifier chords.
    script.chord(LEFT, I | E | SP).types(Keyboard::RightBrace);
    script.chord(LEFT, I | E | BK).types(Keyboard::LeftBrace);
    script.chord(LEFT, A | O | SP).types_mods(Keyboard::RightBrace, Mods::SHIFT);
    script.chord(LEFT, A | O | BK).types_mods(Keyboard::LeftBrace, Mods::SHIFT);
    script.chord(LEFT, N | T | SP).types_mods(Keyboard::Keyboard0, Mods::SHIFT);
    script.chord(LEFT, N | T | BK).types_mods(Keyboard::Keyboard9, Mods::SHIFT);

    script.run();
}

/// The two extras that are plain keys.
#[test]
fn test_posh_extras() {
    use posh::{A, E, I, N, O, T};

    let mut script = Script::posh();

    script.chord(LEFT, N | T | O).types(Keyboard::PrintScreen);
    script.chord(RIGHT, A | I | E).types(Keyboard::Insert);

    script.run();
}

/// Posh excludes the pinkies, so the two pinky keys do nothing at all, either
/// on their own or as part of a chord.  Chords that aren't in the table are
/// equally dead.
#[test]
fn test_posh_pinky_is_dead() {
    let mut script = Script::posh();

    // Taipo's `r` and `a`, which are the pinky keys.
    script.press(LEFT, R).tick(CHORD_TIME).release(LEFT, R).tick(1).idle();
    script.press(RIGHT, A).tick(CHORD_TIME).release(RIGHT, A).tick(1).idle();

    // A chord that would be posh's `n` if the pinky weren't in it.
    script
        .press(LEFT, R | posh::N)
        .tick(CHORD_TIME)
        .release(LEFT, R | posh::N)
        .tick(1)
        .idle();

    // One of the wiki's empty rows.
    script
        .press(LEFT, posh::A | posh::N | posh::T)
        .tick(CHORD_TIME)
        .release(LEFT, posh::A | posh::N | posh::T)
        .tick(1)
        .idle();

    script.run();
}

/// `l` has a second, locally added chord, because the wiki's index-top plus
/// middle-bottom splay is awkward to hit.  Both spellings work, on both hands,
/// and on every thumb layer.
#[test]
fn test_posh_l_alias() {
    let mut script = Script::posh();

    let wiki = posh::I | posh::T;
    let alias = posh::N | posh::I | posh::E;

    script.chord(LEFT, wiki).types(Keyboard::L);
    script.chord(LEFT, alias).types(Keyboard::L);
    script.chord(RIGHT, alias).types(Keyboard::L);

    script
        .chord(LEFT, alias | posh::SP)
        .types_mods(Keyboard::L, Mods::SHIFT);
    script.chord(LEFT, alias | posh::BK).types(Keyboard::Keyboard2);
    script
        .chord(LEFT, alias | posh::SP | posh::BK)
        .types(Keyboard::F2);

    script.run();
}

/// Toggling a second time comes back to taipo, where the pinky keys work
/// again.
#[test]
fn test_posh_toggle_back() {
    let mut script = Script::taipo();

    // Taipo's `r` is on the pinky.
    script.chord(LEFT, R).types(Keyboard::R);

    script.toggle_posh();
    script.press(LEFT, R).tick(CHORD_TIME).release(LEFT, R).tick(1).idle();
    script.chord(LEFT, posh::A).types(Keyboard::A);

    script.toggle_posh();
    script.chord(LEFT, R).types(Keyboard::R);
    // Taipo's `s` is where posh's `a` is.
    script.chord(LEFT, S).types(Keyboard::S);

    script.run();
}

/// The variant belongs to the taipo engine, not to the mode, so it survives a
/// trip out to another mode and back.
#[test]
fn test_posh_survives_mode_change() {
    let mut script = Script::posh();

    script.to_mode(LayoutMode::Qwerty).to_mode(LayoutMode::Steno);
    script.to_mode(LayoutMode::Taipo);

    script.chord(LEFT, posh::N | posh::I).types(Keyboard::S);

    script.run();
}

/// The taipo latch in steno mode uses whichever table is selected.
#[test]
fn test_posh_steno_taipo_latch() {
    let mut script = Script::posh();

    // Taipo mode to steno, via the escape key tap.
    script
        .press_scan(TAIPO_KEY)
        .release_scan(TAIPO_KEY)
        .mode(LayoutMode::Steno)
        .tick(1);

    script.press_scan(TAIPO_KEY);
    script
        .press(LEFT, posh::A)
        .tick(CHORD_TIME)
        .presses(Keyboard::A, Mods::empty());
    script.release(LEFT, posh::A).tick(1).releases();
    script.release_scan(TAIPO_KEY).tick(1).idle();

    script.run();
}

/// The toggle only fires on a solo tap, so it can never change the table in
/// the middle of a chord.
#[test]
fn test_posh_toggle_needs_solo_tap() {
    let mut script = Script::taipo();

    // Tapped in the middle of a chord.
    script
        .press(LEFT, R)
        .press_scan(POSH_TOGGLE_KEY)
        .release_scan(POSH_TOGGLE_KEY)
        .tick(CHORD_TIME)
        .presses(Keyboard::R, Mods::empty())
        .release(LEFT, R)
        .tick(1)
        .releases();

    // Pressed first, but released after another key has gone down.
    script
        .press_scan(POSH_TOGGLE_KEY)
        .press(LEFT, S)
        .release_scan(POSH_TOGGLE_KEY)
        .tick(CHORD_TIME)
        .presses(Keyboard::S, Mods::empty())
        .release(LEFT, S)
        .tick(1)
        .releases();

    // Still taipo, so the pinky still types.
    script.chord(LEFT, R).types(Keyboard::R);

    script.run();
}

/// Outside of taipo mode the key keeps its own meaning, and never toggles.
#[test]
fn test_posh_toggle_only_in_taipo() {
    // In qwerty it is the escape key.
    let mut script = Script::new();
    script
        .press_scan(POSH_TOGGLE_KEY)
        .tick(50)
        .expect(Actions::SendKey(KeyAction::KeySet(vec![Keyboard::Escape])))
        .release_scan(POSH_TOGGLE_KEY)
        .expect(Actions::SendKey(KeyAction::KeySet(vec![])));
    script.run();

    // In steno it is the `#` key.
    let mut script = Script::steno();
    script
        .press_scan(POSH_TOGGLE_KEY)
        .tick(CHORD_TIME)
        .idle()
        .release_scan(POSH_TOGGLE_KEY)
        .steno_stroke(Stroke::from_text("#").unwrap())
        .tick(1)
        .idle();

    // Back in taipo it toggles, and the choice takes effect immediately.
    script.to_mode(LayoutMode::Taipo);
    script.toggle_posh();
    script.chord(LEFT, posh::N | posh::I).types(Keyboard::S);

    script.run();
}

/// The toggle works on a two-row board, which is where the key is the
/// lower-left one.
#[test]
fn test_posh_toggle_two_row() {
    let mut script = Script::two_row();

    script.chord(LEFT, R).types(Keyboard::R);

    script.toggle_posh();
    script.chord(LEFT, posh::N | posh::I).types(Keyboard::S);
    script.press(LEFT, R).tick(CHORD_TIME).release(LEFT, R).tick(1).idle();

    script.run();
}

/// The outer left column doesn't move with the row position, so the toggle
/// still works there, and posh chords land on the lower scan codes.
#[test]
fn test_posh_toggle_lower_rows() {
    let mut script = Script::taipo();

    script.toggle_rows();
    script.toggle_posh();

    script.chord(LEFT, posh::A).types(Keyboard::A);
    script.chord(RIGHT, posh::N | posh::I).types(Keyboard::S);
    script.chord(LEFT, posh::N | posh::E | BK).types(Keyboard::Keyboard0);

    // The pinky is still dead down here.
    script.press(LEFT, R).tick(CHORD_TIME).release(LEFT, R).tick(1).idle();

    script.run();
}
