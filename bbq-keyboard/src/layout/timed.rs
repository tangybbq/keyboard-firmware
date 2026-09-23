//! Driving the layout from a clock, rather than a fixed tick.
//!
//! The [`LayoutManager`] keeps time by being ticked: each [`tick`] says how
//! many milliseconds have passed, and its timers (the taipo chord window and
//! the qwerty combo wait) count those up.  Ticking it every millisecond works,
//! but it keeps the CPU waking a thousand times a second on a keyboard that is
//! doing nothing.
//!
//! Nothing the layout does between ticks depends on time: a key event is acted
//! on as it is delivered, and [`next_tick`] says when a timer will next need a
//! tick.  So a [`TimedLayout`] ticks the layout only when something is due, and
//! before each key event, passing all the time since the last tick in one go.
//! One large tick behaves the same as many small ones, because the timers
//! saturate and fire on `>=`.
//!
//! The catch-up before each event is what keeps a chord's age advancing while
//! no tick is due: without it, a key arriving long after the first key of a
//! chord would still join it.
//!
//! Time is passed in as milliseconds, so this has no clock of its own, and the
//! same code runs in the firmware and in host tests.
//!
//! Against a layout ticked every millisecond, as the replay does, the sequence
//! of outputs is the same.  The only difference is when a chord committed by
//! its timer alone is reported: the per-millisecond tick reports it a
//! millisecond sooner, because the tick in the millisecond of the press
//! already counts as one.
//!
//! [`tick`]: LayoutManager::tick
//! [`next_tick`]: LayoutManager::next_tick

use crate::KeyEvent;

use super::{LayoutActions, LayoutManager};

/// A [`LayoutManager`] driven by wall-clock time rather than a fixed tick.
///
/// The caller delivers key events through [`handle_event`], and calls
/// [`wake`] once at startup and then whenever [`deadline`] comes around.  The
/// deadline has to be asked again after every call, as any of them can move
/// it.  Waking early does no harm; waking late delays a chord's commit.
///
/// [`handle_event`]: Self::handle_event
/// [`wake`]: Self::wake
/// [`deadline`]: Self::deadline
pub struct TimedLayout {
    layout: LayoutManager,
    /// The millisecond the layout has been ticked up to.
    ticked_to: u64,
}

impl TimedLayout {
    /// Wrap a layout, taking `now` as the time it has been ticked up to.
    pub fn new(layout: LayoutManager, now: u64) -> Self {
        TimedLayout {
            layout,
            ticked_to: now,
        }
    }

    /// Catch the layout up to `now`, then deliver the event.
    pub async fn handle_event<ACT: LayoutActions>(
        &mut self,
        event: KeyEvent,
        now: u64,
        actions: &ACT,
    ) {
        self.catch_up(now, actions).await;
        self.layout.handle_event(event, actions).await;
    }

    /// Catch the layout up to `now`, firing any timer that is due.
    ///
    /// For the scheduler's timer, and for the first call at startup, which is
    /// when the layout announces its initial mode.
    pub async fn wake<ACT: LayoutActions>(&mut self, now: u64, actions: &ACT) {
        self.catch_up(now, actions).await;
    }

    /// The time of the next needed [`wake`](Self::wake), or `None` if nothing
    /// is pending.
    ///
    /// This may be in the past, if a wake is overdue.
    pub fn deadline(&self) -> Option<u64> {
        self.layout
            .next_tick()
            .map(|ticks| self.ticked_to + u64::from(ticks))
    }

    pub fn layout(&self) -> &LayoutManager {
        &self.layout
    }

    pub fn layout_mut(&mut self) -> &mut LayoutManager {
        &mut self.layout
    }

    /// Tick the layout for the time since it was last ticked.
    ///
    /// A clock that steps backwards counts as no time passing.  With no time
    /// passed, the tick is skipped unless one is due now, which is only the
    /// case before the initial mode has been announced.
    async fn catch_up<ACT: LayoutActions>(&mut self, now: u64, actions: &ACT) {
        let elapsed = now.saturating_sub(self.ticked_to);
        if elapsed > 0 || self.layout.next_tick() == Some(0) {
            // The timers count in `u32`; anything that long has long since
            // expired them all anyway.
            let ticks = elapsed.min(u64::from(u32::MAX)) as usize;
            self.layout.tick(actions, ticks).await;
        }
        self.ticked_to = now;
    }
}
