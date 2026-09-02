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
/// since Taipo and Posh are different skills that happen to share an engine.
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
    Posh,
}

impl VariantKey {
    pub fn of(v: TaipoVariant) -> VariantKey {
        match v {
            TaipoVariant::Taipo => VariantKey::Taipo,
            TaipoVariant::Posh => VariantKey::Posh,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VariantKey::Taipo => "taipo",
            VariantKey::Posh => "posh",
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

#[derive(Debug, Clone)]
pub struct Correction {
    pub deleted: Option<u16>,
    pub replacement: Option<u16>,
    pub kind: CorrectionKind,
}

#[derive(Debug, Clone)]
pub struct Hesitation {
    pub at: At,
    pub item: Item,
    pub interval_ms: u32,
    pub typical_ms: u32,
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
        a.find_corrections(&taipo);
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
    fn find_corrections(&mut self, taipo: &[&Chord]) {
        for (i, chord) in taipo.iter().enumerate() {
            if !is_backspace(chord) {
                continue;
            }
            // What it deleted: the nearest preceding chord that typed something.
            let deleted = taipo[..i].iter().rev().find(|c| is_text(c)).map(|c| c.code);
            // What replaced it: the next chord that types, as long as it is not another
            // backspace.  A run of backspaces is one correction, not several.
            let replacement = taipo[i + 1..]
                .iter()
                .find(|c| is_text(c) || is_backspace(c))
                .filter(|c| !is_backspace(c))
                .map(|c| c.code);

            // Only the first backspace of a run opens a correction.
            if i > 0 && is_backspace(taipo[i - 1]) {
                continue;
            }

            self.corrections.push(Correction {
                deleted,
                replacement,
                kind: classify(deleted, replacement),
            });

            if let Some(code) = deleted {
                let item = Item::Chord {
                    variant: VariantKey::of(chord.variant),
                    code,
                };
                self.items.entry(item).or_default().corrections += 1;
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
        VariantKey::Posh => bbq_keyboard::layout::posh::POSH_ACTIONS,
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
