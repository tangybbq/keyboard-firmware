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

    /// How many rows to show in each ranked list.
    #[arg(long, default_value_t = 12)]
    top: usize,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let opts = Options {
        alternation_window_ms: cli.alternation_window_ms,
        hesitation_factor: cli.hesitation_factor,
        top: cli.top,
    };

    // Concatenated, so several days of logs analyse as one body of typing.  The offsets are
    // per file, which only matters for the absolute times in the hesitation list.
    let mut text = String::new();
    for path in &cli.logs {
        text.push_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?,
        );
        text.push('\n');
    }

    // The boards in use are two-row.  A three-row log carries RowShift markers, which the
    // replay does not act on yet.
    let analysis = taipo_analyze::analyze(&text, true, &opts).map_err(|e| anyhow::anyhow!(e))?;
    if analysis.total_chords == 0 {
        println!("No chords found.");
        return Ok(());
    }
    report::print(&analysis, &opts);
    Ok(())
}
