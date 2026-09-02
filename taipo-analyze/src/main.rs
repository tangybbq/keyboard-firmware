//! CLI over [`taipo_analyze`].

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use taipo_analyze::{report, Options};

#[derive(Parser)]
#[command(name = "taipo-analyze")]
#[command(about = "Find the trouble spots in a taipo key log")]
struct Cli {
    /// Log files written by `keyminder log`.
    #[arg(required = true)]
    logs: Vec<PathBuf>,

    /// A same-hand pair only counts as an alternation fault when the two chords are closer
    /// together than this.  After a pause, either hand is equally correct.
    #[arg(long, default_value_t = 2000)]
    alternation_window_ms: u32,

    /// A gap this many times a transition's own typical interval counts as a hesitation.
    #[arg(long, default_value_t = 3.0)]
    hesitation_factor: f64,

    /// Beyond this, two chords are too far apart to be typing: the writer stopped.  Such
    /// a pair contributes no interval, so it can neither raise a transition's typical time
    /// nor be reported as a hesitation.
    #[arg(long, default_value_t = 5000)]
    idle_ms: u32,

    /// How many rows to show in each ranked list.
    #[arg(long, default_value_t = 12)]
    top: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let opts = Options {
        alternation_window_ms: cli.alternation_window_ms,
        hesitation_factor: cli.hesitation_factor,
        idle_ms: cli.idle_ms,
        top: cli.top,
    };

    // Several days of logs analyse as one body of typing, but each file is kept apart until
    // it has been split into its sessions: no two of them share a timeline.
    let mut texts = Vec::new();
    for path in &cli.logs {
        texts.push(
            std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?,
        );
    }
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();

    // The boards in use are two-row.  A three-row log carries RowShift markers, which the
    // replay does not act on yet.
    let analysis =
        taipo_analyze::analyze_files(&texts, true, &opts).map_err(|e| anyhow::anyhow!(e))?;
    if analysis.total_chords == 0 {
        println!("No chords found.");
        return Ok(());
    }
    report::print(&analysis, &opts);
    Ok(())
}
