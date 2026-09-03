//! Turning a stream of chords into the things worth knowing about it.
//!
//! The separation from `report` is deliberate: everything here is a number, and the Mac app
//! in phase 4 wants the numbers rather than the text.

use std::collections::HashMap;

use bbq_keyboard::layout::taipo::{ChordEnd, TaipoVariant};
use bbq_keyboard::replay::{Chord, ChordAction};
use bbq_keyboard::{Keyboard, LayoutMode, Side};

use crate::Options;

/// An item the model is keyed on.
///
/// Single chords *and* ordered pairs, because the trouble is often in the sequence rather
/// than the chord: a chord that is easy after one neighbour can be awkward after another,
/// and averaging those together hides exactly the thing worth drilling.  Keyed by variant,
/// since Taipo and Dosh are different skills that happen to share an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Item {
    Chord {
        variant: VariantKey,
        code: u16,
    },
    Transition {
        variant: VariantKey,
        from: u16,
        to: u16,
    },
}

/// `TaipoVariant` without the trait bounds it lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum VariantKey {
    Taipo,
    Dosh,
}

impl VariantKey {
    pub fn of(v: TaipoVariant) -> VariantKey {
        match v {
            TaipoVariant::Taipo => VariantKey::Taipo,
            TaipoVariant::Dosh => VariantKey::Dosh,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VariantKey::Taipo => "taipo",
            VariantKey::Dosh => "dosh",
        }
    }
}

/// Running statistics for one item.
#[derive(Debug, Default, Clone)]
pub struct ItemStats {
    pub count: usize,
    /// Intervals from the previous chord's last key to this chord's first key, each with
    /// the time it happened, so that a slow one can be pointed at in the log.
    pub intervals: Vec<Sample>,
    /// How many times a correction followed within a few chords.
    pub corrections: usize,
    /// What those corrections cost in total, measured rather than priced.
    pub correction_ms: u64,
    /// How many times this was a same-hand pair inside the alternation window.
    pub same_hand: usize,
}

/// A point in the log: which session, and how far into that session.
///
/// A log file has no single timeline -- each session's offsets count from its own zero --
/// so a bare millisecond does not identify anything.  This is what a time in the analysis
/// has to be for the reader to find it again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    /// Which session of the log, counting from zero.
    pub session: usize,
    /// Milliseconds into that session.
    pub time_ms: u32,
}

/// One observation of an interval.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub gap_ms: u32,
    pub at: At,
}

impl ItemStats {
    /// The typical interval, as a median rather than a mean: one long pause while thinking
    /// should not make a whole item look slow.
    pub fn median(&self) -> Option<u32> {
        if self.intervals.is_empty() {
            return None;
        }
        let mut v: Vec<u32> = self.intervals.iter().map(|s| s.gap_ms).collect();
        v.sort_unstable();
        Some(v[v.len() / 2])
    }
}

/// How a correction relates to what it replaced.
///
/// The bit patterns make these mechanically distinguishable, and they want different
/// answers: a misfingering is technique, a different chord entirely is vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionKind {
    /// The retyped chord is the same one.  The chord was right; something else was wrong,
    /// or it was sent twice.
    SameChord,
    /// One finger different: an extra key, a missing key, or a substitution.
    OneKeyOff,
    /// A different chord entirely.  Usually means the right one was not recalled.
    Different,
    /// Nothing was retyped: text was deleted and not replaced.
    NoReplacement,
}

/// What each of the four columns and two thumbs was asked to do by a chord.
///
/// `0` nothing, `1` the bottom key, `2` the top key, `3` both.  Working per finger rather
/// than per bit is what makes the shape of a mistake legible: two chords two bits apart
/// can be one finger on the wrong row or two fingers doing unrelated things, and the bit
/// count cannot tell those apart.
fn finger_states(code: u16) -> [u8; 6] {
    let mut s = [0u8; 6];
    for col in 0..4 {
        s[col] = (code >> col & 1) as u8 | ((code >> (col + 4) & 1) as u8) << 1;
    }
    s[4] = (code >> 8 & 1) as u8;
    s[5] = (code >> 9 & 1) as u8;
    s
}

/// How a mistyped chord differed in shape from the one that replaced it.
///
/// `CorrectionKind` says how *far* the mistake was -- one key, or a different chord
/// entirely.  This says what the hand actually did, which is the part a drill can be built
/// from: the corpus turns out to be dominated by two shapes, and they want different
/// practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confusion {
    /// The right fingers, one or more of them on the wrong row.
    WrongRow,
    /// The right rows, a key struck by the wrong finger.
    WrongFinger,
    /// The right chord with a key missing.
    DroppedKey,
    /// The right chord with an extra key.
    AddedKey,
    /// The right finger keys under the wrong thumb, so the wrong layer.
    WrongLayer,
    /// Nothing systematic.  More likely the wrong chord recalled than a misfingering.
    Unrelated,
}

impl Confusion {
    pub fn name(self) -> &'static str {
        match self {
            Confusion::WrongRow => "wrong row",
            Confusion::WrongFinger => "wrong finger",
            Confusion::DroppedKey => "key dropped",
            Confusion::AddedKey => "key added",
            Confusion::WrongLayer => "wrong layer",
            Confusion::Unrelated => "unrelated",
        }
    }

    /// Classify the chord that was typed against the one that replaced it.
    ///
    /// The order of the tests is the classification: each is narrower than the next and
    /// the first that fits wins.  A chord differing on one finger by row *and* on another
    /// by finger has no single name, and falls through to `Unrelated` rather than being
    /// filed under whichever test happened to run first.
    pub fn of(typed: u16, meant: u16) -> Confusion {
        if typed == meant {
            return Confusion::Unrelated;
        }
        let (t, m) = (finger_states(typed), finger_states(meant));
        let differ: Vec<usize> = (0..6).filter(|&i| t[i] != m[i]).collect();
        let thumbs = differ.iter().any(|&i| i >= 4);
        let fingers = differ.iter().any(|&i| i < 4);

        if thumbs {
            // A thumb selects the layer, so a thumb difference is a different chord rather
            // than a misfingering -- unless it is the only difference.
            return if fingers {
                Confusion::Unrelated
            } else {
                Confusion::WrongLayer
            };
        }

        // Every finger that differs is on the other row: the hand had the right shape and
        // put it in the wrong place.
        if differ
            .iter()
            .all(|&i| (t[i] == 1 && m[i] == 2) || (t[i] == 2 && m[i] == 1))
        {
            return Confusion::WrongRow;
        }

        // The same number of keys on each row, so the rows were right and the keys are
        // simply under the wrong fingers.  Nothing narrower than this works: the whole
        // shape can shift a column over, which leaves a finger holding a key in both
        // chords -- a different key, but a per-finger "one of these is empty" test reads
        // that as unrelated and it is the plainest finger slip there is.
        let row_counts = |c: u16| ((c & 0x0f).count_ones(), (c & 0xf0).count_ones());
        if row_counts(typed) == row_counts(meant) {
            return Confusion::WrongFinger;
        }

        if typed & meant == typed {
            return Confusion::DroppedKey;
        }
        if typed & meant == meant {
            return Confusion::AddedKey;
        }
        Confusion::Unrelated
    }
}

#[derive(Debug, Clone)]
pub struct Correction {
    pub deleted: Option<u16>,
    pub replacement: Option<u16>,
    pub kind: CorrectionKind,
    /// What it cost, from the mistyped chord being committed to its replacement being
    /// committed: noticing, the backspaces, and the retype.
    ///
    /// Measured rather than priced at some number of chords, since the noticing is most
    /// of it and no constant would have that in it.  `None` when there is nothing to
    /// measure between -- nothing was retyped, or the log does not reach back to what was
    /// deleted.
    pub cost_ms: Option<u32>,
    /// The shape of the mistake, when both ends are known and they differ.
    pub confusion: Option<Confusion>,
}

#[derive(Debug, Clone)]
pub struct Hesitation {
    pub at: At,
    pub item: Item,
    pub interval_ms: u32,
    pub typical_ms: u32,
}

/// One item, with what it cost the writer.
///
/// The currency is milliseconds, which is what makes the two halves comparable: time spent
/// above this writer's own pace, plus time spent undoing what went wrong.  Both are
/// measured from the log; neither is a weight anyone chose.
#[derive(Debug, Clone, Copy)]
pub struct Ranked {
    pub item: Item,
    /// How many times it occurred at all.
    pub exposure: usize,
    /// How many of those had an interval that was typing rather than a pause.
    pub samples: usize,
    /// Its own typical interval.
    pub median_ms: u32,
    /// Total time above the writer's baseline pace.
    pub slow_ms: u64,
    pub corrections: usize,
    /// Total measured time spent correcting it.
    pub correction_ms: u64,
    /// Same-hand pairs, reported rather than charged: alternation is a metric while
    /// monitoring and an error only while drilling.
    pub same_hand: usize,
    /// `slow_ms + correction_ms`, which is what the list is ordered by.
    pub cost_ms: u64,
}

/// A same-hand pair that was close enough together to be avoidable.
#[derive(Debug, Clone)]
pub struct SameHandRun {
    pub side: Side,
    pub from: u16,
    pub to: u16,
    pub gap_ms: u32,
}

#[derive(Debug, Default)]
pub struct Analysis {
    /// How many sessions the log held.  More than one means the file was appended to
    /// across collector runs, and nothing is measured across the joins.
    pub sessions: usize,
    /// The fingerprint of the tables everything here was replayed through.
    pub layout: u64,
    /// Fingerprints found in the log that are not that one, in the order they sort.
    pub foreign_layouts: Vec<u64>,
    /// How many sessions carried one of those.
    pub foreign_sessions: usize,
    /// How many sessions had no `layout=` at all, which is firmware too old to report one.
    pub unstamped_sessions: usize,
    pub total_chords: usize,
    /// Chords assembled while not in taipo mode.  Counted, then excluded from everything
    /// else: they were never taipo typing and would only add noise.
    pub non_taipo: usize,
    pub span_ms: u32,

    pub ended_released: usize,
    pub ended_timer: usize,
    pub ended_other_hand: usize,

    pub spreads: Vec<u32>,
    pub dead: Vec<(At, u16, VariantKey)>,
    pub split: Vec<(At, u16)>,

    pub eligible_pairs: usize,
    pub same_hand: Vec<SameHandRun>,
    /// Consecutive chords too far apart to be typing, so no interval was recorded for
    /// them.  Reported, because a corpus that is mostly this is a corpus about something
    /// other than typing.
    pub idle_pairs: usize,
    /// The ceiling that was used, so the methods here can tell a typing gap from a pause
    /// without being handed the options again.
    pub idle_ms: u32,
    /// Every recorded gap, for the distribution.  What decides `alternation_window_ms` and
    /// `idle_ms` is the shape of this, and the shape is worth showing rather than
    /// summarising into the two constants it justifies.
    pub gaps: Vec<u32>,

    pub corrections: Vec<Correction>,
    pub hesitations: Vec<Hesitation>,

    pub items: HashMap<Item, ItemStats>,
    pub variant_chords: HashMap<VariantKey, usize>,
    pub variant_switches: usize,
    pub spelled: Vec<SpelledGram>,
}

/// Whether a chord types a printable character, in the table it was looked up in.
///
/// What separates typing from working the machine.  A one-shot Cmd, a Return, an arrow
/// key: nobody drills those, and the pause in front of one is a decision about which
/// command to run rather than anything about the chord.  Left in the ranking they take
/// the top of it, because deciding what to do takes far longer than typing does.
///
/// Space counts.  It is a character, it is a third of the transitions, and pausing at a
/// word boundary is a fact about this writer's typing worth seeing.
pub fn types_a_character(variant: VariantKey, code: u16) -> bool {
    let table = match variant {
        VariantKey::Taipo => bbq_keyboard::layout::taipo::TAIPO_ACTIONS,
        VariantKey::Dosh => bbq_keyboard::layout::dosh::DOSH_ACTIONS,
    };
    let Some(entry) = table.iter().find(|e| e.code == code) else {
        return false;
    };
    use bbq_keyboard::layout::export::char_for_key;
    use bbq_keyboard::layout::taipo::Action;
    match &entry.action {
        Action::Simple(k) => char_for_key(*k, false).is_some(),
        Action::Shifted(k) => char_for_key(*k, true).is_some(),
        Action::Text(_) => true,
        Action::OneShot(_) | Action::Release | Action::Variant(_) => false,
    }
}

/// Whether a chord's action is a backspace, which is what a correction looks like.
fn is_backspace(chord: &Chord) -> bool {
    matches!(
        chord.action,
        Some(ChordAction::Key(Keyboard::DeleteBackspace))
    )
}

/// Whether a chord types something, as opposed to being a modifier or a backspace.
fn is_text(chord: &Chord) -> bool {
    matches!(
        chord.action,
        Some(ChordAction::Key(_) | ChordAction::Shifted(_) | ChordAction::Text(_))
    ) && !is_backspace(chord)
}

/// Whether two chord codes differ by exactly one finger.
///
/// Three cases, and the third is the one that is easy to miss: a key added or a key
/// removed flips one bit, but a key *substituted* for its neighbour flips two -- one off,
/// one on -- and is still a single misplaced finger.  Requiring the popcounts to match is
/// what separates that from two unrelated chords that happen to be two bits apart.
fn one_key_off(a: u16, b: u16) -> bool {
    let diff = (a ^ b).count_ones();
    diff == 1 || (diff == 2 && a.count_ones() == b.count_ones())
}

fn classify(deleted: Option<u16>, replacement: Option<u16>) -> CorrectionKind {
    match (deleted, replacement) {
        (_, None) => CorrectionKind::NoReplacement,
        (None, _) => CorrectionKind::Different,
        (Some(a), Some(b)) if a == b => CorrectionKind::SameChord,
        (Some(a), Some(b)) if one_key_off(a, b) => CorrectionKind::OneKeyOff,
        _ => CorrectionKind::Different,
    }
}

impl Analysis {
    /// Analyse a log, one chord stream per session.
    ///
    /// Sessions rather than one stream because a log file holds several timelines: the
    /// offsets restart whenever the collector reconnects.  Nothing that needs two chords --
    /// an interval, an alternation pair, a correction, a spelled gram -- is ever computed
    /// across a boundary, since the two chords either side of one may be minutes or hours
    /// apart and their times cannot even be subtracted.
    pub fn build(sessions: &[&[&Chord]], opts: &Options) -> Analysis {
        let mut a = Analysis::default();
        a.sessions = sessions.len();
        a.idle_ms = opts.idle_ms;
        for (num, chords) in sessions.iter().enumerate() {
            a.add_session(num, chords, opts);
        }
        // Wants every session's samples, since a transition's typical interval is a fact
        // about the writer rather than about one sitting.
        a.find_hesitations(opts);
        a
    }

    /// Fold one session's chords into the analysis.
    fn add_session(&mut self, session: usize, chords: &[&Chord], opts: &Options) {
        let a = self;
        a.total_chords += chords.len();

        // Summed per session, so the hours between sittings are not counted as time spent
        // typing.  It is the denominator of the chords-per-minute figure.
        if let (Some(first), Some(last)) = (chords.first(), chords.last()) {
            a.span_ms += last.time_ms.saturating_sub(first.first_key_ms);
        }

        // Taipo-mode chords only.  A chord assembled in steno or qwerty mode was still
        // assembled, but it is not taipo typing and would only blur every average.
        let taipo: Vec<&Chord> = chords
            .iter()
            .copied()
            .filter(|c| {
                let keep = c.mode == LayoutMode::Taipo;
                keep
            })
            .collect();
        a.non_taipo += chords.len() - taipo.len();

        // Starts as `None` in each session: the table in use at the end of one sitting
        // says nothing about the one the next begins with.
        let mut last_variant: Option<VariantKey> = None;
        for chord in &taipo {
            let v = VariantKey::of(chord.variant);
            *a.variant_chords.entry(v).or_default() += 1;
            if last_variant.is_some_and(|prev| prev != v) {
                a.variant_switches += 1;
            }
            last_variant = Some(v);

            match chord.end {
                ChordEnd::AllReleased => a.ended_released += 1,
                ChordEnd::TimerExpired => a.ended_timer += 1,
                ChordEnd::OtherHand => a.ended_other_hand += 1,
            }
            a.spreads.push(chord.spread_ms());

            if chord.is_dead() {
                a.dead.push((
                    At {
                        session,
                        time_ms: chord.time_ms,
                    },
                    chord.code,
                    v,
                ));
            }
        }

        // Split chords: a timer-ended chord followed on the same hand almost immediately.
        // The replay does the mechanical part; what it means is a judgement this reports
        // rather than makes.
        for idx in bbq_keyboard::replay::split_chords(&taipo, opts.alternation_window_ms.min(300)) {
            a.split.push((
                At {
                    session,
                    time_ms: taipo[idx].time_ms,
                },
                taipo[idx].code,
            ));
        }

        a.walk_pairs(session, &taipo, opts);
        a.find_corrections(&taipo, opts);
        a.spelled.extend(find_spelled_grams(&taipo));
    }

    /// Everything that needs two consecutive chords: intervals, transitions, alternation.
    fn walk_pairs(&mut self, session: usize, taipo: &[&Chord], opts: &Options) {
        for pair in taipo.windows(2) {
            let (prev, next) = (pair[0], pair[1]);
            let v = VariantKey::of(next.variant);
            // A variant switch mid-stream makes the pair meaningless as a transition.
            if VariantKey::of(prev.variant) != v {
                continue;
            }

            // The gap is idle time: from the last key of one chord going down to the first
            // key of the next.  Rollover can make that negative, which is zero here.
            let gap = next.first_key_ms.saturating_sub(prev.last_key_ms);

            self.gaps.push(gap);
            // A gap past the ceiling is the writer having stopped.  The pair still
            // happened, so it counts as an exposure, but its interval measures an
            // absence and is not recorded: an interval kept here would be averaged into
            // the transition's typical time and then reported as a hesitation against
            // the baseline it had just raised.
            let typing = gap < opts.idle_ms;
            if !typing {
                self.idle_pairs += 1;
            }
            let sample = Sample {
                gap_ms: gap,
                at: At {
                    session,
                    time_ms: next.first_key_ms,
                },
            };

            let item = Item::Transition {
                variant: v,
                from: prev.code,
                to: next.code,
            };
            let entry = self.items.entry(item).or_default();
            entry.count += 1;
            if typing {
                entry.intervals.push(sample);
            }

            let chord_item = Item::Chord {
                variant: v,
                code: next.code,
            };
            let centry = self.items.entry(chord_item).or_default();
            centry.count += 1;
            if typing {
                centry.intervals.push(sample);
            }

            // Alternation only means anything inside a burst.  After a pause the fingers
            // are back at rest and either hand is equally correct, so a pair that far apart
            // is not counted -- and is not in the denominator either, or a session with a
            // lot of thinking time would score better than one without.
            if gap < opts.alternation_window_ms {
                self.eligible_pairs += 1;
                if prev.side == next.side {
                    self.same_hand.push(SameHandRun {
                        side: next.side,
                        from: prev.code,
                        to: next.code,
                        gap_ms: gap,
                    });
                    self.items.entry(item).or_default().same_hand += 1;
                }
            }
        }
    }

    /// A backspace, what it deleted, and what replaced it.
    fn find_corrections(&mut self, taipo: &[&Chord], opts: &Options) {
        for (i, chord) in taipo.iter().enumerate() {
            if !is_backspace(chord) {
                continue;
            }
            // What it deleted: the nearest preceding chord that typed something.
            let deleted_at = taipo[..i].iter().rposition(|c| is_text(c));
            let deleted = deleted_at.map(|j| taipo[j].code);
            // What replaced it: the next chord that types, as long as it is not another
            // backspace.  A run of backspaces is one correction, not several.
            let replaced_at = taipo[i + 1..]
                .iter()
                .position(|c| is_text(c) || is_backspace(c))
                .map(|j| j + i + 1)
                .filter(|&j| !is_backspace(taipo[j]));
            let replacement = replaced_at.map(|j| taipo[j].code);

            // Only the first backspace of a run opens a correction.
            //
            // The run is measured over the chords that mean anything, not over adjacent
            // ones: a modifier or a dead chord between two backspaces types nothing, so
            // the second is still deleting what the first was.  Looking only at `i - 1`
            // read that as a fresh correction, and both halves then blamed the same chord
            // -- which is how a chord came to be charged more deletions than it had uses.
            let previous = taipo[..i]
                .iter()
                .rposition(|c| is_text(c) || is_backspace(c));
            if previous.map_or(false, |j| is_backspace(taipo[j])) {
                continue;
            }

            // The whole span, noticing included.  Capped at the idle ceiling: a
            // correction that took longer than that had the writer doing something else
            // in the middle of it, and charging the item for that is the same mistake the
            // ceiling exists to stop.
            let cost_ms = deleted_at
                .zip(replaced_at)
                .map(|(d, r)| taipo[r].time_ms.saturating_sub(taipo[d].time_ms))
                .filter(|&ms| ms <= opts.idle_ms);

            let confusion = deleted
                .zip(replacement)
                .filter(|(d, r)| d != r)
                .map(|(d, r)| Confusion::of(d, r));

            self.corrections.push(Correction {
                deleted,
                replacement,
                kind: classify(deleted, replacement),
                cost_ms,
                confusion,
            });

            if let Some(code) = deleted {
                let item = Item::Chord {
                    variant: VariantKey::of(chord.variant),
                    code,
                };
                let entry = self.items.entry(item).or_default();
                entry.corrections += 1;
                entry.correction_ms += cost_ms.unwrap_or(0) as u64;
            }
        }
    }

    /// A gap well above what this transition usually takes.
    ///
    /// Not an error, and the more interesting category: it finds what is not yet automatic
    /// without needing anything to go wrong.  Measured against the transition's own
    /// typical interval rather than a global one, because that is the whole point -- a
    /// chord can be quick after one neighbour and slow after another.
    fn find_hesitations(&mut self, opts: &Options) {
        // Needs enough samples for a median to mean anything.
        const MIN_SAMPLES: usize = 4;

        let mut found = Vec::new();
        for (item, stats) in &self.items {
            // On the recorded intervals rather than the occurrences: a transition seen
            // ten times, always across a break, has no typing interval to be slow
            // against.
            if !matches!(item, Item::Transition { .. }) || stats.intervals.len() < MIN_SAMPLES {
                continue;
            }
            let Some(typical) = stats.median() else {
                continue;
            };
            if typical == 0 {
                continue;
            }
            for sample in &stats.intervals {
                if (sample.gap_ms as f64) > typical as f64 * opts.hesitation_factor {
                    found.push(Hesitation {
                        at: sample.at,
                        item: *item,
                        interval_ms: sample.gap_ms,
                        typical_ms: typical,
                    });
                }
            }
        }
        // By how far above its own typical the gap was, not by the gap itself.  Sorted by
        // raw milliseconds the list is just everything that reached the idle ceiling; four
        // seconds on a transition that usually takes 192ms is the one worth looking at,
        // and four seconds on one that usually takes 1021ms is barely a pause.
        found.sort_by_key(|h| {
            std::cmp::Reverse((h.interval_ms as u64 * 1000) / h.typical_ms.max(1) as u64)
        });
        self.hesitations = found;
    }

    /// The median gap of the same-hand pairs, which says whether they are happening
    /// mid-burst or at the edge of the window.
    pub fn same_hand_median_gap(&self) -> Option<u32> {
        if self.same_hand.is_empty() {
            return None;
        }
        let mut v: Vec<u32> = self.same_hand.iter().map(|r| r.gap_ms).collect();
        v.sort_unstable();
        Some(v[v.len() / 2])
    }

    /// The share of eligible pairs that stayed on one hand.
    pub fn same_hand_rate(&self) -> f64 {
        if self.eligible_pairs == 0 {
            return 0.0;
        }
        self.same_hand.len() as f64 / self.eligible_pairs as f64
    }

    /// The writer's own pace: the *mean* gap between two chords that were typing.
    ///
    /// Everything in the ranking is measured against this rather than against an absolute
    /// number of milliseconds, so the ranking says "slow for you" and keeps saying it as
    /// the writer gets faster.
    ///
    /// The mean and not the median, which is the difference between a ranking that works
    /// and one that only re-sorts the frequency table.  Gaps are strongly right skewed --
    /// median 275ms against a mean nearer 400 -- so against the median every item
    /// accumulates excess from its own tail, at a rate set by how often it occurs, and the
    /// list comes out as `Bk`, `Sp`, `s`, `t`: the commonest chords, in order.  Against
    /// the mean a typical item scores about zero however often it is typed, and what is
    /// left is the items that really are slower than the rest.  The idle ceiling is what
    /// makes a mean usable at all here; without it one afternoon away from the keyboard
    /// would set it.
    pub fn baseline_ms(&self) -> u32 {
        let typing: Vec<u32> = self
            .gaps
            .iter()
            .copied()
            .filter(|&g| g < self.idle_ms)
            .collect();
        if typing.is_empty() {
            return 0;
        }
        (typing.iter().map(|&g| g as u64).sum::<u64>() / typing.len() as u64) as u32
    }

    /// The trouble spots, most expensive first.
    ///
    /// The point of ranking, and the reason it is a sum rather than a rate: something
    /// fumbled twice out of two occurrences matters less than something fumbled ten times
    /// out of fifty, and only a total weighted by how often the item actually comes up in
    /// this writer's own text can say so.  A rate on its own promotes whatever is rarest.
    ///
    /// Chords and transitions are ranked separately because they are not additive: a slow
    /// interval belongs to a chord *and* to the transition that led into it, and one list
    /// over both would count every millisecond twice.
    pub fn ranked(&self, transitions: bool) -> Vec<Ranked> {
        // What a transition is slow *against* is the other transitions out of the same
        // chord, not the corpus average.  Starting a word takes longer than continuing
        // one, and every transition out of space inherits that; measured against the
        // corpus the transition list came out as `space -> s`, `space -> c`, `space ->
        // d`, which is one fact about word boundaries repeated twenty times.  Against
        // the average gap out of space, what is left is the chords that are slow *after
        // a space in particular* -- which is the sequence effect the whole item type
        // exists for.
        let mut out_of: HashMap<(VariantKey, u16), (u64, usize)> = HashMap::new();
        for (item, stats) in &self.items {
            if let Item::Transition { variant, from, .. } = item {
                let entry = out_of.entry((*variant, *from)).or_default();
                entry.0 += stats.intervals.iter().map(|s| s.gap_ms as u64).sum::<u64>();
                entry.1 += stats.intervals.len();
            }
        }

        let corpus = self.baseline_ms();
        let mut rows: Vec<Ranked> = self
            .items
            .iter()
            .filter(|(item, _)| matches!(item, Item::Transition { .. }) == transitions)
            .filter(|(item, _)| match item {
                Item::Chord { variant, code } => types_a_character(*variant, *code),
                // Both ends: a transition into a letter from Return is the writer having
                // read something, and out of one to Cmd is the writer having decided to
                // do something else.
                Item::Transition { variant, from, to } => {
                    types_a_character(*variant, *from) && types_a_character(*variant, *to)
                }
            })
            .map(|(item, stats)| {
                // Transitions against their own starting chord, chords against the
                // corpus.  A chord has no narrower neighbourhood to be judged in.
                let baseline = match item {
                    Item::Transition { variant, from, .. } => out_of
                        .get(&(*variant, *from))
                        .filter(|(_, n)| *n > 0)
                        .map(|(sum, n)| (sum / *n as u64) as u32)
                        .unwrap_or(corpus),
                    Item::Chord { .. } => corpus,
                };
                // Time above the writer's own pace, over every occurrence that was
                // typing.  Signed per sample and only the total clamped: an item that is
                // quicker than average on most of its occurrences has genuinely earned
                // that time back, and truncating each sample at zero would charge it for
                // its tail while crediting it with nothing.  That asymmetry is what made
                // the first version of this list rank by frequency.
                let total: i64 = stats
                    .intervals
                    .iter()
                    .map(|s| s.gap_ms as i64 - baseline as i64)
                    .sum();
                let slow_ms = total.max(0) as u64;
                Ranked {
                    item: *item,
                    exposure: stats.count,
                    samples: stats.intervals.len(),
                    median_ms: stats.median().unwrap_or(0),
                    slow_ms,
                    corrections: stats.corrections,
                    correction_ms: stats.correction_ms,
                    same_hand: stats.same_hand,
                    cost_ms: slow_ms + stats.correction_ms,
                }
            })
            .filter(|r| r.cost_ms > 0)
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.cost_ms));
        rows
    }

    /// Percentile of the inter-chord gap distribution, in milliseconds.
    pub fn gap_percentile(&self, p: f64) -> u32 {
        if self.gaps.is_empty() {
            return 0;
        }
        let mut v = self.gaps.clone();
        v.sort_unstable();
        v[((v.len() - 1) as f64 * p) as usize]
    }

    /// How many gaps were at least this long.
    pub fn gaps_over(&self, ms: u32) -> usize {
        self.gaps.iter().filter(|&&g| g >= ms).count()
    }

    /// The gap distribution in buckets, as `(low, high, count)`.
    ///
    /// The edges widen with the values because that is how the gaps are spread: the
    /// difference between 100ms and 150ms is a fact about typing and the difference
    /// between 10s and 20s is not.  The last bucket's high is `u32::MAX`.
    pub fn gap_histogram(&self) -> Vec<(u32, u32, usize)> {
        const EDGES: &[u32] = &[
            0, 50, 75, 100, 125, 150, 175, 200, 250, 300, 400, 500, 650, 800, 1000, 1300,
            1600, 2000, 2500, 3000, 4000, 5000, 7000, 10000, 20000, 60000, u32::MAX,
        ];
        EDGES
            .windows(2)
            .map(|w| {
                (
                    w[0],
                    w[1],
                    self.gaps.iter().filter(|&&g| g >= w[0] && g < w[1]).count(),
                )
            })
            .collect()
    }

    /// Percentile of the spread distribution, in milliseconds.
    pub fn spread_percentile(&self, p: f64) -> u32 {
        if self.spreads.is_empty() {
            return 0;
        }
        let mut v = self.spreads.clone();
        v.sort_unstable();
        v[((v.len() - 1) as f64 * p) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify() {
        assert_eq!(classify(Some(0x030), Some(0x030)), CorrectionKind::SameChord);
        // A key added.
        assert_eq!(classify(Some(0x030), Some(0x031)), CorrectionKind::OneKeyOff);
        // A key missed.
        assert_eq!(classify(Some(0x030), Some(0x010)), CorrectionKind::OneKeyOff);
        // A key substituted for its neighbour: two bits, but one finger.
        assert_eq!(classify(Some(0x030), Some(0x050)), CorrectionKind::OneKeyOff);
        // Two bits apart, but a different number of fingers, so not a substitution.
        assert_eq!(classify(Some(0x030), Some(0x00c)), CorrectionKind::Different);
        assert_eq!(classify(Some(0x010), Some(0x00c)), CorrectionKind::Different);
        assert_eq!(classify(Some(0x030), None), CorrectionKind::NoReplacement);
    }

    /// One chord, for the correction tests.
    fn chord(code: u16, action: ChordAction, time_ms: u32) -> Chord {
        Chord {
            time_ms,
            side: Side::Left,
            code,
            variant: TaipoVariant::Taipo,
            mode: LayoutMode::Taipo,
            first_key_ms: time_ms,
            last_key_ms: time_ms,
            end: ChordEnd::AllReleased,
            action: Some(action),
        }
    }

    /// A run of backspaces is one correction even when something that types nothing has
    /// got in between them.
    ///
    /// A modifier or a dead chord between two backspaces does not end the run: it types
    /// nothing, so the second backspace is still deleting what the first was.  Counting
    /// it as a fresh correction made both halves blame the same chord, which is how a
    /// chord came to be charged more deletions than it had uses.
    #[test]
    fn test_a_modifier_does_not_break_a_backspace_run() {
        let bk = || ChordAction::Key(Keyboard::DeleteBackspace);
        let chords = vec![
            chord(0x001, ChordAction::Key(Keyboard::A), 0),
            chord(0x200, bk(), 100),
            // A shift, pressed and abandoned in the middle of backspacing.
            chord(0x088, ChordAction::OneShot(bbq_keyboard::Mods::SHIFT), 200),
            chord(0x200, bk(), 300),
            chord(0x002, ChordAction::Key(Keyboard::O), 400),
        ];
        let refs: Vec<&Chord> = chords.iter().collect();

        let mut analysis = Analysis::default();
        analysis.find_corrections(&refs, &Options::default());

        assert_eq!(analysis.corrections.len(), 1, "one correction, not two");
        assert_eq!(analysis.corrections[0].deleted, Some(0x001));
        // No single replacement, exactly as for a run with nothing in the middle of it:
        // two backspaces took more than one chord back, so no one chord replaced it.
        assert_eq!(analysis.corrections[0].replacement, None);
        assert_eq!(analysis.corrections[0].kind, CorrectionKind::NoReplacement);
    }

    /// A modifier *after* a lone backspace does not hide what replaced it.
    #[test]
    fn test_a_modifier_does_not_hide_the_replacement() {
        let chords = vec![
            chord(0x001, ChordAction::Key(Keyboard::A), 0),
            chord(0x200, ChordAction::Key(Keyboard::DeleteBackspace), 100),
            chord(0x088, ChordAction::OneShot(bbq_keyboard::Mods::SHIFT), 200),
            chord(0x002, ChordAction::Key(Keyboard::O), 300),
        ];
        let refs: Vec<&Chord> = chords.iter().collect();

        let mut analysis = Analysis::default();
        analysis.find_corrections(&refs, &Options::default());

        assert_eq!(analysis.corrections.len(), 1);
        assert_eq!(analysis.corrections[0].deleted, Some(0x001));
        assert_eq!(analysis.corrections[0].replacement, Some(0x002));
    }

    /// And a plain run is still one correction, which is what it always was.
    #[test]
    fn test_a_plain_backspace_run_is_one_correction() {
        let bk = || ChordAction::Key(Keyboard::DeleteBackspace);
        let chords = vec![
            chord(0x001, ChordAction::Key(Keyboard::A), 0),
            chord(0x200, bk(), 100),
            chord(0x200, bk(), 200),
            chord(0x200, bk(), 300),
            chord(0x002, ChordAction::Key(Keyboard::O), 400),
        ];
        let refs: Vec<&Chord> = chords.iter().collect();

        let mut analysis = Analysis::default();
        analysis.find_corrections(&refs, &Options::default());

        assert_eq!(analysis.corrections.len(), 1);
        assert_eq!(analysis.corrections[0].deleted, Some(0x001));
        assert_eq!(analysis.corrections[0].replacement, None);
    }

    #[test]
    fn test_median_is_robust_to_one_long_pause() {
        let stats = ItemStats {
            count: 5,
            intervals: [100, 110, 120, 130, 9000]
                .into_iter()
                .map(|gap_ms| Sample {
                    gap_ms,
                    at: At {
                        session: 0,
                        time_ms: 0,
                    },
                })
                .collect(),
            ..Default::default()
        };
        assert_eq!(stats.median(), Some(120));
    }
}

//////////////////////////////////////////////////////////////////////////////
// Grams that were spelled out
//////////////////////////////////////////////////////////////////////////////

/// A run of single-letter chords that a multi-character chord would have typed in one.
///
/// This is the check nothing outside the keyboard can make.  `docs/taipo-drills.md` says
/// so in as many words -- "nothing outside the keyboard can see whether a word was chorded
/// or spelled out, so the lists do the enforcing" -- and it is why the drills have to be
/// built defensively.  Here the question is simply answerable.
#[derive(Debug, Clone)]
pub struct SpelledGram {
    pub text: &'static str,
    /// The chord that would have typed it in one.
    pub code: u16,
}

/// The character a chord types, for the chords that type exactly one.
///
/// Spelled out rather than closed with a wildcard, so that a new kind of
/// action has to be given an answer here rather than quietly becoming "types
/// nothing".
fn typed_text(action: &ChordAction) -> Option<String> {
    match action {
        ChordAction::Key(k) => char_for_key(*k, false).map(|c| c.to_string()),
        ChordAction::Shifted(k) => char_for_key(*k, true).map(|c| c.to_string()),
        ChordAction::Text(s) => Some((*s).to_string()),
        // None of these put anything on the screen.  A variant selection is
        // not dead -- `is_dead` asks whether the table had an entry at all --
        // but it types nothing, so it takes no part in the n-gram analysis.
        ChordAction::OneShot(_) | ChordAction::Release | ChordAction::Variant(_) => None,
    }
}

/// `Keyboard` back to the character it types.
///
/// Built by inverting `usb_typer::key_for_char` rather than by writing a second table, so
/// that it cannot disagree with what the firmware actually sends.
fn char_for_key(key: Keyboard, shifted: bool) -> Option<char> {
    (0x20u8..0x7f).map(char::from).find(|&c| {
        match bbq_keyboard::usb_typer::key_for_char(c) {
            Some((k, mods)) => k == key && mods.contains(bbq_keyboard::Mods::SHIFT) == shifted,
            None => false,
        }
    })
}

/// Every multi-character chord in a variant's table, longest first.
fn text_chords(variant: VariantKey) -> Vec<(&'static str, u16)> {
    use bbq_keyboard::layout::taipo::Action;
    let table: &[bbq_keyboard::layout::taipo::Entry] = match variant {
        VariantKey::Taipo => bbq_keyboard::layout::taipo::TAIPO_ACTIONS,
        VariantKey::Dosh => bbq_keyboard::layout::dosh::DOSH_ACTIONS,
    };
    let mut out: Vec<(&'static str, u16)> = table
        .iter()
        .filter_map(|e| match e.action {
            Action::Text(s) => Some((s, e.code)),
            _ => None,
        })
        .collect();
    out.sort_by_key(|(s, _)| std::cmp::Reverse(s.len()));
    out
}

/// Find runs of single-character chords that a text chord would have covered.
///
/// Greedy and longest-first, so `tion` is reported rather than `on` inside it.  A run is
/// only counted when every chord in it typed exactly one character, since that is what
/// "spelled out" means.
pub fn find_spelled_grams(taipo: &[&Chord]) -> Vec<SpelledGram> {
    let mut out = Vec::new();
    // What each chord typed, as text, with `None` for anything that is not plain typing.
    let typed: Vec<Option<String>> = taipo
        .iter()
        .map(|c| c.action.as_ref().and_then(typed_text))
        .collect();

    let mut i = 0;
    while i < taipo.len() {
        let variant = VariantKey::of(taipo[i].variant);
        let mut matched = false;
        for (text, code) in text_chords(variant) {
            let len = text.chars().count();
            if len < 2 || i + len > taipo.len() {
                continue;
            }
            let run = &typed[i..i + len];
            // Every chord in the run must have typed exactly one character, and the
            // variant must not change partway.
            let ok = run.iter().zip(text.chars()).all(|(got, want)| {
                got.as_deref().is_some_and(|s| s.chars().count() == 1 && s.starts_with(want))
            }) && taipo[i..i + len]
                .iter()
                .all(|c| VariantKey::of(c.variant) == variant);
            if ok {
                out.push(SpelledGram {
                    text,
                    code,
                });
                i += len;
                matched = true;
                break;
            }
        }
        if !matched {
            i += 1;
        }
    }
    out
}
