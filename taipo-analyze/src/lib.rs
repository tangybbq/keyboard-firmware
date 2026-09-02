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

    // The tables these records were produced by have to be the ones they are replayed
    // through.  A chord that has since been given an entry stops being a dead chord, and
    // if what it gained selects the chord table then every chord after it in that session
    // resolves against the wrong one.  Counted rather than refused: a corpus older than
    // the current tables is the normal case for anyone who edits them, and the numbers
    // are still worth having as long as nobody is told they are exact.
    let ours = bbq_keyboard::layout::fingerprint::layout_fingerprint();
    let foreign: Vec<u64> = {
        let mut seen: Vec<u64> = sessions
            .iter()
            .filter_map(|s| s.layout)
            .filter(|f| *f != ours)
            .collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    let unstamped = sessions.iter().filter(|s| s.layout.is_none()).count();
    let foreign_sessions = sessions
        .iter()
        .filter(|s| s.layout.is_some_and(|f| f != ours))
        .count();

    let derived = bbq_keyboard::replay::replay_sessions(two_row, &sessions);
    let chords: Vec<Vec<&bbq_keyboard::replay::Chord>> = derived
        .iter()
        .map(|d| bbq_keyboard::replay::chords(d))
        .collect();
    let per_session: Vec<&[&bbq_keyboard::replay::Chord]> =
        chords.iter().map(|c| c.as_slice()).collect();
    let mut analysis = stats::Analysis::build(&per_session, opts);
    analysis.layout = ours;
    analysis.foreign_layouts = foreign;
    analysis.foreign_sessions = foreign_sessions;
    analysis.unstamped_sessions = unstamped;
    Ok(analysis)
}
