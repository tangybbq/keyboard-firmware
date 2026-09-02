//! What real typing actually looks like.
//!
//! `taipo-teacher.md` phase 3.  Replays the key logs `keyminder log` writes through the
//! real chord engine and works out what went wrong and where the time went.
//!
//! A library as well as a CLI, because the plan is explicit that the Mac app should consume
//! *this* analysis rather than grow a second one.  [`stats`] is all numbers; [`report`] is
//! the prose, and nothing in `stats` depends on it.

pub mod report;
pub mod stats;

/// The knobs the analysis takes.
///
/// Deliberately not the clap type: a library should not make its callers parse arguments.
#[derive(Debug, Clone)]
pub struct Options {
    /// A same-hand pair only counts as an alternation fault when the two chords are closer
    /// together than this.  After a pause either hand is equally correct, so a gap beyond
    /// it is neither counted nor put in the denominator.
    pub alternation_window_ms: u32,
    /// A gap this many times a transition's own typical interval counts as a hesitation.
    pub hesitation_factor: f64,
    /// Beyond this, the gap between two chords is not typing at all.
    ///
    /// The writer stopped: read something, ran a command, left the room.  Such a pair is
    /// counted as an occurrence but contributes no interval, so it neither inflates a
    /// transition's typical time nor gets reported as a hesitation.  Without it the first
    /// real corpus reported a 64-minute "hesitation" after Return, measured against a
    /// baseline of 24 seconds that was made of other pauses exactly like it.
    ///
    /// Five seconds is judgement, from the gap distribution: it is above the 98th
    /// percentile, so it costs almost nothing, and recalling a chord you half know is a
    /// matter of a second or two.  Past that the pause is about what to write rather than
    /// how to type it, which is not what this measures.
    pub idle_ms: u32,
    /// How many rows to show in each ranked list.
    pub top: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            alternation_window_ms: 2000,
            hesitation_factor: 3.0,
            idle_ms: 5000,
            top: 12,
        }
    }
}

/// Replay a log and analyse it, which is the whole pipeline in one call.
pub fn analyze(text: &str, two_row: bool, opts: &Options) -> Result<stats::Analysis, String> {
    analyze_files(&[text], two_row, opts)
}

/// The same, over several log files.
///
/// Each file is split into its sessions before anything is replayed, and each session is
/// replayed on its own.  Both splits matter: a log file is appended to across collector
/// runs, so its own offsets restart partway through, and two files have no timeline in
/// common at all.  Concatenating either would hand the replay a step backwards in time.
pub fn analyze_files(
    texts: &[&str],
    two_row: bool,
    opts: &Options,
) -> Result<stats::Analysis, String> {
    let mut sessions = Vec::new();
    for text in texts {
        sessions.extend(bbq_keyboard::replay::sessions_from_text(text)?);
    }
    let derived = bbq_keyboard::replay::replay_sessions(two_row, &sessions);
    let chords: Vec<Vec<&bbq_keyboard::replay::Chord>> = derived
        .iter()
        .map(|d| bbq_keyboard::replay::chords(d))
        .collect();
    let per_session: Vec<&[&bbq_keyboard::replay::Chord]> =
        chords.iter().map(|c| c.as_slice()).collect();
    Ok(stats::Analysis::build(&per_session, opts))
}
