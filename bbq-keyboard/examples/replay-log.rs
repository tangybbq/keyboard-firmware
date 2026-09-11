//! Replay a key event log file and print what the engine derives from it.
//!
//! The text log the collector writes (`keyminder log`, or the trainer's own
//! files under its logs directory) is split into sessions, each replayed
//! through the real layout engine, and every derived event is printed one
//! per line in the golden-file format.  For looking at what a keyboard
//! actually did, with grep.
//!
//!     cargo run --example replay-log -- FILE [--two-row] [--only chord|stroke|key]

use std::fs;

use bbq_keyboard::replay::{replay_sessions, sessions_from_text};

fn main() {
    let mut path = None;
    let mut two_row = true;
    let mut only: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--two-row" => two_row = true,
            "--three-row" => two_row = false,
            "--only" => only = args.next(),
            other => path = Some(other.to_string()),
        }
    }
    let path = path.expect("a log file");
    let text = fs::read_to_string(&path).expect("read the log");
    let sessions = sessions_from_text(&text).expect("parse the log");
    let derived = replay_sessions(two_row, &sessions);
    for (session, derived) in sessions.iter().zip(&derived) {
        println!(
            "# session started={:?} layout={:?} mode={:?} variant={:?} events={}",
            session.started_unix,
            session.layout.map(|l| format!("{l:#x}")),
            session.mode,
            session.variant,
            session.events.len()
        );
        for event in derived {
            let line = event.to_line();
            if let Some(only) = &only {
                if !line.starts_with(only.as_str()) {
                    continue;
                }
            }
            println!("{line}");
        }
    }
}
