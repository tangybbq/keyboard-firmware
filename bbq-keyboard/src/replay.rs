//! Replay a key event log through the real layout engine.
//!
//! Phase 0 of `taipo-teacher.md`.  The device logs the cheap thing — a key
//! code, a press or a release, and a time — and everything interesting is
//! derived here by running the log through the same [`LayoutManager`] the
//! keyboard runs.  Which chord was assembled, on which hand, what it typed,
//! whether it typed anything at all, and how it ended are all things the
//! engine knows and the HID stream does not.
//!
//! Deriving rather than logging is the load-bearing decision: a firmware
//! change cannot silently invalidate the analysis, because the analysis is
//! the firmware.
//!
//! # Time
//!
//! The replay ticks the layout once per millisecond, exactly as
//! `jolt-embassy-rp`'s dispatch loop does, so the engine sees the same
//! quantization it sees on the keyboard.  Within a millisecond, key events are
//! delivered first and the tick runs after, so a chord finished by releasing
//! its last key at time `t` is reported at `t`.
//!
//! The log's times are milliseconds from the start of the *session*; nothing
//! here cares where the zero is.  The device's 20 ms debounce shifts every
//! press and release by the same amount, so intervals — which is all any of
//! this looks at — are unaffected.
//!
//! A log file generally holds more than one session: the collector appends to
//! one file per day and starts counting from zero each time it connects.
//! [`sessions_from_text`] is what splits those apart, and [`replay_sessions`]
//! gives each one its own engine.  Feeding a whole file to [`replay`] as a
//! single stream is a step backwards in time, which it refuses.
//!
//! # Key codes
//!
//! A logged key code is what `translate.rs` produced and what the row shift
//! has not yet been applied to.  That is the board-independent space
//! [`crate::layout::taipo::SCAN_MAP`] is indexed in, and the row shift is part
//! of the replay: [`LayoutManager`] applies it, so the row position, the mode
//! and the chord table all come out of the engine rather than being tracked
//! alongside it.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

use usbd_human_interface_device::page::Keyboard;

use crate::layout::export::{bit_for_name, name_for_code, BIT_NAMES};
use crate::layout::posh::POSH_ACTIONS;
use crate::layout::taipo::{Action, ChordEnd, TaipoVariant, SCAN_MAP, TAIPO_ACTIONS};
use crate::layout::{LayoutActions, LayoutManager, TAIPO_CHORD_TIME};
use crate::{KeyAction, KeyEvent, LayoutMode, MinorMode, Mods, Side};

/// How long the replay keeps ticking after the last logged event, so that a
/// chord still under the fingers when the log ends is committed rather than
/// lost.  One chord window plus a little.
const TAIL_MS: u32 = TAIPO_CHORD_TIME + 2;

//////////////////////////////////////////////////////////////////////////////
// The log
//////////////////////////////////////////////////////////////////////////////

/// One raw key event, as the device records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyLogEvent {
    /// Milliseconds since the start of the log.
    pub time_ms: u32,
    /// The key code, after `translate.rs` and before the row shift.
    pub key: u8,
    /// True for a press, false for a release.
    pub press: bool,
}

impl KeyLogEvent {
    /// One line of the text log format: the time, `+` or `-`, and the key.
    ///
    /// A key the scan map knows is named by hand and key, as `L.a` or `R.Sp`;
    /// anything else — the mode key, the two toggles, a position with no taipo
    /// meaning — is written as `k12`, since it still has to survive a round
    /// trip.  The names are the ones in `TAIPO.md`, which is what the drills
    /// and `layouts.json` use as well.
    ///
    /// The format is deliberately plain text: greppable, diffable, cheap to
    /// append, and readable in a test failure.  It is also what phase 2's
    /// on-disk log is meant to look like.
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {}",
            self.time_ms,
            if self.press { '+' } else { '-' },
            key_name(self.key),
        )
    }

    /// Parse one line written by [`to_line`](Self::to_line).
    ///
    /// Everything from a `#` is a comment, and a line with nothing left is not
    /// an event; those come back as `Ok(None)`.
    pub fn from_line(line: &str) -> Result<Option<KeyLogEvent>, String> {
        let line = match line.split_once('#') {
            Some((head, _)) => head,
            None => line,
        }
        .trim();
        if line.is_empty() {
            return Ok(None);
        }
        // A `=` line is a state marker written by `keyminder log`: the mode, chord
        // table or row position changing.  It is not a key event, so it is skipped
        // here; a consumer that needs the state reads it with [`marker_from_line`].
        // Skipping rather than erroring is what lets the two halves of the log file
        // share one parser.
        if marker_from_line(line).is_some() {
            return Ok(None);
        }
        let mut fields = line.split_whitespace();
        let (Some(time), Some(dir), Some(name), None) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return Err(format!("expected `<time> <+|-> <key>`, got {line:?}"));
        };
        let time_ms = time
            .parse::<u32>()
            .map_err(|e| format!("bad time {time:?}: {e}"))?;
        let press = match dir {
            "+" => true,
            "-" => false,
            other => return Err(format!("expected `+` or `-`, got {other:?}")),
        };
        let key = key_for_name(name).ok_or_else(|| format!("unknown key {name:?}"))?;
        Ok(Some(KeyLogEvent {
            time_ms,
            key,
            press,
        }))
    }
}

/// A whole log, as text, one event per line.
pub fn log_to_text(events: &[KeyLogEvent]) -> String {
    let mut out = String::new();
    for event in events {
        out.push_str(&event.to_line());
        out.push('\n');
    }
    out
}

/// A state marker line: `105018 = variant 1`.
///
/// Returns the offset, the marker's name, and its value.  The names are the ones
/// `keyminder log` writes: `mode`, `variant`, `row`, `resume`, `pause`.
///
/// The replay does not act on these yet -- selecting the chord table from a `variant`
/// marker is phase 3's job -- but they have to parse, or a log file containing them
/// cannot be read at all.
pub fn marker_from_line(line: &str) -> Option<(u32, &str, u8)> {
    let line = match line.split_once('#') {
        Some((head, _)) => head,
        None => line,
    }
    .trim();
    let mut fields = line.split_whitespace();
    let (Some(time), Some("="), Some(name), Some(value), None) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    ) else {
        return None;
    };
    Some((time.parse().ok()?, name, value.parse().ok()?))
}

/// Every state marker in a text log, in order.
pub fn markers_from_text(text: &str) -> Vec<(u32, &str, u8)> {
    text.lines().filter_map(marker_from_line).collect()
}

/// Parse a whole text log that has a single timeline.
///
/// Blank lines, comments and state markers are ignored.  A file holding more than one
/// session is an error rather than a flattened event list: its offsets restart with each
/// session, so the concatenation would step backwards in time.  Use
/// [`sessions_from_text`] for those.
pub fn log_from_text(text: &str) -> Result<Vec<KeyLogEvent>, String> {
    let mut sessions = sessions_from_text(text)?;
    if sessions.len() > 1 {
        return Err(format!(
            "log has {} sessions, each with its own timeline; use sessions_from_text",
            sessions.len()
        ));
    }
    Ok(match sessions.pop() {
        Some(session) => session.events,
        None => Vec::new(),
    })
}

/// One stretch of log with a single, continuous timeline.
///
/// A log file is appended to across connections and across runs of whatever is doing the
/// collecting, and each of those starts its own offset accumulator at zero.  A session is
/// the span between two such restarts: within one, times only go forwards; across one,
/// they mean nothing to each other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogSession {
    /// Unix seconds from the `# started` line, when the log carried one.
    ///
    /// Only good for ordering and labelling sessions.  It is when the collector connected,
    /// not when the events happened: the offsets inside the session start at its first
    /// record, which comes some unknown time later.
    pub started_unix: Option<u64>,
    /// The events, in non-decreasing time order.
    pub events: Vec<KeyLogEvent>,
}

/// Whether a comment line ends the timeline that precedes it.
///
/// All three are written by the collector, and all three mean the offsets that follow
/// count from a new zero: `# session` when it connects, `# scrubbed` when a retroactive
/// discard truncated the file, and `# device reset` when the keyboard rebooted and threw
/// away what it was buffering.
fn breaks_timeline(line: &str) -> bool {
    let line = line.trim();
    line.starts_with("# session")
        || line.starts_with("# scrubbed")
        || line.starts_with("# device reset")
}

/// The unix seconds from a `# started 1788121960 (unix seconds)` line.
fn started_from_line(line: &str) -> Option<u64> {
    let mut fields = line.trim().split_whitespace();
    match (fields.next(), fields.next()) {
        (Some("#"), Some("started")) => fields.next()?.parse().ok(),
        _ => None,
    }
}

/// Split a text log into its sessions and parse each one.
///
/// Sessions with no events are dropped: a header written just before the collector lost
/// the device says nothing about any typing.
///
/// A timeline also breaks wherever the times step backwards, header or no header.  That
/// is not merely tolerance for a malformed file: the offsets on either side of such a
/// step count from different zeros, so replaying across it would report intervals,
/// alternation pairs and corrections that never happened.
pub fn sessions_from_text(text: &str) -> Result<Vec<LogSession>, String> {
    let mut out: Vec<LogSession> = Vec::new();
    let mut current = LogSession {
        started_unix: None,
        events: Vec::new(),
    };
    for (num, line) in text.lines().enumerate() {
        if breaks_timeline(line) {
            out.push(current);
            current = LogSession {
                started_unix: None,
                events: Vec::new(),
            };
            continue;
        }
        if let Some(started) = started_from_line(line) {
            current.started_unix = Some(started);
            continue;
        }
        match KeyLogEvent::from_line(line) {
            Ok(Some(event)) => {
                // A step backwards is a restart the collector did not announce -- the
                // day-rollover case, where the file changed under a batch that had
                // already been given the old file's offsets.  The break is real
                // whether or not anything wrote a header for it, and splitting here
                // is what keeps the replay's monotonic assert meaning "corrupt within
                // a session" rather than "this log spans two of them".
                if current.events.last().is_some_and(|last| event.time_ms < last.time_ms) {
                    out.push(core::mem::replace(
                        &mut current,
                        LogSession {
                            started_unix: None,
                            events: Vec::new(),
                        },
                    ));
                }
                current.events.push(event)
            }
            Ok(None) => (),
            Err(e) => return Err(format!("line {}: {}", num + 1, e)),
        }
    }
    out.push(current);
    out.retain(|session| !session.events.is_empty());
    Ok(out)
}

/// The name of a key code: `L.a` and `R.Sp` for the taipo keys, `k12` for
/// anything else.
///
/// This names the key in the *upper* row position, which is the position the
/// log's key codes are recorded in.
pub fn key_name(key: u8) -> String {
    match SCAN_MAP.get(key as usize).copied().flatten() {
        Some((side, mask)) => format!(
            "{}.{}",
            if side.is_left() { 'L' } else { 'R' },
            BIT_NAMES[mask.trailing_zeros() as usize],
        ),
        None => format!("k{key}"),
    }
}

/// The key code a name from [`key_name`] refers to.
pub fn key_for_name(name: &str) -> Option<u8> {
    if let Some(number) = name.strip_prefix('k') {
        return number.parse().ok();
    }
    let (hand, key) = name.split_once('.')?;
    let side = match hand {
        "L" => Side::Left,
        "R" => Side::Right,
        _ => return None,
    };
    let mask = 1u16 << bit_for_name(key)?;
    (0..SCAN_MAP.len() as u8).find(|code| SCAN_MAP[*code as usize] == Some((side, mask)))
}

//////////////////////////////////////////////////////////////////////////////
// What comes out
//////////////////////////////////////////////////////////////////////////////

/// What a chord does, in a form that outlives the table it came from.
///
/// The same cases as [`Action`], which the tables hold by reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordAction {
    /// A single key.
    Key(Keyboard),
    /// A single key, with shift.
    Shifted(Keyboard),
    /// A short sequence of characters, one keypress each.
    Text(&'static str),
    /// A one-shot modifier.
    OneShot(Mods),
    /// The null chord, which releases held modifiers.
    Release,
    /// A chord that selects the chord table rather than typing anything.  The
    /// [`Chord`] carrying it names the variant the chord was *looked up* in,
    /// which is the one it is replacing.
    Variant(TaipoVariant),
}

impl ChordAction {
    fn of(action: &Action) -> ChordAction {
        match action {
            Action::Simple(k) => ChordAction::Key(*k),
            Action::Shifted(k) => ChordAction::Shifted(*k),
            Action::Text(t) => ChordAction::Text(t),
            Action::OneShot(m) => ChordAction::OneShot(*m),
            Action::Release => ChordAction::Release,
            Action::Variant(v) => ChordAction::Variant(*v),
        }
    }
}

/// A chord the engine committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    /// When the engine committed it.
    pub time_ms: u32,
    /// The hand it was built on.
    pub side: Side,
    /// The ten-bit chord code.
    pub code: u16,
    /// The table it was looked up in.
    pub variant: TaipoVariant,
    /// The mode the keyboard was in.  A chord assembled in steno mode is still
    /// reported, and generally typed nothing.
    pub mode: LayoutMode,
    /// When the first key of the chord went down.
    pub first_key_ms: u32,
    /// When the last key of the chord went down.  Equal to `first_key_ms` for
    /// a one-key chord, or for a chord whose keys all landed in the same
    /// millisecond.
    pub last_key_ms: u32,
    /// Why the chord stopped accumulating.
    pub end: ChordEnd,
    /// What the table says the chord does, or `None` for a chord the table has
    /// no entry for.
    pub action: Option<ChordAction>,
}

impl Chord {
    /// How long the chord took to assemble: nothing for a struck chord, tens
    /// of milliseconds for one that was rolled together finger by finger.
    pub fn spread_ms(&self) -> u32 {
        self.last_key_ms - self.first_key_ms
    }

    /// A chord the current table has no entry for.  It types nothing at all,
    /// which makes it a pure error signal, and one no host-side approach can
    /// see.
    pub fn is_dead(&self) -> bool {
        self.action.is_none()
    }

    /// The chord's keys, named as `TAIPO.md` names them.
    pub fn key_names(&self) -> String {
        name_for_code(self.code)
    }
}

/// Everything the replay derives, in the order it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Derived {
    /// A chord was committed.
    Chord(Chord),
    /// A HID report the layout asked for.
    Key { time_ms: u32, action: KeyAction },
    /// The held modifiers changed.  `sticky` is the subset of `oneshot` that
    /// survives a keypress.
    Mods {
        time_ms: u32,
        oneshot: Mods,
        sticky: Mods,
    },
    /// The layout mode changed.
    Mode { time_ms: u32, mode: LayoutMode },
    /// The chord table in use changed.
    Variant {
        time_ms: u32,
        variant: TaipoVariant,
    },
    /// The 2-row layouts moved between the row pairs of a 3-row board.
    RowPosition { time_ms: u32, lower: bool },
}

impl Derived {
    /// When it happened.
    pub fn time_ms(&self) -> u32 {
        match self {
            Derived::Chord(chord) => chord.time_ms,
            Derived::Key { time_ms, .. }
            | Derived::Mods { time_ms, .. }
            | Derived::Mode { time_ms, .. }
            | Derived::Variant { time_ms, .. }
            | Derived::RowPosition { time_ms, .. } => *time_ms,
        }
    }

    /// The chord, if this is one.
    pub fn as_chord(&self) -> Option<&Chord> {
        match self {
            Derived::Chord(chord) => Some(chord),
            _ => None,
        }
    }
}

/// Just the chords out of a derived stream.
pub fn chords(derived: &[Derived]) -> Vec<&Chord> {
    derived.iter().filter_map(Derived::as_chord).collect()
}

/// Chords that look like one chord that came apart.
///
/// A chord committed by the timer, followed on the same hand within
/// `window_ms` by another chord, with nothing on the other hand in between.
/// That is the `CHORD_TIME` window being hit by a hand that was still
/// assembling, and it is as much a signal about the constant as about the
/// writer.
///
/// Returns the index, within `chords`, of the first chord of each such pair.
/// The real classification — whether the two halves plausibly belong together
/// — is phase 3's; this is the mechanical part, and the part that needs the
/// engine.
pub fn split_chords(chords: &[&Chord], window_ms: u32) -> Vec<usize> {
    let mut out = Vec::new();
    for (num, pair) in chords.windows(2).enumerate() {
        let (first, second) = (pair[0], pair[1]);
        if first.end == ChordEnd::TimerExpired
            && second.side == first.side
            && second.first_key_ms >= first.time_ms
            && second.first_key_ms - first.time_ms <= window_ms
        {
            out.push(num);
        }
    }
    out
}

//////////////////////////////////////////////////////////////////////////////
// The replay
//////////////////////////////////////////////////////////////////////////////

/// A key event log, replayed through a real [`LayoutManager`].
///
/// Feed events in non-decreasing time order, then call [`finish`](Self::finish)
/// for the derived stream.  [`replay`] is the whole thing in one call.
pub struct Replay {
    layout: LayoutManager,
    recorder: Recorder,
    /// The next millisecond whose tick has not run yet.
    now: u32,
}

impl Replay {
    /// A replay of a board with the given row count, starting in taipo mode.
    ///
    /// `two_row` is the board's own `two_row` attribute: true for mesa1 and
    /// proto4, false for the jolts.  It decides whether the row toggle means
    /// anything and whether qwerty is in the mode cycle.
    pub fn new(two_row: bool) -> Replay {
        Replay::new_in_mode(two_row, LayoutMode::Taipo)
    }

    /// A replay starting in a particular mode.
    ///
    /// The mode is reached by tapping the mode key, using the layout's own
    /// cycle rather than a copy of it, before the log starts.  Nothing that
    /// happens during it is derived.
    pub fn new_in_mode(two_row: bool, mode: LayoutMode) -> Replay {
        let mut replay = Replay {
            layout: LayoutManager::new(two_row),
            recorder: Recorder::new(),
            now: 0,
        };
        // The layout announces its initial mode on the first tick.
        replay.tick();
        for _ in 0..8 {
            if replay.recorder.mode.borrow().eq(&mode) {
                break;
            }
            replay.deliver(KeyEvent::Press(crate::layout::MODE_KEY));
            replay.deliver(KeyEvent::Release(crate::layout::MODE_KEY));
            replay.tick();
        }
        assert_eq!(
            *replay.recorder.mode.borrow(),
            mode,
            "the mode cycle does not reach {mode:?}; is its layout feature enabled?"
        );
        replay.recorder.derived.borrow_mut().clear();
        replay.now = 0;
        replay.recorder.now.replace(0);
        replay
    }

    /// Feed one logged key event.
    ///
    /// Events must arrive in non-decreasing time order; a step backwards is a
    /// corrupt log and panics rather than producing quietly wrong timings.
    pub fn feed(&mut self, event: KeyLogEvent) {
        assert!(
            event.time_ms >= self.now,
            "log went backwards in time: {} after {}",
            event.time_ms,
            self.now
        );
        // Run the ticks for every millisecond up to this one; the event is
        // delivered before its own millisecond's tick, so a chord finished by
        // this event is reported at this time.
        while self.now < event.time_ms {
            self.tick();
        }
        self.recorder.note_key(event);
        self.deliver(if event.press {
            KeyEvent::Press(event.key)
        } else {
            KeyEvent::Release(event.key)
        });
    }

    /// Run out the clock and take the derived stream.
    ///
    /// The clock runs on for one chord window past the last event, so a chord
    /// still held when the log ended is committed rather than dropped.
    pub fn finish(mut self) -> Vec<Derived> {
        let end = self.now + TAIL_MS;
        while self.now <= end {
            self.tick();
        }
        self.recorder.derived.take()
    }

    /// Run the tick for the current millisecond, and move on.
    fn tick(&mut self) {
        self.recorder.now.replace(self.now);
        now_or_never(self.layout.tick(&self.recorder, 1));
        self.now += 1;
    }

    fn deliver(&mut self, event: KeyEvent) {
        self.recorder.now.replace(self.now);
        now_or_never(self.layout.handle_event(event, &self.recorder));
    }
}

/// Replay a whole log in one call.
pub fn replay(two_row: bool, events: &[KeyLogEvent]) -> Vec<Derived> {
    let mut replay = Replay::new(two_row);
    for event in events {
        replay.feed(*event);
    }
    replay.finish()
}

/// Replay each session of a log, one derived stream per session.
///
/// Every session gets its own [`Replay`], which is the whole point: the timeline restarts
/// at a session boundary, and so does the engine.  A chord still under the fingers when the
/// collector disconnected is committed at the end of its own session rather than merging
/// into the first chord of the next one, and no interval spans the boundary.
pub fn replay_sessions(two_row: bool, sessions: &[LogSession]) -> Vec<Vec<Derived>> {
    sessions
        .iter()
        .map(|session| replay(two_row, &session.events))
        .collect()
}

//////////////////////////////////////////////////////////////////////////////
// The recorder
//////////////////////////////////////////////////////////////////////////////

/// The [`LayoutActions`] implementation the replay listens through.
///
/// Everything is behind a `RefCell` because `LayoutActions` deliberately takes
/// `&self`: on the keyboard the handler is shared, and protects its own data.
struct Recorder {
    derived: RefCell<Vec<Derived>>,
    now: RefCell<u32>,
    mode: RefCell<LayoutMode>,
    variant: RefCell<TaipoVariant>,
    /// Whether the 2-row layouts are on the lower pair of rows.  Needed to
    /// resolve a logged key code to a chord bit, since the log records the
    /// code before the shift.
    lower: RefCell<bool>,
    /// When each chord bit last went down, per side.  A chord's keys are all
    /// pressed after the chord started, so the times of its bits are its own.
    pressed_ms: RefCell<[[u32; 10]; 2]>,
}

impl Recorder {
    fn new() -> Recorder {
        Recorder {
            derived: RefCell::new(Vec::new()),
            now: RefCell::new(0),
            mode: RefCell::new(LayoutMode::default()),
            variant: RefCell::new(TaipoVariant::default()),
            lower: RefCell::new(false),
            pressed_ms: RefCell::new([[0; 10]; 2]),
        }
    }

    /// Note when a key went down, for the chord timings.
    fn note_key(&self, event: KeyLogEvent) {
        if !event.press {
            return;
        }
        let key = if *self.lower.borrow() {
            row_shift(event.key)
        } else {
            event.key
        };
        if let Some(Some((side, mask))) = SCAN_MAP.get(key as usize) {
            self.pressed_ms.borrow_mut()[side.index()][mask.trailing_zeros() as usize] =
                event.time_ms;
        }
    }

    fn push(&self, event: Derived) {
        self.derived.borrow_mut().push(event);
    }
}

impl LayoutActions for Recorder {
    async fn set_mode(&self, mode: LayoutMode) {
        *self.mode.borrow_mut() = mode;
        self.push(Derived::Mode {
            time_ms: *self.now.borrow(),
            mode,
        });
    }

    async fn set_mode_select(&self, _mode: LayoutMode) {
        // The mode key being held is not interesting to a replay; the mode
        // change it settles on is, and that arrives through `set_mode`.
    }

    async fn send_key(&self, key: KeyAction) {
        self.push(Derived::Key {
            time_ms: *self.now.borrow(),
            action: key,
        });
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Posh => self.set_variant(TaipoVariant::Posh),
        }
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Posh => self.set_variant(TaipoVariant::Taipo),
        }
    }

    #[cfg(feature = "steno")]
    async fn send_raw_steno(&self, _stroke: bbq_steno::Stroke) {
        // Steno is not what this is for.  The chords are still reported, and
        // carry the mode, so a run that strayed into steno is visible.
    }

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        self.push(Derived::Mods {
            time_ms: *self.now.borrow(),
            oneshot,
            sticky,
        });
    }

    async fn taipo_chord(&self, side: Side, code: u16, end: ChordEnd) {
        let time_ms = *self.now.borrow();
        let variant = *self.variant.borrow();
        let pressed = self.pressed_ms.borrow()[side.index()];
        let (mut first, mut last) = (u32::MAX, 0);
        for bit in 0..10 {
            if code & (1 << bit) != 0 {
                first = first.min(pressed[bit]);
                last = last.max(pressed[bit]);
            }
        }
        let table = match variant {
            TaipoVariant::Taipo => TAIPO_ACTIONS,
            TaipoVariant::Posh => POSH_ACTIONS,
        };
        let action = table
            .iter()
            .find(|entry| entry.code == code)
            .map(|entry| ChordAction::of(&entry.action));
        self.push(Derived::Chord(Chord {
            time_ms,
            side,
            code,
            variant,
            mode: *self.mode.borrow(),
            first_key_ms: if first == u32::MAX { time_ms } else { first },
            last_key_ms: if first == u32::MAX { time_ms } else { last },
            end,
            action,
        }));
    }

    async fn set_row_position(&self, lower: bool) {
        *self.lower.borrow_mut() = lower;
        self.push(Derived::RowPosition {
            time_ms: *self.now.borrow(),
            lower,
        });
    }
}

impl Recorder {
    fn set_variant(&self, variant: TaipoVariant) {
        *self.variant.borrow_mut() = variant;
        self.push(Derived::Variant {
            time_ms: *self.now.borrow(),
            variant,
        });
    }
}

/// The row shift, for builds that have one.
#[cfg(feature = "proto3")]
fn row_shift(key: u8) -> u8 {
    crate::layout::lower_row_remap(key)
}

#[cfg(not(feature = "proto3"))]
fn row_shift(key: u8) -> u8 {
    key
}

//////////////////////////////////////////////////////////////////////////////
// Driving the async engine
//////////////////////////////////////////////////////////////////////////////

/// Run a future to completion, which for these futures means polling it once.
///
/// The layout engine's async only exists so that the firmware's handler can
/// await a channel.  [`Recorder`] never does, and the engine awaits nothing
/// else, so a `Pending` here would mean the recorder had grown something that
/// blocks — which would make the replay's timings wrong rather than slow, so
/// it is better as a panic than as a spin.
fn now_or_never<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the replay's LayoutActions must never block"),
    }
}

//////////////////////////////////////////////////////////////////////////////
// Formatting, for golden files and test failures
//////////////////////////////////////////////////////////////////////////////

impl Chord {
    /// A one-line description, for a golden file or a test failure.
    pub fn to_line(&self) -> String {
        format!(
            "{} {} {} {:#05x} [{}] {} spread={} {}",
            self.time_ms,
            match self.side {
                Side::Left => "L",
                Side::Right => "R",
            },
            match self.variant {
                TaipoVariant::Taipo => "taipo",
                TaipoVariant::Posh => "posh",
            },
            self.code,
            self.key_names(),
            match self.end {
                ChordEnd::AllReleased => "released",
                ChordEnd::TimerExpired => "timer",
                ChordEnd::OtherHand => "other-hand",
            },
            self.spread_ms(),
            match &self.action {
                None => "dead".to_string(),
                Some(ChordAction::Key(k)) => format!("key {k:?}"),
                Some(ChordAction::Shifted(k)) => format!("shifted {k:?}"),
                Some(ChordAction::Text(t)) => format!("text {t:?}"),
                Some(ChordAction::OneShot(m)) => format!("oneshot {m:?}"),
                Some(ChordAction::Release) => "release".to_string(),
                Some(ChordAction::Variant(v)) => format!("variant {v:?}"),
            },
        )
    }
}

impl Derived {
    /// A one-line description, for a golden file or a test failure.
    pub fn to_line(&self) -> String {
        match self {
            Derived::Chord(chord) => format!("chord {}", chord.to_line()),
            Derived::Key { time_ms, action } => format!("key {time_ms} {action:?}"),
            Derived::Mods {
                time_ms,
                oneshot,
                sticky,
            } => format!("mods {time_ms} oneshot={oneshot:?} sticky={sticky:?}"),
            Derived::Mode { time_ms, mode } => format!("mode {time_ms} {mode:?}"),
            Derived::Variant { time_ms, variant } => format!("variant {time_ms} {variant:?}"),
            Derived::RowPosition { time_ms, lower } => format!("rows {time_ms} lower={lower}"),
        }
    }
}

/// A whole derived stream, one event per line.
pub fn derived_to_text(derived: &[Derived]) -> String {
    let mut out = String::new();
    for event in derived {
        out.push_str(&event.to_line());
        out.push('\n');
    }
    out
}
