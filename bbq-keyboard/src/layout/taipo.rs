//! Taipo keyboard support.
//!
//! The Taipo keyboard layout is a chorded layout, in that each
//! half of the keyboard is complete. However, it makes use of 2 thumb keys for
//! each half, resulting in each half having 10 keys.
//!
//! The two halves are completely symmetrical, and the intent is to be able to
//! freely type between the two halves, allowing, for example, rollover between
//! the halves.  As such, we have to maintain the state of the two halves
//! separately.
//!
//! Variants:
//!
//! Everything here other than the chord table is shared with the Posh layout
//! (see the `posh` module), which is a Taipo derivative leaving the pinkies
//! out.  `TaipoVariant` selects which table `actions()` looks chords up in;
//! the parent module switches between them, and `LayoutManager` is what
//! decides when.
//!
//! Modifiers:
//!
//! The Taipo layout describes four modifier combos for each side, one for each
//! of the modifiers.  They are described as "one shot".  They way they are
//! implemented here, is that pressing the modifier sends the modifier
//! immediately.  When a non-modifier key is pressed, the modifiers will then
//! all be released.  The two thumb keys together is defined as a "null" key,
//! which will release any pressed modifiers without pressing any keys.  This
//! allows for modifiers to be pressed and released.
//!
//! In addition, modifiers can be made sticky with a double press: a modifier
//! chord whose modifiers are all already held (the same chord again, or its
//! counterpart on the other hand).  This makes all of the currently held
//! modifiers sticky, not just the double-pressed one; there isn't support for
//! making only some of them sticky.  Sticky modifiers remain pressed across
//! any number of keypresses, until the two thumb keys are pressed together.
//! This is useful for some types of GUI manipulation, such as holding down alt
//! while pressing tab or arrow keys.
//!
//! Multi-character chords:
//!
//! A table entry may type a short sequence of characters rather than a single
//! key (`Action::Text`).  Each character is sent as its own HID report, and
//! all but the last are released as they are typed; the last one is left held,
//! so that the chord's release finishes the sequence exactly as it finishes a
//! single-key chord.  Held modifiers apply per character, following the same
//! one-shot/sticky rules as everywhere else, so a one-shot modifier lands on
//! the first character only.  The consequence of leaving the last character
//! held is that holding such a chord auto-repeats only that last character.

use arraydeque::ArrayDeque;
use usbd_human_interface_device::page::Keyboard;

// use crate::log::info;

use crate::usb_typer::key_for_char;
use crate::{KeyEvent, Side, Mods, KeyAction};

use super::{taipo_map, LayoutActions};

/// Which chord table the Taipo engine is interpreting chords with.
///
/// The engine, the chord timing, and the modifier handling are the same for
/// every variant; only the table that maps a chord code to an [`Action`]
/// differs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TaipoVariant {
    /// The Taipo layout itself.
    #[default]
    Taipo,
    /// Posh, a Taipo-derived layout that leaves the pinkies out.  See the
    /// [`posh`](super::posh) module.
    Posh,
}

impl TaipoVariant {
    fn toggle(self) -> Self {
        match self {
            TaipoVariant::Taipo => TaipoVariant::Posh,
            TaipoVariant::Posh => TaipoVariant::Taipo,
        }
    }
}

pub struct TaipoManager {
    /// Managing state for each side.
    sides: [SideManager; 2],

    /// Which chord table chords are looked up in.
    variant: TaipoVariant,

    /// Key events passed through.
    keys: TaipoEvents,

    /// Modifiers that are down.
    oneshot: Mods,

    /// The subset of `oneshot` that is sticky, surviving keypresses until the
    /// null chord releases everything.  Invariant: `sticky ⊆ oneshot`.
    sticky: Mods,

    /// Does the HID have a non-modifier key down?
    down: bool,

    /// Which "Taipo keys" are pressed?  These indicate that typo events should be sent, even while
    /// in Steno mode.
    taipo_keys: u8,

    /// A latch of the taipo keys, to allow these keys to be used like a layer shift.
    taipo_latch: u8,

    /// The `(oneshot, sticky)` pair last reported through
    /// [`LayoutActions::set_mod_state`], so that the indicator is only told
    /// about actual changes.
    reported: (Mods, Mods),
}

impl Default for TaipoManager {
    fn default() -> Self {
        TaipoManager {
            sides: [SideManager::new(Side::Left), SideManager::new(Side::Right)],
            variant: TaipoVariant::default(),
            keys: TaipoEvents::new(),
            oneshot: Mods::empty(),
            sticky: Mods::empty(),
            down: false,
            taipo_keys: 0,
            taipo_latch: 0,
            reported: (Mods::empty(), Mods::empty()),
        }
    }
}

impl TaipoManager {
    /// The chord table for the variant currently selected.
    fn actions(&self) -> &'static [Entry] {
        match self.variant {
            TaipoVariant::Taipo => TAIPO_ACTIONS,
            TaipoVariant::Posh => super::posh::POSH_ACTIONS,
        }
    }

    /// Switch to the other chord table, returning the variant now in use.
    ///
    /// Held modifiers are deliberately left alone; the two variants share the
    /// modifier state, and the caller only toggles with every key released
    /// anyway, so this can never happen in the middle of a chord.
    pub fn toggle_variant(&mut self) -> TaipoVariant {
        self.variant = self.variant.toggle();
        self.variant
    }

    /// Poll doesn't do anything.
    pub fn poll(&mut self) {
    }

    /// Tick is needed to track time.
    ///
    /// The tick also tracks whether we are in steno mode at the time.  In steno mode, everything
    /// works as before, except we don't actually send the resulting keys, unless the taipo modifier
    /// key is pressed.
    pub async fn tick<ACT: LayoutActions>(&mut self, actions: &ACT, ticks: usize, is_steno: bool) {
        self.sides[0].tick(&mut self.keys, ticks);
        self.sides[1].tick(&mut self.keys, ticks);

        // After polling, handle any events.
        while let Some(tevent) = self.keys.pop_front() {
            //info!("ev: p:{}, code:{:x}, sten:{:?}, lat:{}", tevent.is_press, tevent.code, is_steno,
            //      self.taipo_latch);
            if !tevent.is_press {
                // If a key is actually pressed, release it. This shouldn't
                // really need to be conditional.
                if self.down {
                    // `oneshot` is non-empty here only when sticky modifiers
                    // are held; keep them in the report.
                    let action = if self.oneshot.is_empty() {
                        KeyAction::KeyRelease
                    } else {
                        KeyAction::ModOnly(self.oneshot)
                    };
                    self.send(actions, is_steno, action).await;
                    self.down = false;
                    self.taipo_latch = 0;
                }
                continue;
            }

            // Report the chord to whatever is watching, before looking it up.
            // A chord with no table entry is exactly the interesting case, and
            // so is a chord assembled in steno mode whose keys are never sent,
            // so this is not gated on either.
            if let Some(end) = tevent.end {
                actions.taipo_chord(tevent.side, tevent.code, end).await;
            }

            // Look up the code to see if we have an action.  The tables are
            // `'static`, so binding the entry here releases the borrow of
            // `self` that `actions()` takes.
            let entry = self.actions().iter().find(|e| e.code == tevent.code);
            match entry {
                Some(Entry { action: Action::Simple(k), .. }) => {
                    self.release_nonmod(actions, is_steno).await;
                    self.send(actions, is_steno, KeyAction::KeyPress(*k, self.oneshot)).await;
                    self.down = true;
                    self.oneshot = self.sticky;
                }
                Some(Entry { action: Action::Shifted(k), .. }) => {
                    self.release_nonmod(actions, is_steno).await;
                    self.send(actions, is_steno, KeyAction::KeyPress(*k, self.oneshot | Mods::SHIFT)).await;
                    self.down = true;
                    self.oneshot = self.sticky;
                }
                Some(Entry { action: Action::Text(text), .. }) => {
                    self.type_text(actions, is_steno, *text).await;
                }
                Some(Entry { action: Action::OneShot(m), .. }) => {
                    let new_mods = self.oneshot | *m;

                    // If this modification adds any new modifiers, send a new
                    // event.  A press that adds nothing is a double press,
                    // which makes all of the held modifiers sticky.
                    if new_mods != self.oneshot {
                        self.release_nonmod(actions, is_steno).await;
                        self.send(actions, is_steno, KeyAction::ModOnly(new_mods)).await;
                        self.oneshot = new_mods;
                    } else {
                        self.sticky = self.oneshot;
                    }
                }
                Some(Entry { action: Action::Release, .. }) => {
                    if !self.oneshot.is_empty() {
                        self.send(actions, is_steno, KeyAction::KeyRelease).await;
                        self.oneshot = Mods::empty();
                        self.sticky = Mods::empty();
                        self.taipo_latch = 0;
                    }
                }
                None => (),
            }
        }

        // Let the indicator know about any change in the modifiers being held.
        if (self.oneshot, self.sticky) != self.reported {
            self.reported = (self.oneshot, self.sticky);
            actions.set_mod_state(self.oneshot, self.sticky).await;
        }
    }

    /// Type a sequence of characters, one keypress per character.
    ///
    /// Each character is its own HID report.  All but the last are released as
    /// they are typed; the last is left held, so that the chord's release
    /// finishes the sequence exactly as it finishes a single-key chord,
    /// including the taipo latch bookkeeping.
    ///
    /// Because `oneshot` is consumed per character, held modifiers follow the
    /// usual rule: a one-shot modifier applies to the first character only,
    /// while sticky modifiers apply to all of them.  A shift asked for by the
    /// text itself is or-ed in on top.
    ///
    /// Characters the key table has no key for are skipped.  Sequences want to
    /// be short: every character is a report on a queue that drains at one per
    /// millisecond, so a long one would stall the tick it is sent from.
    async fn type_text<ACT: LayoutActions>(
        &mut self,
        actions: &ACT,
        is_steno: bool,
        text: &'static str,
    ) {
        for ch in text.chars() {
            let Some((key, shift)) = key_for_char(ch) else {
                continue;
            };
            self.release_nonmod(actions, is_steno).await;
            self.send(actions, is_steno, KeyAction::KeyPress(key, self.oneshot | shift)).await;
            self.down = true;
            self.oneshot = self.sticky;
        }
    }

    /// Release any non-modifier keys.  Because of the alternation, which could
    /// be for the same key, we simply don't do any rollover, releasing any
    /// pressed non-modifier keys when a new key needs to be pressed.
    ///
    /// The release goes through `send`, so that it is suppressed in steno mode
    /// just as the press that it is undoing would have been.
    async fn release_nonmod<ACT: LayoutActions>(&mut self, actions: &ACT, is_steno: bool) {
        if self.down {
            let action = if self.oneshot.is_empty() {
                KeyAction::KeyRelease
            } else {
                KeyAction::ModOnly(self.oneshot)
            };
            self.send(actions, is_steno, action).await;
            self.down = false;
        }
    }

    pub async fn handle_event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT) {
        let (is_press, code) = match event {
            KeyEvent::Press(code) => (true, code),
            KeyEvent::Release(code) => (false, code),
        };
        let (side, tcode) = if let Some(Some((side, tcode))) = SCAN_MAP.get(code as usize) {
            (side, tcode)
        } else {
            // The map indicates it is dead, check for the special taipo keys.
            if let Some(bit) = taipo_map(code) {
                if is_press {
                    self.taipo_keys |= bit;
                    self.taipo_latch |= bit;
                } else {
                    self.taipo_keys &= !bit;

                    // If the taipo key was pressed, just by itself.  Keys left
                    // over from a rolled-over chord are still counted here, as
                    // this is asking whether anything is physically held.
                    if self.sides[0].pressed == 0 && self.sides[1].pressed == 0 {
                        self.taipo_latch = 0;
                    }
                }
            }

            // Regardless, just return.
            return;
        };
        /*
        let text_side = match side {
            Side::Left => "left",
            Side::Right => "right",
        };
        info!("taipo: p:{}, code:{}, side:{}, tcode:{:x}",
              is_press, code, text_side, tcode);
        */
        if is_press {
            // The hands alternate, so a key landing here means the other hand
            // is done with whatever it was building.  Commit that chord now,
            // rather than making it wait out its timer; this is what keeps the
            // chord window from having to be short enough to separate
            // alternating chords by time alone.
            self.sides[1 - side.index()].force_down(&mut self.keys, ChordEnd::OtherHand);
            self.sides[side.index()].press(*tcode, &mut self.keys);
        } else {
            self.sides[side.index()].release(*tcode, &mut self.keys);
        }

        // While presses are happing, make sure the latch is included.
        if is_press {
            self.taipo_latch |= self.taipo_keys;
        }

        let _ = actions;
    }

    /// Actually perform a keypress action.  This is mitigated by whether we are actually in taipo
    /// mode.  If we're in steno mode, and the taipo shift wasn't pressed, just ignore the event.
    async fn send<ACT: LayoutActions>(&mut self, actions: &ACT, is_steno: bool, action: KeyAction) {
        // info!("steno:{:?}, keys:{:?}, latch:{:?}", is_steno, self.taipo_keys, self.taipo_latch);
        if is_steno && self.taipo_latch == 0 {
            // Steno mode, and no latch, do nothing.
            return;
        }

        actions.send_key(action).await;
    }
}

/// For each side, this tracks the state of keys pressed on that side.
struct SideManager {
    /// Which hand this is.  Carried so that the events it emits can name the
    /// hand without the caller having to add it back.
    side: Side,
    /// Keys that are currently pressed.
    pressed: u16,
    /// Keys that are physically still held, but belong to a chord that has
    /// already been sent.  These take no part in the chord being built, and are
    /// only tracked until they are released.
    inactive: u16,
    /// Keys that have been seen.
    seen: u16,
    /// How many ticks since the last key pressed went down.
    age: u32,
    /// Set when we determined a key was pressed, and sent a code. No more
    /// changes will happen.
    down: bool,
}

// Manage the key presses and releases per-side.  We consider keys that come
// down within a given time interval to be pressed together.  This is more
// strict than what is done for steno.  However, we want to be able to handle
// rollover even just beyond the left-right alternating.

/// How long, in milliseconds, after the first key of a chord we still consider
/// additional keys to be part of that chord.  The chord is sent when this
/// expires, or as soon as all of its keys are released, or as soon as a key
/// goes down on the other side.
///
/// The window can afford to be generous because it is rarely what ends a
/// chord: a chord that is tapped is sent on the release of its last key, and
/// one that is rolled out of is sent when the other hand starts.  What it
/// really bounds is how long a slowly-assembled chord may take to come
/// together, so it wants to be longer than a hand needs to land every finger,
/// and short enough that a deliberately held chord still repeats.
///
/// Re-exported by the parent module as `TAIPO_CHORD_TIME` so that the tests
/// time their chords against the value the layout actually uses.
pub const CHORD_TIME: u32 = 100;

impl SideManager {
    fn new(side: Side) -> Self {
        SideManager {
            side,
            pressed: 0,
            inactive: 0,
            seen: 0,
            age: 0,
            down: false,
        }
    }

    fn press(&mut self, tcode: u16, keys: &mut TaipoEvents) {
        // info!("smpress: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
        if self.down {
            // This chord has already been sent, so this key starts a new one.
            // End the old chord here, rather than waiting for its keys to come
            // up; they become inactive, and are only tracked until they are
            // released.
            let _ = keys.push_back(TaipoEvent {
                is_press: false,
                side: self.side,
                code: self.seen,
                end: None,
            });
            // info!("taipo: rollover release {:x}", self.seen);
            self.inactive |= self.pressed;
            self.seen = 0;
            self.down = false;
        }
        if self.seen == 0 {
            // The window is measured from the first key of the chord, so that a
            // chord that is rolled slowly still lands at a predictable time.
            self.age = 0;
        }
        self.seen |= tcode;
        self.pressed |= tcode;
        // info!("Usmpress: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
    }

    fn release(&mut self, tcode: u16, keys: &mut TaipoEvents) {
        // info!("smrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);
        self.pressed &= !tcode;
        if self.inactive & tcode != 0 {
            // The key was left over from a chord that has already been sent and
            // released, so this is just bookkeeping, freeing the key up to be
            // used by a later chord.
            self.inactive &= !tcode;
            return;
        }
        // If everything taking part in the chord is released, and the timer
        // hasn't expired, we need to send down, and then release.
        if self.pressed & !self.inactive == 0 && self.seen != 0 {
            if !self.down {
                let _ = keys.push_back(TaipoEvent {
                    is_press: true,
                    side: self.side,
                    code: self.seen,
                    end: Some(ChordEnd::AllReleased),
                });
                // info!("taipo: press {:x}", self.seen);
            }
            let _ = keys.push_back(TaipoEvent {
                is_press: false,
                side: self.side,
                code: self.seen,
                end: None,
            });
            // info!("taipo: release {:x}", self.seen);
            self.seen = 0;
            self.age = 0;
            self.down = false;
        }
        // info!("Usmrel: down:{} seen:{}, age:{}", self.down, self.seen, self.age);

    }

    fn tick(&mut self, keys: &mut TaipoEvents, ticks: usize) {
        // If we already sent, or just if nothing has been pressed.
        if self.down || self.seen == 0 {
            return;
        }
        self.age = self.age.saturating_add(ticks as u32);
        if self.age >= CHORD_TIME {
            self.force_down(keys, ChordEnd::TimerExpired);
        }
    }

    /// Commit the chord being built, as though its timer had expired.
    ///
    /// The keys stay held; as with the timer, they take no further part in the
    /// chord, and are only tracked until they come back up.  Does nothing if
    /// there is no chord in progress, or if it has already been sent.  `end`
    /// is the reason to report, which is the timer for a `tick` and the other
    /// hand for a key landing there.
    fn force_down(&mut self, keys: &mut TaipoEvents, end: ChordEnd) {
        if self.down || self.seen == 0 {
            return;
        }
        let _ = keys.push_back(TaipoEvent {
            is_press: true,
            side: self.side,
            code: self.seen,
            end: Some(end),
        });
        // info!("taipo: tpress {:x}", self.seen);
        self.down = true;
    }
}

#[cfg(test)]
mod test_side_manager {
    use super::{ChordEnd, Side, SideManager, TaipoEvents};

    /// The chord window, in the units `spin` counts.  Taken from the layout
    /// itself, so that retuning the window doesn't silently invalidate every
    /// test that waits for it.
    const CHORD_TIME: usize = super::CHORD_TIME as usize;

    struct Tester {
        events: TaipoEvents,
        manager: SideManager,
        /// The termination reasons of the chords committed by the last
        /// `events` call, in order.
        ends: Vec<ChordEnd>,
    }

    impl Tester {
        fn new() -> Tester {
            Tester {
                events: TaipoEvents::new(),
                manager: SideManager::new(Side::Left),
                ends: Vec::new(),
            }
        }

        fn press(&mut self, keys: u16) {
            self.manager.press(keys, &mut self.events);
        }

        fn release(&mut self, keys: u16) {
            self.manager.release(keys, &mut self.events);
        }

        fn spin(&mut self, ticks: usize) {
            self.manager.tick(&mut self.events, ticks);
        }

        /// A key landed on the other hand, which commits whatever this one was
        /// building.  This is what `TaipoManager::handle_event` does.
        fn other_hand(&mut self) {
            self.manager
                .force_down(&mut self.events, ChordEnd::OtherHand);
        }

        /// Check the events produced since the last call, as `(is_press,
        /// code)` pairs.  The side is not checked, as there is only one, and
        /// the termination reasons are checked by `ends`, so that these tests
        /// stay about how the chord is assembled.
        fn events(&mut self, events: &[(bool, u16)]) {
            // Ensure the events match.
            let mut gotten = Vec::new();
            self.ends.clear();
            while let Some(ev) = self.events.pop_front() {
                assert_eq!(ev.side, Side::Left);
                assert_eq!(ev.is_press, ev.end.is_some());
                if let Some(end) = ev.end {
                    self.ends.push(end);
                }
                gotten.push((ev.is_press, ev.code));
            }
            assert_eq!(&gotten[..], events);
        }

        /// Check why each chord committed by the last `events` call ended.
        fn ends(&self, ends: &[ChordEnd]) {
            assert_eq!(&self.ends[..], ends);
        }
    }

    /// A chord that is tapped and released well before the timer expires is
    /// sent as soon as the last key comes up, press immediately followed by
    /// release.
    #[test]
    fn test_quick_tap() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.events(&[]);
        tester.release(1);
        tester.events(&[(true, 1),
                        (false, 1)]);
    }

    /// The chord is committed by the timer at exactly the window, not before.
    #[test]
    fn test_timer_boundary() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME - 1);
        tester.events(&[]);
        tester.spin(1);
        tester.events(&[(true, 1)]);
    }

    /// Several keys released before the timer expires still make up a single
    /// chord, sent when the last of them comes up.
    #[test]
    fn test_release_commits_chord() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.press(2);
        tester.release(1);
        tester.events(&[]);
        tester.release(2);
        tester.events(&[(true, 3),
                        (false, 3)]);
    }

    /// Releasing part of a chord that has already been sent doesn't do
    /// anything; the release comes when the last key is up.
    #[test]
    fn test_partial_release() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.press(2);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 3)]);
        tester.release(1);
        tester.events(&[]);
        tester.release(2);
        tester.events(&[(false, 3)]);
    }

    /// The window is measured from the first key of the chord: a key that
    /// arrives later joins the chord, but doesn't push out when it is sent.
    #[test]
    fn test_fixed_window() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME - 10);
        tester.press(2);
        tester.spin(9);
        tester.events(&[]);
        // The window after the first key, not after the second.
        tester.spin(1);
        tester.events(&[(true, 3)]);
    }

    /// A key that arrives after the window has expired isn't merged into the
    /// chord, but starts a new one.
    #[test]
    fn test_key_after_window() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.press(2);
        tester.spin(CHORD_TIME);
        tester.events(&[(false, 1),
                        (true, 2)]);
        tester.release(1);
        tester.release(2);
        tester.events(&[(false, 2)]);
    }

    /// The event queue is fixed size, and events that don't fit are silently
    /// discarded.  In practice the queue is drained every tick, and a tick can
    /// only produce a couple of events per side, so this only matters if key
    /// events arrive much faster than the layout is ticked.
    #[test]
    fn test_event_queue_overflow() {
        let mut tester = Tester::new();
        let capacity = tester.events.capacity();

        // Each tap produces two events, so one more than the queue holds.
        let mut expected = Vec::new();
        for _ in 0..(capacity / 2 + 1) {
            tester.press(1);
            tester.release(1);
        }
        for _ in 0..(capacity / 2) {
            expected.push((true, 1));
            expected.push((false, 1));
        }
        tester.events(&expected[..]);
    }

    /// Test the basics of the side.  Simulate two keys being pressed, and that
    /// the event is sent when the timer expires.
    #[test]
    fn test_side_manager_basic() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.press(2);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 3)]);
        tester.release(2);
        tester.events(&[]);
        tester.release(1);
        tester.events(&[(false, 3)]);
    }

    /// Test rollover.  Once a set of keys has been pressed, and sent, other
    /// keys can come in, which will be considered part of a new chord.  The
    /// rollover only works with different keys.
    #[test]
    fn test_rollover() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.press(2);
        tester.spin(CHORD_TIME);
        tester.events(&[(false, 1),
                        (true, 2)]);
        tester.release(1);
        tester.events(&[]);
        tester.release(2);
        tester.events(&[(false, 2)]);
    }

    /// A rolled-over chord can also be committed by releasing it, while the
    /// keys of the chord it replaced are still held.
    #[test]
    fn test_rollover_quick_tap() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.press(2);
        tester.events(&[(false, 1)]);
        tester.release(2);
        tester.events(&[(true, 2),
                        (false, 2)]);
        tester.release(1);
        tester.events(&[]);
    }

    /// Rollover repeats: a third chord can start while the keys of the first
    /// are still held down.
    #[test]
    fn test_repeated_rollover() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.press(2);
        tester.spin(CHORD_TIME);
        tester.events(&[(false, 1),
                        (true, 2)]);
        tester.press(4);
        tester.spin(CHORD_TIME);
        tester.events(&[(false, 2),
                        (true, 4)]);
        tester.release(1);
        tester.release(2);
        tester.events(&[]);
        tester.release(4);
        tester.events(&[(false, 4)]);
    }

    /// Each of the three ways a chord can end reports itself, which is what
    /// the host replay reads to tell a struck chord from a rolled one from one
    /// that sat under the fingers until the window ran out.
    #[test]
    fn test_chord_end_reasons() {
        // Every key back up while the window was still open.
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.release(1);
        tester.events(&[(true, 1), (false, 1)]);
        tester.ends(&[ChordEnd::AllReleased]);

        // Held until the window ran out.
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.ends(&[ChordEnd::TimerExpired]);

        // Committed early, because the other hand started.
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(5);
        tester.other_hand();
        tester.events(&[(true, 1)]);
        tester.ends(&[ChordEnd::OtherHand]);
    }

    /// A rolled-over chord on the same hand is committed by the timer or by
    /// its release, exactly as a first chord is; the rollover only ends the
    /// chord that came before, which was already committed.
    #[test]
    fn test_rollover_end_reason() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.ends(&[ChordEnd::TimerExpired]);
        tester.press(2);
        tester.release(2);
        tester.events(&[(false, 1), (true, 2), (false, 2)]);
        tester.ends(&[ChordEnd::AllReleased]);
    }

    /// An inactive key becomes available again as soon as it is released, and
    /// can be used by the chord that is being built.
    #[test]
    fn test_key_reuse_after_rollover() {
        let mut tester = Tester::new();
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 1)]);
        tester.press(2);
        tester.events(&[(false, 1)]);
        tester.release(1);
        tester.press(1);
        tester.spin(CHORD_TIME);
        tester.events(&[(true, 3)]);
        tester.release(1);
        tester.release(2);
        tester.events(&[(false, 3)]);
    }
}

/// Why a chord stopped accumulating and was committed.
///
/// Three quite different pieces of technique, and telling them apart is most
/// of the point of observing the engine at all: a chord that was struck and
/// released, one that was rolled out of, and one that sat under the fingers
/// until the window ran out.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ChordEnd {
    /// Every key of the chord came back up while the window was still open.
    AllReleased,
    /// The chord window expired with keys still held down.
    TimerExpired,
    /// A key landed on the other hand, which ends this hand's chord.
    OtherHand,
}

/// A single press or release indicated by Taipo.
#[derive(Debug, Eq, PartialEq)]
struct TaipoEvent {
    is_press: bool,
    /// The hand the chord was built on.
    side: Side,
    code: u16,
    /// Why the chord was committed.  `None` on a release, which just finishes
    /// a chord that was committed earlier.
    end: Option<ChordEnd>,
}

/// A queue of events recorded.
///
/// The queue is drained on every tick.  Between ticks, each side can add a
/// release for a rolled-over chord, and a press and release for the chord that
/// replaced it, so this is generously sized; events that don't fit are silently
/// dropped.
type TaipoEvents = ArrayDeque<TaipoEvent, 16>;

/// Mapping between scan codes, and Taipo codes.  Taipo codes are a 10 number,
/// with the top two bits as the two thumb keys, then the top row, and bottom
/// row, with bit order represented by the view from the right side.
///
/// On the 3-row boards this describes the layout sitting on the top two rows.
/// The lower row position is handled by remapping the scan codes before they
/// get here (see the row position discussion in the parent module), so there is
/// only the one table.
#[cfg(feature = "proto3")]
pub static SCAN_MAP: [Option<(Side, u16)>; 48] = [
    // 0
    None,
    None,
    None,
    None,
    Some((Side::Left, 0x010)),

    // 5
    Some((Side::Left, 0x001)),
    None,
    None,
    Some((Side::Left, 0x020)),
    Some((Side::Left, 0x002)),

    // 10
    None,
    None,
    Some((Side::Left, 0x040)),
    Some((Side::Left, 0x004)),
    None,

    // 15
    None,
    Some((Side::Left, 0x080)),
    Some((Side::Left, 0x008)),
    None,
    Some((Side::Left, 0x100)),

    // 20
    None,
    None,
    None,
    Some((Side::Left, 0x200)),
    None,

    // 25
    None,
    None,
    None,
    Some((Side::Right, 0x010)),
    Some((Side::Right, 0x001)),

    // 30
    None,
    None,
    Some((Side::Right, 0x020)),
    Some((Side::Right, 0x002)),
    None,

    // 35
    None,
    Some((Side::Right, 0x040)),
    Some((Side::Right, 0x004)),
    None,
    None,

    // 40
    Some((Side::Right, 0x080)),
    Some((Side::Right, 0x008)),
    None,
    Some((Side::Right, 0x100)),
    None,

    // 45
    None,
    None,
    Some((Side::Right, 0x200)),
];

#[cfg(feature = "proto2")]
pub static SCAN_MAP: [Option<(Side, u16)>; 30] = [
    // 0
    Some((Side::Left, 0x200)),
    Some((Side::Left, 0x100)),
    None,
    None,
    None,

    // 5
    Some((Side::Left, 0x008)),
    Some((Side::Left, 0x080)),
    Some((Side::Left, 0x004)),
    Some((Side::Left, 0x040)),
    Some((Side::Left, 0x020)),

    // 10
    Some((Side::Left, 0x002)),
    Some((Side::Left, 0x010)),
    Some((Side::Left, 0x001)),
    None,
    None,

    // 15
    Some((Side::Right, 0x200)),
    Some((Side::Right, 0x100)),
    None,
    None,
    None,

    // 20
    Some((Side::Right, 0x008)),
    Some((Side::Right, 0x080)),
    Some((Side::Right, 0x004)),
    Some((Side::Right, 0x040)),
    Some((Side::Right, 0x020)),

    // 25
    Some((Side::Right, 0x002)),
    Some((Side::Right, 0x010)),
    Some((Side::Right, 0x001)),
    None,
    None,
];

/// An Action is what should happen when particular key or combo is pressed.
/// Taipo does not have anything that acts as a shift key, as all keys are
/// pressed together (like steno).
///
/// Public so that host tools can walk the tables; see
/// [`crate::layout::export`].
pub enum Action {
    Simple(Keyboard),
    Shifted(Keyboard),
    /// Type a short sequence of characters, one keypress each.  The keys come
    /// from the same table [`crate::usb_typer`] types strings with, so the
    /// shift is per character.  Keep these short; see `type_text`.
    Text(&'static str),
    OneShot(Mods),
    Release,
}

/// The mapping between each key and its Action.
pub struct Entry {
    pub code: u16,
    pub action: Action,
}

pub static TAIPO_ACTIONS: &[Entry] = &[
    // The thumb keys by themselves.
    Entry { code: 0x100, action: Action::Simple(Keyboard::Space), },
    Entry { code: 0x200, action: Action::Simple(Keyboard::DeleteBackspace), },

    // The thumb keys together releases any modifiers.
    Entry { code: 0x300, action: Action::Release, },

    // Tab and variants.
    Entry { code: 0x0e0, action: Action::Simple(Keyboard::Tab), },
    Entry { code: 0x1e0, action: Action::Simple(Keyboard::DeleteForward), },
    Entry { code: 0x2e0, action: Action::Simple(Keyboard::Insert), },

    // Enter and variants
    Entry { code: 0x00e, action: Action::Simple(Keyboard::ReturnEnter), },
    Entry { code: 0x10e, action: Action::Simple(Keyboard::Escape), },

    // Multi-character chords: the most valuable English n-grams, on the
    // easiest chords that leave the pinky out.  See `docs/ngrams-results.md`
    // for where both orders come from: grams are ranked by the chords they
    // save once the grams above them already have chords, so `th` is not
    // here -- `the` takes most of what it would save.
    //
    // Five of the thirteen are followed by an extension of themselves, which
    // takes a third to a half of what its base would otherwise type -- `ing`
    // takes 34% of `in`, `ation` 56% of `tion`.  They are here so that the
    // base is never learned as a habit the extension has to break.  Each sits
    // on its base's chord plus one key, so the pair is one shape and not two,
    // and the table order is the order they are worth learning in.
    //
    // The bare chord types the gram, the space thumb capitalizes its first
    // letter.  `+Bk` and both-thumb variants are left unmapped throughout.
    // The comments give the chord's keys, its ease score, and the letters it
    // shares with what it types.
    // eit (2.45) -> 'the', sharing et
    Entry { code: 0x08c, action: Action::Text("the"), },
    Entry { code: 0x18c, action: Action::Text("The"), },

    // ein (2.45) -> 'in', sharing in
    Entry { code: 0x0c8, action: Action::Text("in"), },
    Entry { code: 0x1c8, action: Action::Text("In"), },

    // eint (2.87) = ein + t -> 'ing', sharing in
    Entry { code: 0x0cc, action: Action::Text("ing"), },
    Entry { code: 0x1cc, action: Action::Text("Ing"), },

    // ent (2.47) -> 'er', sharing e
    Entry { code: 0x04c, action: Action::Text("er"), },
    Entry { code: 0x14c, action: Action::Text("Er"), },

    // int (2.47) -> 'an', sharing n
    Entry { code: 0x0c4, action: Action::Text("an"), },
    Entry { code: 0x1c4, action: Action::Text("An"), },

    // inst (4.07) = int + s -> 'and', sharing an
    Entry { code: 0x0e4, action: Action::Text("and"), },
    Entry { code: 0x1e4, action: Action::Text("And"), },

    // not (3.07) -> 'tion', sharing not
    Entry { code: 0x046, action: Action::Text("tion"), },
    Entry { code: 0x146, action: Action::Text("Tion"), },

    // enot (4.07) = not + e -> 'ation', sharing ot
    Entry { code: 0x04e, action: Action::Text("ation"), },
    Entry { code: 0x14e, action: Action::Text("Ation"), },

    // nst (3.07) -> 're'
    Entry { code: 0x064, action: Action::Text("re"), },
    Entry { code: 0x164, action: Action::Text("Re"), },

    // nos (3.29) -> 'or', sharing o
    Entry { code: 0x062, action: Action::Text("or"), },
    Entry { code: 0x162, action: Action::Text("Or"), },

    // nost (3.71) = nos + t -> 'for', sharing o
    Entry { code: 0x066, action: Action::Text("for"), },
    Entry { code: 0x166, action: Action::Text("For"), },

    // ost (3.29) -> 'es', sharing s
    Entry { code: 0x026, action: Action::Text("es"), },
    Entry { code: 0x126, action: Action::Text("Es"), },

    // eis (3.30) -> 'en', sharing e
    Entry { code: 0x0a8, action: Action::Text("en"), },
    Entry { code: 0x1a8, action: Action::Text("En"), },

    // eios (3.94) = eis + o -> 'ent', sharing en
    Entry { code: 0x0aa, action: Action::Text("ent"), },
    Entry { code: 0x1aa, action: Action::Text("Ent"), },

    // eio (3.30) -> 'on', sharing o
    Entry { code: 0x08a, action: Action::Text("on"), },
    Entry { code: 0x18a, action: Action::Text("On"), },

    // eos (3.54) -> 'al'
    Entry { code: 0x02a, action: Action::Text("al"), },
    Entry { code: 0x12a, action: Action::Text("Al"), },

    // ios (3.54) -> 'at'
    Entry { code: 0x0a2, action: Action::Text("at"), },
    Entry { code: 0x1a2, action: Action::Text("At"), },

    // ens (4.00) -> 'st', sharing s
    Entry { code: 0x068, action: Action::Text("st"), },
    Entry { code: 0x168, action: Action::Text("St"), },

    // The single letters, with shift, and the punctuation below these.
    Entry { code: 0x001, action: Action::Simple(Keyboard::A), },
    Entry { code: 0x101, action: Action::Shifted(Keyboard::A), },
    Entry { code: 0x201, action: Action::Shifted(Keyboard::Comma), },

    Entry { code: 0x002, action: Action::Simple(Keyboard::O), },
    Entry { code: 0x102, action: Action::Shifted(Keyboard::O), },
    Entry { code: 0x202, action: Action::Shifted(Keyboard::LeftBrace), },

    Entry { code: 0x004, action: Action::Simple(Keyboard::T), },
    Entry { code: 0x104, action: Action::Shifted(Keyboard::T), },
    Entry { code: 0x204, action: Action::Simple(Keyboard::LeftBrace), },

    Entry { code: 0x008, action: Action::Simple(Keyboard::E), },
    Entry { code: 0x108, action: Action::Shifted(Keyboard::E), },
    Entry { code: 0x208, action: Action::Shifted(Keyboard::Keyboard9), },

    Entry { code: 0x010, action: Action::Simple(Keyboard::R), },
    Entry { code: 0x110, action: Action::Shifted(Keyboard::R), },
    Entry { code: 0x210, action: Action::Shifted(Keyboard::Dot), },

    Entry { code: 0x020, action: Action::Simple(Keyboard::S), },
    Entry { code: 0x120, action: Action::Shifted(Keyboard::S), },
    Entry { code: 0x220, action: Action::Shifted(Keyboard::RightBrace), },

    Entry { code: 0x040, action: Action::Simple(Keyboard::N), },
    Entry { code: 0x140, action: Action::Shifted(Keyboard::N), },
    Entry { code: 0x240, action: Action::Simple(Keyboard::RightBrace), },

    Entry { code: 0x080, action: Action::Simple(Keyboard::I), },
    Entry { code: 0x180, action: Action::Shifted(Keyboard::I), },
    Entry { code: 0x280, action: Action::Shifted(Keyboard::Keyboard0), },

    // Paired letters, shifted, and number/symbol.
    Entry { code: 0x0c0, action: Action::Simple(Keyboard::Y), },
    Entry { code: 0x1c0, action: Action::Shifted(Keyboard::Y), },
    Entry { code: 0x2c0, action: Action::Simple(Keyboard::Keyboard5), },

    Entry { code: 0x00c, action: Action::Simple(Keyboard::H), },
    Entry { code: 0x10c, action: Action::Shifted(Keyboard::H), },
    Entry { code: 0x20c, action: Action::Simple(Keyboard::Keyboard0), },

    Entry { code: 0x006, action: Action::Simple(Keyboard::U), },
    Entry { code: 0x106, action: Action::Shifted(Keyboard::U), },
    Entry { code: 0x206, action: Action::Simple(Keyboard::Keyboard2), },

    Entry { code: 0x009, action: Action::Simple(Keyboard::D), },
    Entry { code: 0x109, action: Action::Shifted(Keyboard::D), },
    Entry { code: 0x209, action: Action::Shifted(Keyboard::Keyboard2), },

    Entry { code: 0x0a0, action: Action::Simple(Keyboard::F), },
    Entry { code: 0x1a0, action: Action::Shifted(Keyboard::F), },
    Entry { code: 0x2a0, action: Action::Simple(Keyboard::Keyboard6), },

    Entry { code: 0x00a, action: Action::Simple(Keyboard::C), },
    Entry { code: 0x10a, action: Action::Shifted(Keyboard::C), },
    Entry { code: 0x20a, action: Action::Simple(Keyboard::Keyboard1), },

    Entry { code: 0x082, action: Action::Simple(Keyboard::K), },
    Entry { code: 0x182, action: Action::Shifted(Keyboard::K), },
    Entry { code: 0x282, action: Action::Shifted(Keyboard::Equal), },

    Entry { code: 0x041, action: Action::Simple(Keyboard::J), },
    Entry { code: 0x141, action: Action::Shifted(Keyboard::J), },
    Entry { code: 0x241, action: Action::Simple(Keyboard::Equal), },

    Entry { code: 0x081, action: Action::Simple(Keyboard::W), },
    Entry { code: 0x181, action: Action::Shifted(Keyboard::W), },
    Entry { code: 0x281, action: Action::Shifted(Keyboard::Keyboard7), },

    Entry { code: 0x030, action: Action::Simple(Keyboard::B), },
    Entry { code: 0x130, action: Action::Shifted(Keyboard::B), },
    Entry { code: 0x230, action: Action::Simple(Keyboard::Keyboard9), },

    Entry { code: 0x003, action: Action::Simple(Keyboard::L), },
    Entry { code: 0x103, action: Action::Shifted(Keyboard::L), },
    Entry { code: 0x203, action: Action::Simple(Keyboard::Keyboard4), },

    Entry { code: 0x060, action: Action::Simple(Keyboard::P), },
    Entry { code: 0x160, action: Action::Shifted(Keyboard::P), },
    Entry { code: 0x260, action: Action::Simple(Keyboard::Keyboard7), },

    Entry { code: 0x090, action: Action::Simple(Keyboard::G), },
    Entry { code: 0x190, action: Action::Shifted(Keyboard::G), },
    Entry { code: 0x290, action: Action::Shifted(Keyboard::Keyboard3), },

    Entry { code: 0x050, action: Action::Simple(Keyboard::Z), },
    Entry { code: 0x150, action: Action::Shifted(Keyboard::Z), },
    Entry { code: 0x250, action: Action::Simple(Keyboard::Keyboard8), },

    Entry { code: 0x005, action: Action::Simple(Keyboard::Q), },
    Entry { code: 0x105, action: Action::Shifted(Keyboard::Q), },
    Entry { code: 0x205, action: Action::Simple(Keyboard::Keyboard3), },

    Entry { code: 0x014, action: Action::Simple(Keyboard::X), },
    Entry { code: 0x114, action: Action::Shifted(Keyboard::X), },
    Entry { code: 0x214, action: Action::Shifted(Keyboard::Keyboard6), },

    Entry { code: 0x028, action: Action::Simple(Keyboard::V), },
    Entry { code: 0x128, action: Action::Shifted(Keyboard::V), },
    Entry { code: 0x228, action: Action::Shifted(Keyboard::Keyboard8), },

    Entry { code: 0x018, action: Action::Simple(Keyboard::M), },
    Entry { code: 0x118, action: Action::Shifted(Keyboard::M), },
    Entry { code: 0x218, action: Action::Shifted(Keyboard::Keyboard4), },

    // Punctuation only keys.
    Entry { code: 0x024, action: Action::Simple(Keyboard::ForwardSlash), },
    Entry { code: 0x124, action: Action::Simple(Keyboard::Backslash), },
    Entry { code: 0x224, action: Action::Shifted(Keyboard::Backslash), },

    Entry { code: 0x042, action: Action::Simple(Keyboard::Minus), },
    Entry { code: 0x142, action: Action::Shifted(Keyboard::Minus), },
    Entry { code: 0x242, action: Action::Shifted(Keyboard::Keyboard5), },

    Entry { code: 0x012, action: Action::Simple(Keyboard::Semicolon), },
    Entry { code: 0x112, action: Action::Shifted(Keyboard::Semicolon), },

    Entry { code: 0x084, action: Action::Shifted(Keyboard::ForwardSlash), },
    Entry { code: 0x184, action: Action::Shifted(Keyboard::Keyboard1), },

    Entry { code: 0x048, action: Action::Simple(Keyboard::Comma), },
    Entry { code: 0x148, action: Action::Simple(Keyboard::Dot), },
    Entry { code: 0x248, action: Action::Shifted(Keyboard::Grave), },

    // These aren't quite as per the chart, but the chart doesn't appear to be a
    // US layout.
    Entry { code: 0x021, action: Action::Simple(Keyboard::Apostrophe), },
    Entry { code: 0x121, action: Action::Shifted(Keyboard::Apostrophe), },
    Entry { code: 0x221, action: Action::Simple(Keyboard::Grave), },

    // The one shot keys.
    Entry { code: 0x088, action: Action::OneShot(Mods::SHIFT), },
    Entry { code: 0x188, action: Action::Simple(Keyboard::LeftArrow), },
    Entry { code: 0x288, action: Action::Simple(Keyboard::PageDown), },

    Entry { code: 0x011, action: Action::OneShot(Mods::GUI), },
    Entry { code: 0x111, action: Action::Simple(Keyboard::RightArrow), },
    Entry { code: 0x211, action: Action::Simple(Keyboard::PageUp), },

    Entry { code: 0x044, action: Action::OneShot(Mods::CONTROL), },
    Entry { code: 0x144, action: Action::Simple(Keyboard::DownArrow), },
    Entry { code: 0x244, action: Action::Simple(Keyboard::End), },

    Entry { code: 0x022, action: Action::OneShot(Mods::ALT), },
    Entry { code: 0x122, action: Action::Simple(Keyboard::UpArrow), },
    Entry { code: 0x222, action: Action::Simple(Keyboard::Home), },

    // Map the function keys to the numbers, but with both thumbs pressed. F11
    // is 'v' and F12 is 'w'.
    Entry { code: 0x30c, action: Action::Simple(Keyboard::F10), },
    Entry { code: 0x30a, action: Action::Simple(Keyboard::F1), },
    Entry { code: 0x306, action: Action::Simple(Keyboard::F2), },
    Entry { code: 0x305, action: Action::Simple(Keyboard::F3), },
    Entry { code: 0x303, action: Action::Simple(Keyboard::F4), },
    Entry { code: 0x3c0, action: Action::Simple(Keyboard::F5), },
    Entry { code: 0x3a0, action: Action::Simple(Keyboard::F6), },
    Entry { code: 0x360, action: Action::Simple(Keyboard::F7), },
    Entry { code: 0x350, action: Action::Simple(Keyboard::F8), },
    Entry { code: 0x330, action: Action::Simple(Keyboard::F9), },
    Entry { code: 0x328, action: Action::Simple(Keyboard::F11), },
    Entry { code: 0x381, action: Action::Simple(Keyboard::F12), },
];

#[cfg(test)]
mod tests {
    use super::TAIPO_ACTIONS;

    /// Every chord code appears at most once; a duplicate would silently
    /// shadow the later entry.
    #[test]
    fn test_codes_unique() {
        let mut codes: Vec<u16> = TAIPO_ACTIONS.iter().map(|e| e.code).collect();
        codes.sort();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count, "duplicate code in TAIPO_ACTIONS");
    }

    /// Every entry is reachable: an empty chord is never looked up.
    #[test]
    fn test_codes_nonempty() {
        for entry in TAIPO_ACTIONS {
            assert_ne!(entry.code, 0, "empty code in TAIPO_ACTIONS");
        }
    }
}
