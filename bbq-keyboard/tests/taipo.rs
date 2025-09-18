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
    KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode,
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
        panic!("set_mode_select not implemented");
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

        let tests = gen_qwerty_tests();

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

/// Generate some basic qwerty mode sanity tests.
fn gen_qwerty_tests() -> Vec<ActorStep> {
    // Start with a tick which should send us the event indicating our
    // initial mode is qwerty.
    let mut tests = vec![
        ActorStep::Tick(1),
        ActorStep::Action(Actions::SetMode(LayoutMode::Qwerty)),
    ];

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

    tests
}
