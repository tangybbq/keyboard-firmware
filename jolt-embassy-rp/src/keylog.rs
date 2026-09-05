//! The key event log.
//!
//! `taipo-teacher.md` phase 2.  A ring buffer of four-byte records, fed from the two paths
//! that key events take into the layout, and drained by the minder handler.
//!
//! # What is recorded, and what is not
//!
//! Raw key events, and markers for the engine state a replay cannot derive from them.  The
//! chord each key belonged to, which hand it was on, what it typed, whether the chord was
//! dead or split -- none of that is here, because all of it is *derived* by replaying these
//! records through the real `TaipoManager` on the host.  The device does the cheap part.
//!
//! # Never at the expense of a keystroke
//!
//! [`log_key`] cannot block and cannot fail.  A full buffer drops its oldest *record*; the
//! key being typed is never delayed and never dropped.  That is why the buffer is a plain
//! array behind a critical section rather than a channel: there is no await point, so there
//! is no way for logging to get in front of typing.
//!
//! # Off at boot
//!
//! Logging starts disabled and stays that way until a host asks for it, so a keyboard with
//! no host attached accumulates nothing.  This is a privacy decision rather than a memory
//! one: the records are the letters.

use core::cell::RefCell;

use embassy_sync::blocking_mutex::{raw::CriticalSectionRawMutex, Mutex};
use embassy_time::Instant;
use minder::keylog::{Delta, Marker, Record, RECORD_SIZE};
use minder::Event;

use bbq_keyboard::layout::taipo::TaipoVariant;

use crate::minder::push_event;

/// How many records the buffer holds.
///
/// A chord is two records per key, so an average two-key chord is four records and a
/// character costs about 16 bytes.  8192 records is 32 KiB and roughly ten minutes of
/// solid typing, which is sized for a host that has briefly gone away rather than one that
/// was never there.  The RAM is available -- `memory.x` leaves a 200K region of which a
/// build uses about 11K outside the heap -- so this is a comfort choice, not a squeeze.
pub const CAPACITY: usize = 8192;

/// The shared log.
static LOG: Mutex<CriticalSectionRawMutex, RefCell<KeyLog>> =
    Mutex::new(RefCell::new(KeyLog::new()));

/// What a drain found.
pub struct Batch {
    /// How many records were copied out.
    ///
    /// Returned rather than asked for separately, because a key landing between the two
    /// calls would leave the caller slicing the buffer to a different length than was
    /// written into it.
    pub count: usize,
    pub seq: u32,
    pub dropped: u32,
    pub anchor_ms: u32,
    pub remaining: u32,
}

struct KeyLog {
    /// Records, oldest at `seq_base`.
    records: [[u8; RECORD_SIZE]; CAPACITY],
    /// How many records are held.
    len: usize,
    /// Index of the oldest record.
    head: usize,
    /// Sequence number of the oldest record held.
    seq_base: u32,
    /// Records dropped since the last drain reported it.
    dropped: u32,
    /// When the newest record was written.  `None` when empty.
    last: Option<Instant>,
    /// Whether keys are being recorded at all.
    enabled: bool,
    /// Raise `LogReady` at this many records; zero disables the notification.
    watermark: u32,
    /// Whether `LogReady` has already been raised for the current crossing.
    notified: bool,
    /// The last value seen for each state marker, tracked whether or not logging is on.
    ///
    /// This is what makes [`log_state`] possible.  While logging is off the markers are not
    /// recorded, but the engine still changes mode and chord table, so the state has to be
    /// followed anyway -- otherwise turning logging on would report whatever the state was
    /// when it was last turned off.
    state: [u8; STATE_MARKERS],
}

/// How many markers describe engine state rather than an event.
const STATE_MARKERS: usize = 3;

/// The `state` slot a marker occupies, if it describes state.
fn state_slot(marker: Marker) -> Option<usize> {
    Some(match marker {
        Marker::Mode => 0,
        Marker::Variant => 1,
        Marker::RowShift => 2,
        Marker::Resume | Marker::Pause => return None,
    })
}

impl KeyLog {
    const fn new() -> Self {
        Self {
            records: [[0; RECORD_SIZE]; CAPACITY],
            len: 0,
            head: 0,
            seq_base: 0,
            dropped: 0,
            last: None,
            enabled: false,
            watermark: 0,
            notified: false,
            // Not all zeros: slot 1 is the chord table, and the engine comes up in
            // `TaipoVariant::DEFAULT` rather than in whichever variant happens to encode
            // as zero.  A host joining the stream is told this state verbatim by
            // `log_state`, so a wrong value here is a whole session attributed to the
            // wrong table.
            state: [0, TaipoVariant::DEFAULT.marker(), 0],
        }
    }

    /// Append, dropping the oldest if there is no room.
    fn push(&mut self, entry: Record) {
        let bytes = entry.encode();
        if self.len == CAPACITY {
            // Drop the oldest.  Its sequence number is gone with it, which is what tells the
            // host there is a gap: the numbers it next sees start after the ones it lost.
            self.head = (self.head + 1) % CAPACITY;
            self.seq_base = self.seq_base.wrapping_add(1);
            self.len -= 1;
            self.dropped = self.dropped.saturating_add(1);
        }
        let tail = (self.head + self.len) % CAPACITY;
        self.records[tail] = bytes;
        self.len += 1;
    }

    /// The delta since the previous record, and a note of this one's time.
    fn tick(&mut self, now: Instant) -> Delta {
        let delta = match self.last {
            Some(prev) => Delta::from_millis(now.duration_since(prev).as_millis()),
            // The first record of a session has nothing to be relative to.
            None => Delta::from_millis(0),
        };
        self.last = Some(now);
        delta
    }
}

/// Turn logging on or off, and set the watermark.
///
/// A change either way writes a marker, so that a gap in the record stream can be told from
/// a quiet keyboard.
pub fn set_logging(enabled: bool, watermark: u32) {
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        let now = Instant::now();
        if enabled != log.enabled {
            let delta = log.tick(now);
            let marker = if enabled { Marker::Resume } else { Marker::Pause };
            // The Pause marker is written while still enabled, and the Resume after; either
            // way it lands inside the region it describes the edge of.
            log.push(Record::marker(delta, marker, 0));
        }
        log.enabled = enabled;
        log.watermark = watermark;
        log.notified = false;
    })
}

/// Record a key going down or coming up.
///
/// Never blocks, never fails, and never delays the key.  Silently does nothing when logging
/// is off.
pub fn log_key(code: u8, press: bool) {
    let now = Instant::now();
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        if !log.enabled {
            return;
        }
        let delta = log.tick(now);
        log.push(Record::key(delta, code, press));
    });
    notify();
}

/// Tell the host there is something to fetch, if it asked to be told.
///
/// Deliberately outside the log's lock: `push_event` takes the event queue's, and holding
/// both at once would be a lock ordering to have to remember.
fn notify() {
    if let Some(pending) = crossed_watermark() {
        push_event(Event::LogReady { pending });
    }
}

/// Record a change in engine state.
pub fn log_marker(marker: Marker, value: u8) {
    let now = Instant::now();
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        // Tracked even while logging is off, so that turning it on can report the state the
        // engine is actually in rather than the state it was in when logging stopped.
        if let Some(slot) = state_slot(marker) {
            log.state[slot] = value;
        }
        if !log.enabled {
            return;
        }
        let delta = log.tick(now);
        log.push(Record::marker(delta, marker, value));
    });
    notify();
}

/// How many records are waiting, and whether that has newly crossed the watermark.
///
/// The caller raises `Event::LogReady` when this returns true.  Crossing is reported once,
/// not once per record, so a busy keyboard does not turn the event queue into a second copy
/// of the log.
pub fn crossed_watermark() -> Option<u32> {
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        let pending = log.len as u32;
        if log.watermark == 0 || log.notified || pending < log.watermark {
            return None;
        }
        log.notified = true;
        Some(pending)
    })
}

/// Copy out up to `max_bytes` worth of records, without consuming them.
///
/// Returns what to report alongside them.  Nothing is discarded until [`ack`], so a host
/// that dies mid-transfer refetches rather than losing the batch.
pub fn drain(out: &mut [u8], max_bytes: usize) -> Batch {
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        let room = (max_bytes / RECORD_SIZE).min(out.len() / RECORD_SIZE);
        let count = room.min(log.len);

        for i in 0..count {
            let idx = (log.head + i) % CAPACITY;
            out[i * RECORD_SIZE..(i + 1) * RECORD_SIZE].copy_from_slice(&log.records[idx]);
        }

        // Measured from the newest record in *this batch*, which is what the host walks
        // backwards from.
        let anchor_ms = match (count, log.last) {
            (0, _) | (_, None) => 0,
            (_, Some(last)) if count == log.len => {
                Instant::now().duration_since(last).as_millis().min(u32::MAX as u64) as u32
            }
            // A partial batch does not end at `last`, and the device does not keep a
            // timestamp per record.  The host reconstructs the rest from the deltas of the
            // batch that follows, so anchoring a partial batch is neither possible nor
            // needed; say so with a sentinel rather than a plausible wrong number.
            _ => u32::MAX,
        };

        let batch = Batch {
            count,
            seq: log.seq_base,
            dropped: log.dropped,
            anchor_ms,
            remaining: (log.len - count) as u32,
        };
        log.dropped = 0;
        // Re-arm the notification once the host has seen the buffer.
        log.notified = false;
        batch
    })
}

/// Discard records up to and including `through_seq`.
///
/// Out-of-range acks are ignored rather than errored: a host that acks something already
/// dropped, or that has not arrived, is confused rather than dangerous, and the next drain
/// will tell it where things actually stand.
pub fn ack(through_seq: u32) {
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        let first = log.seq_base;
        if through_seq.wrapping_sub(first) >= log.len as u32 {
            // Either older than everything held, or newer than anything sent.
            if through_seq.wrapping_sub(first) < u32::MAX / 2 && log.len > 0 {
                // Newer than what is held: drop everything.
                log.head = 0;
                log.seq_base = log.seq_base.wrapping_add(log.len as u32);
                log.len = 0;
            }
            return;
        }
        let count = (through_seq.wrapping_sub(first) + 1) as usize;
        log.head = (log.head + count) % CAPACITY;
        log.len -= count;
        log.seq_base = log.seq_base.wrapping_add(count as u32);
    })
}

/// Write the engine's current state into the log, so that a stream a host joins partway
/// through is still self-describing.
///
/// Without this a replay starting mid-stream would not know which chord table to look
/// chords up in, and would attribute Dosh chords to the Taipo table with nothing to show
/// that it had.
pub fn log_state() {
    let now = Instant::now();
    LOG.lock(|log| {
        let mut log = log.borrow_mut();
        if !log.enabled {
            return;
        }
        for (slot, marker) in [
            (0usize, Marker::Mode),
            (1, Marker::Variant),
            (2, Marker::RowShift),
        ] {
            let value = log.state[slot];
            let delta = log.tick(now);
            log.push(Record::marker(delta, marker, value));
        }
    })
}
