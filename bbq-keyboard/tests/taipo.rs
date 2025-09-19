//! Tests for Taipo
//!
//! Taipo is a fairly unique keyboard layout that has some complexity behind it.
//! These tests test mode switching to/from taipo and cycling through the
//! various modes.

#![allow(dead_code)]

use core::panic;
use std::{cell::RefCell, collections::VecDeque};

use bbq_keyboard::{
    layout::{LayoutActions, LayoutManager},
    KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode, Mods,
};
use bbq_steno::Stroke;
use futures::executor::block_on;

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
    /// Cause this much time to pass for the layout engine (in its ticks).
    Tick(usize),
    /// Send an action.
    Event(KeyEvent),
    /// Expect a keypress action.
    Action(Actions),
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
        // println!("set_mode called with mode: {:?}", mode);
        self.actions.borrow_mut().push_back(Actions::SetMode(mode));
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        println!("set_mode_select called with mode: {:?}", mode);
        self.actions
            .borrow_mut()
            .push_back(Actions::SetModeSelect(mode));
    }

    async fn send_key(&self, key: KeyAction) {
        println!("send_key called with key: {:?}", key);
        self.actions.borrow_mut().push_back(Actions::SendKey(key));
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        println!("set_sub_mode called with submode: {:?}", submode);
        panic!("set_sub_mode not implemented");
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        print!("clear_sub_mode called with submode: {:?}", submode);
        panic!("clear_sub_mode not implemented");
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        println!("send_raw_steno called with stroke: {:?}", stroke);
        panic!("send_raw_steno not implemented");
    }
}

/// Basic test.  Makes sure we come up in qwerty mode successfully, and can type
/// a few things.  This isn't a test of qwerty, just basic functionality.
static BASIC_TEST: [ActorStep; 2] = [
    ActorStep::Tick(1),
    ActorStep::Action(Actions::SetMode(LayoutMode::Qwerty)),
];

#[test]
fn test_basic_layout() {
    block_on(async {
        let mut layout = LayoutManager::new(false);
        let mut actor = TestActor::new();

        let mut tests = create_initial_set_mode();
        gen_qwerty_tests(&mut tests);
        switch_qwerty_to_taipo(&mut tests);
        gen_taipo_rollover_tests(&mut tests);

        for step in &tests {
            match step {
                ActorStep::Tick(t) => {
                    layout.tick(&mut actor, *t).await;
                }
                ActorStep::Event(e) => {
                    layout.handle_event(*e, &actor).await;
                }
                ActorStep::Action(a) => {
                    let act = actor.actions.borrow_mut().pop_front();
                    match act {
                        Some(act) => {
                            assert_eq!(&act, a);
                        }
                        None => {
                            panic!("Expected action {:?}, but none found", a);
                        }
                    }
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

/// Create the initial set mode that comes in when the layout manager starts.
fn create_initial_set_mode() -> Vec<ActorStep> {
    vec![
        ActorStep::Tick(1),
        ActorStep::Action(Actions::SetMode(LayoutMode::Qwerty)),
    ]
}

/// Generate some basic qwerty mode sanity tests.
fn gen_qwerty_tests(tests: &mut Vec<ActorStep>) {
    // Press the 'Q' key.
    tests.push(ActorStep::Event(KeyEvent::Press(4)));

    // qwerty wants 50 ms to determine keys vs chords. So, tick 50ms, to get our
    // down event.
    tests.push(ActorStep::Tick(50));
    tests.push(ActorStep::Action(Actions::SendKey(KeyAction::KeySet(
        vec![Keyboard::Q],
    ))));
    tests.push(ActorStep::Event(KeyEvent::Release(4)));
    tests.push(ActorStep::Action(Actions::SendKey(KeyAction::KeySet(
        vec![],
    ))));
}

/// Switch to taipo mode.
fn switch_qwerty_to_taipo(tests: &mut Vec<ActorStep>) {
    for mode in &[LayoutMode::Steno, LayoutMode::Taipo] {
        // Press the mode switch key.
        tests.push(ActorStep::Event(KeyEvent::Press(2)));
        tests.push(ActorStep::Tick(50));
        tests.push(ActorStep::Action(Actions::SetModeSelect(*mode)));
        tests.push(ActorStep::Event(KeyEvent::Release(2)));
        tests.push(ActorStep::Action(Actions::SetMode(*mode)));
    }
}

/// Generate tests for Taipo mode hand rollover functionality. Tests basic
/// left-to-right hand rollover with timing considerations.
///
/// This doesn't extensively test the layout, and just uses a few keys across
/// the keyboard.
fn gen_taipo_rollover_tests(tests: &mut Vec<ActorStep>) {
    // Test 1: Basic left hand chord, then right hand chord (no overlap)
    // Left hand: 12 is just the 'n'.
    tests.push(ActorStep::Event(KeyEvent::Press(12)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(12)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::N);

    // Left hand: 8 is just the 's'.
    tests.push(ActorStep::Event(KeyEvent::Press(12)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(12)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::N);

    // Pressing both within the chord timeout will give the 'p' key.
    tests.push(ActorStep::Event(KeyEvent::Press(12)));
    tests.push(ActorStep::Tick(10)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Press(8)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(12)));
    tests.push(ActorStep::Event(KeyEvent::Release(8)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::P);

    // Test these keys on the right side as well.
    tests.push(ActorStep::Event(KeyEvent::Press(36)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(36)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::N);

    // Right hand: 8 is just the 's'.
    tests.push(ActorStep::Event(KeyEvent::Press(32)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(32)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::S);

    // Pressing both within the chord timeout will give the 'p' key.
    tests.push(ActorStep::Event(KeyEvent::Press(36)));
    tests.push(ActorStep::Tick(10)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Press(32)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(36)));
    tests.push(ActorStep::Event(KeyEvent::Release(32)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::P);

    // For basic rollover, across the two sides.
    tests.push(ActorStep::Event(KeyEvent::Press(12)));
    tests.push(ActorStep::Tick(10)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Press(32)));
    tests.push(ActorStep::Event(KeyEvent::Press(8)));
    tests.push(ActorStep::Tick(50)); // Within chord timeout
    tests.push(ActorStep::Event(KeyEvent::Release(12)));
    tests.push(ActorStep::Event(KeyEvent::Release(32)));
    tests.push(ActorStep::Event(KeyEvent::Release(8)));
    tests.push(ActorStep::Tick(50));
    expect_taipo_press_release(tests, Keyboard::P);
    expect_taipo_press_release(tests, Keyboard::S);
}

/// Add an expectation of a press-release sequence from the taipo side.  Most of
/// the ordinary keys are sent this way
fn expect_taipo_press_release(tests: &mut Vec<ActorStep>, key: Keyboard) {
    tests.push(ActorStep::Action(Actions::SendKey(KeyAction::KeyPress(
        key,
        Mods::empty(),
    ))));
    tests.push(ActorStep::Action(Actions::SendKey(KeyAction::KeyRelease)));
}
