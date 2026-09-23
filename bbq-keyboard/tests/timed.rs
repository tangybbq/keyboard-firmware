//! Tests for `TimedLayout`: driving the layout from a clock gives the same
//! results as ticking it every millisecond.
//!
//! Each test runs the same key events through two copies of the layout.  One
//! is ticked every millisecond, as the replay does, and the other is driven by
//! a `TimedLayout`, woken only at key events and at its deadline.  Every
//! `LayoutActions` call is recorded, and the two records have to match, call
//! for call.  The times may differ by the one millisecond described in the
//! `timed` module: the timed layout never reports anything earlier, and never
//! more than a millisecond later.

#![cfg(feature = "proto3")]

use std::cell::{Cell, RefCell};

use bbq_keyboard::layout::taipo::{TaipoVariant, SCAN_MAP};
use bbq_keyboard::layout::{
    LayoutActions, LayoutManager, TimedLayout, FN_LEFT, FN_RIGHT, MODE_KEY,
};
use bbq_keyboard::replay::{log_from_text, markers_from_text};
use bbq_keyboard::{KeyAction, KeyEvent, LayoutMode, MinorMode, Mods, Side};
use futures::executor::block_on;

/// Every call the layout makes, with the time it was made at.
#[derive(Default)]
struct Recorder {
    now: Cell<u64>,
    calls: RefCell<Vec<(u64, String)>>,
}

impl Recorder {
    fn push(&self, call: String) {
        self.calls.borrow_mut().push((self.now.get(), call));
    }

    /// The mode most recently set, if any.
    fn mode(&self) -> Option<String> {
        self.calls
            .borrow()
            .iter()
            .rev()
            .find(|(_, call)| call.starts_with("SetMode("))
            .map(|(_, call)| call.clone())
    }
}

impl LayoutActions for Recorder {
    async fn set_mode(&self, mode: LayoutMode) {
        self.push(format!("SetMode({mode:?})"));
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        self.push(format!("SetModeSelect({mode:?})"));
    }

    async fn send_key(&self, key: KeyAction) {
        self.push(format!("SendKey({key:?})"));
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        self.push(format!("SetSubMode({submode:?})"));
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        self.push(format!("ClearSubMode({submode:?})"));
    }

    #[cfg(feature = "steno")]
    async fn send_raw_steno(&self, stroke: bbq_steno::Stroke) {
        self.push(format!("SendRawSteno({stroke:?})"));
    }

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        self.push(format!("SetModState({oneshot:?}, {sticky:?})"));
    }

    async fn taipo_chord(&self, side: Side, code: u16, end: bbq_keyboard::layout::ChordEnd) {
        self.push(format!("TaipoChord({side:?}, {code:#x}, {end:?})"));
    }

    #[cfg(feature = "orsy")]
    async fn orsy_stroke(
        &self,
        chord: bbq_orsy::Chord,
        outcome: bbq_keyboard::layout::orsy::StrokeOutcome,
    ) {
        self.push(format!("OrsyStroke({chord:?}, {outcome:?})"));
    }

    async fn set_row_position(&self, lower: bool) {
        self.push(format!("SetRowPosition({lower})"));
    }
}

/// A key event at a time in milliseconds.
#[derive(Clone, Copy, Debug)]
struct Timed {
    time: u64,
    event: KeyEvent,
}

/// How long both drivers keep going after the last event, so that anything
/// still waiting on a timer is committed: past both the taipo chord window and
/// the qwerty combo wait.
const TAIL_MS: u64 = 200;

/// Run the events through a layout ticked every millisecond.
///
/// As in the replay, the tick for millisecond 0 comes first, to announce the
/// mode, and after that events are delivered before their own millisecond's
/// tick.
fn per_ms(layout: LayoutManager, events: &[Timed]) -> Vec<(u64, String)> {
    let mut layout = layout;
    let rec = Recorder::default();
    let mut now = 0;
    let tick = |layout: &mut LayoutManager, now: &mut u64| {
        rec.now.set(*now);
        block_on(layout.tick(&rec, 1));
        *now += 1;
    };
    tick(&mut layout, &mut now);
    for ev in events {
        while now < ev.time {
            tick(&mut layout, &mut now);
        }
        rec.now.set(ev.time);
        block_on(layout.handle_event(ev.event, &rec));
    }
    let end = events.last().map_or(0, |ev| ev.time) + TAIL_MS;
    while now <= end {
        tick(&mut layout, &mut now);
    }
    rec.calls.take()
}

/// Run the events through a `TimedLayout`, waking it at its deadlines and at
/// the events.
///
/// `spurious` adds wakes at times nothing is due, from the given generator,
/// which is how the firmware's scheduler behaves when an event moves the
/// deadline it was waiting on.
fn timed(layout: LayoutManager, events: &[Timed], mut spurious: Option<&mut Lcg>) -> Vec<(u64, String)> {
    let rec = Recorder::default();
    block_on(async {
        let mut timed = TimedLayout::new(layout, 0);
        timed.wake(0, &rec).await;
        let mut last = 0;
        for ev in events {
            // Every deadline before the event.  One at the event's own time is
            // left for the event's catch-up, which has to handle it too.
            while let Some(deadline) = timed.deadline() {
                if deadline >= ev.time {
                    break;
                }
                assert!(deadline >= last, "deadline {deadline} is before {last}");
                rec.now.set(deadline);
                timed.wake(deadline, &rec).await;
                last = deadline;
            }
            if let Some(rng) = spurious.as_deref_mut() {
                if rng.below(4) == 0 && ev.time > last {
                    let at = last + rng.below(ev.time - last + 1);
                    rec.now.set(at);
                    timed.wake(at, &rec).await;
                }
            }
            rec.now.set(ev.time);
            timed.handle_event(ev.event, ev.time, &rec).await;
            last = ev.time;
        }
        while let Some(deadline) = timed.deadline() {
            assert!(deadline > last || deadline == 0, "stuck at {deadline}");
            rec.now.set(deadline);
            timed.wake(deadline, &rec).await;
            last = deadline;
        }
        // Nothing more is due, however long we wait.
        let before = rec.calls.borrow().len();
        rec.now.set(last + TAIL_MS);
        timed.wake(last + TAIL_MS, &rec).await;
        assert_eq!(rec.calls.borrow().len(), before, "a wake with nothing due did something");
    });
    rec.calls.take()
}

/// Check that the two drivers made the same calls, in the same order, with
/// the timed layout never early and at most a millisecond late.
fn check_same(what: &str, per_ms: &[(u64, String)], timed: &[(u64, String)]) {
    for (num, (a, b)) in per_ms.iter().zip(timed.iter()).enumerate() {
        assert_eq!(a.1, b.1, "{what}: call {num} differs (per-ms at {}, timed at {})", a.0, b.0);
        assert!(
            b.0 == a.0 || b.0 == a.0 + 1,
            "{what}: call {num}, {}, per-ms at {} but timed at {}",
            a.1,
            a.0,
            b.0
        );
    }
    assert_eq!(
        per_ms.len(),
        timed.len(),
        "{what}: per-ms made {} calls, timed made {}",
        per_ms.len(),
        timed.len()
    );
}

/// A small seeded generator, so the streams are the same on every run and
/// nothing new is needed to make them.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    /// A number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The keys the streams are made of: every taipo key on both hands most of
/// the time, and now and then one of the others.
fn pick_key(rng: &mut Lcg, taipo: &[u8]) -> u8 {
    match rng.below(20) {
        // The mode key, rarely, so that each stream visits a few modes.
        0 => MODE_KEY,
        // The taipo shift keys, the row and variant toggles, and the Fn keys.
        1 => [20, 44, 22, 46][rng.below(4) as usize],
        2 => [0, 1, FN_LEFT, FN_RIGHT][rng.below(4) as usize],
        // Anything on the board, for qwerty and steno.
        3 => rng.below(48) as u8,
        _ => taipo[rng.below(taipo.len() as u64) as usize],
    }
}

/// The time to the next event: mostly the gaps of fast typing, some around
/// the chord and combo windows, and some long pauses.
fn pick_gap(rng: &mut Lcg) -> u64 {
    match rng.below(20) {
        0..=9 => rng.below(25),
        10..=13 => rng.below(130),
        14..=16 => [49, 50, 51, 99, 100, 101][rng.below(6) as usize],
        _ => 150 + rng.below(1500),
    }
}

/// A random stream of events: presses and releases that make sense together,
/// ending with every key released.
fn stream(seed: u64, len: usize) -> Vec<Timed> {
    let taipo: Vec<u8> = (0..SCAN_MAP.len() as u8)
        .filter(|code| SCAN_MAP[*code as usize].is_some())
        .collect();
    let mut rng = Lcg(seed);
    let mut held: Vec<u8> = Vec::new();
    let mut time = 1;
    let mut events = Vec::new();
    for _ in 0..len {
        time += pick_gap(&mut rng);
        let release = !held.is_empty() && (held.len() >= 6 || rng.below(2) == 0);
        let event = if release {
            let key = held.remove(rng.below(held.len() as u64) as usize);
            KeyEvent::Release(key)
        } else {
            let key = pick_key(&mut rng, &taipo);
            if held.contains(&key) {
                held.retain(|k| *k != key);
                KeyEvent::Release(key)
            } else {
                held.push(key);
                KeyEvent::Press(key)
            }
        };
        events.push(Timed { time, event });
    }
    for key in held {
        time += pick_gap(&mut rng);
        events.push(Timed { time, event: KeyEvent::Release(key) });
    }
    events
}

/// Random streams, on both kinds of board, give the same calls either way.
#[test]
fn test_equivalence_fuzz() {
    for seed in 0..60 {
        for two_row in [false, true] {
            let events = stream(seed, 400);
            let what = format!("seed {seed}, two_row {two_row}");
            let a = per_ms(LayoutManager::new(two_row), &events);
            let b = timed(LayoutManager::new(two_row), &events, None);
            check_same(&what, &a, &b);
            let mut rng = Lcg(seed ^ 0x5eed);
            let c = timed(LayoutManager::new(two_row), &events, Some(&mut rng));
            check_same(&format!("{what}, spurious wakes"), &a, &c);
        }
    }
}

/// The streams really do exercise the timers: some chords are committed by
/// the window running out, and some qwerty keys by the combo wait.
#[test]
fn test_fuzz_reaches_timers() {
    let mut expired = 0;
    let mut modes = std::collections::BTreeSet::new();
    for seed in 0..60 {
        let calls = per_ms(LayoutManager::new(false), &stream(seed, 400));
        for (_, call) in &calls {
            if call.ends_with("TimerExpired)") {
                expired += 1;
            }
            if call.starts_with("SetMode(") {
                modes.insert(call.clone());
            }
        }
    }
    assert!(expired > 100, "only {expired} chords ended on the timer");
    assert!(modes.len() >= 3, "only visited {modes:?}");
}

/// A layout in the given mode and chord table, as the replay sets one up.
fn prepared(two_row: bool, mode: LayoutMode, variant: TaipoVariant) -> LayoutManager {
    let mut layout = LayoutManager::new(two_row);
    layout.set_taipo_variant(variant);
    let rec = Recorder::default();
    block_on(async {
        layout.tick(&rec, 1).await;
        let want = format!("SetMode({mode:?})");
        for _ in 0..8 {
            if rec.mode().as_deref() == Some(&want) {
                return;
            }
            layout.handle_event(KeyEvent::Press(MODE_KEY), &rec).await;
            layout.handle_event(KeyEvent::Release(MODE_KEY), &rec).await;
        }
        panic!("the mode cycle does not reach {mode:?}");
    });
    layout
}

/// The golden key logs give the same calls either way.
#[test]
fn test_equivalence_golden() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "log"))
        .collect();
    names.sort();
    assert!(!names.is_empty());
    for path in names {
        let text = std::fs::read_to_string(&path).unwrap();
        let mut mode = LayoutMode::Taipo;
        let mut variant = TaipoVariant::DEFAULT;
        for (_, name, value) in markers_from_text(&text) {
            match name {
                "mode" => match LayoutMode::from_marker(value) {
                    Some(m) => mode = m,
                    None => continue,
                },
                "variant" => variant = TaipoVariant::from_marker(value),
                _ => (),
            }
        }
        let events: Vec<Timed> = log_from_text(&text)
            .unwrap()
            .into_iter()
            .map(|ev| Timed {
                // Clear of the tick at 0, which the drivers keep for
                // themselves.
                time: u64::from(ev.time_ms) + 1,
                event: if ev.press {
                    KeyEvent::Press(ev.key)
                } else {
                    KeyEvent::Release(ev.key)
                },
            })
            .collect();
        for two_row in [false, true] {
            let what = format!("{}, two_row {two_row}", path.display());
            let a = per_ms(prepared(two_row, mode, variant), &events);
            let b = timed(prepared(two_row, mode, variant), &events, None);
            assert!(a.len() > 20, "{what}: only {} calls", a.len());
            check_same(&what, &a, &b);
        }
    }
}
