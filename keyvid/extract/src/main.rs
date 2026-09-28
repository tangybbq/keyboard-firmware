//! Cut a stretch of typing out of a key log, for the video scenes to animate.
//!
//! The log is the one TaipoTeacher writes each day (or `keyminder log`'s): key
//! presses and releases with their times, nothing more.  What those keys
//! meant -- which chord, which Orsy stroke, what it typed -- is worked out by
//! replaying the whole session through the real layout engine, so a video
//! shows what the firmware did rather than a Python re-implementation's idea
//! of it.
//!
//!     keyvid-extract LOG --list                   # sessions, and what was typed when
//!     keyvid-extract LOG --session 2 --from 2475000 --to 2490000 -o clips/ok.json
//!     keyvid-extract LOG --runs                   # the stretches between mode switches
//!     keyvid-extract LOG --last orsy -o clips/ok.json
//!
//! Times are the log's own milliseconds on the keyboard's clock, which count
//! from the start of each session and have no tie to the time of day; `--list`
//! prints them beside each line of text so a stretch can be picked out.
//!
//! The easier way to mark a stretch is with the mode switch: switch to Orsy,
//! type it, switch back, and `--last orsy` cuts exactly that.  A run is the
//! typing between two switches, without the switch chords themselves.

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use bbq_keyboard::layout::export::char_for_key;
use bbq_keyboard::layout::fingerprint;
use bbq_keyboard::layout::orsy::StrokeOutcome;
use bbq_keyboard::layout::taipo::TaipoVariant;
use bbq_keyboard::replay::{replay_sessions, sessions_from_text, ChordAction, Derived};
use bbq_keyboard::{KeyAction, Keyboard, LayoutMode, Mods, Side};
use clap::Parser;
use serde::Serialize;

#[derive(Parser)]
struct Args {
    /// The key log to read.
    log: PathBuf,
    /// Print each session and a transcript of it with times, instead of a clip.
    #[arg(long)]
    list: bool,
    /// Which session of the log, counting from 0 as `--list` does.
    #[arg(long, default_value_t = 0)]
    session: usize,
    /// Where the clip starts, in the log's milliseconds.  The session's start
    /// when left out.
    #[arg(long)]
    from: Option<u32>,
    /// Where the clip ends, in the log's milliseconds.  The session's end when
    /// left out.
    #[arg(long)]
    to: Option<u32>,
    /// The log came from a three-row board (the proto3, the jolts).  The mesas
    /// and the proto4 are two-row, which is the default.
    #[arg(long)]
    three_row: bool,
    /// Print the runs -- the stretches typed in one mode between two switches
    /// -- numbered for `--run`, instead of a clip.
    #[arg(long)]
    runs: bool,
    /// Cut run N, as `--runs` numbers them; negative counts from the end.
    #[arg(long, allow_negative_numbers = true)]
    run: Option<i64>,
    /// Cut the last run in this mode (`orsy`, `dosh`, `taipo`) that a switch
    /// ended.  One still going when the log ends is left out: it is whatever
    /// was typed to ask for the clip.
    #[arg(long)]
    last: Option<String>,
    /// Where to write the clip; standard output when left out.
    #[arg(short, long)]
    out: Option<PathBuf>,
}

/// A clip, as the scenes read it.
#[derive(Serialize)]
struct Clip {
    source: String,
    session: usize,
    /// The window, in the log's milliseconds.  Every other time in the clip
    /// counts from `from_ms`.
    from_ms: u32,
    to_ms: u32,
    /// The mode at the start of the clip: `dosh`, `taipo` or `orsy`.
    start_mode: String,
    /// What was on the current line when the clip started.  The `text` of each
    /// event leaves it out.
    text_before: String,
    /// Every key press and release in the window.
    keys: Vec<KeyEvent>,
    /// What the engine made of them.
    events: Vec<Event>,
}

#[derive(Serialize)]
struct KeyEvent {
    t: u32,
    /// As the log names it: `L.o`, `R.Bk`.
    key: String,
    down: bool,
}

#[derive(Serialize)]
struct Event {
    /// When the engine committed it.
    t: u32,
    /// `chord`, `stroke` or `mode`.
    kind: &'static str,
    /// The mode after this event.
    mode: String,
    /// `L` or `R` for a chord; absent for a stroke, which is both hands.
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<&'static str>,
    /// The chord bits on each hand.
    left: u16,
    right: u16,
    /// When the first and last keys of it went down.
    first_key: u32,
    last_key: u32,
    /// The keys, spelled as the docs spell them: `o+Bk`, `o+n-a+i+Sp`.
    spell: String,
    /// What to show for it: what it typed, or what it did.
    label: String,
    /// Whether the table had nothing for it.
    dead: bool,
    /// The label when the event types nothing: what the action was.
    #[serde(skip)]
    fallback: String,
    /// Everything typed since the clip started, once this event's keys were
    /// sent.
    text: String,
}

fn main() -> Result<()> {
    let mut args = Args::parse();
    let text = fs::read_to_string(&args.log)
        .with_context(|| format!("reading {}", args.log.display()))?;
    let sessions = sessions_from_text(&text).map_err(anyhow::Error::msg)?;
    let two_row = !args.three_row;
    let derived = replay_sessions(two_row, &sessions);

    if args.list {
        for (i, (session, derived)) in sessions.iter().zip(&derived).enumerate() {
            let first = session.events.first().map_or(0, |e| e.time_ms);
            let last = session.events.last().map_or(0, |e| e.time_ms);
            println!(
                "# session {i}: {first}..{last} ({} keys), opens in {}",
                session.events.len(),
                mode_name(
                    session.mode.unwrap_or(LayoutMode::Taipo),
                    session.variant.unwrap_or(TaipoVariant::Dosh)
                ),
            );
            list(session.mode, session.variant, derived);
        }
        return Ok(());
    }

    if args.runs || args.run.is_some() || args.last.is_some() {
        let runs = find_runs(&sessions, &derived);
        if args.runs {
            for (i, run) in runs.iter().enumerate() {
                let (from, to) = run.window();
                let mut text: String = run.text.replace('\n', "⏎");
                if text.chars().count() > 60 {
                    text = text.chars().take(59).collect::<String>() + "…";
                }
                println!(
                    "{i:>4} session {} {:<5} {from:>9}..{to:<9} {:>5.1}s{} {text}",
                    run.session,
                    run.mode,
                    (to - from) as f64 / 1000.0,
                    if run.switched_at.is_some() { "" } else { " (open)" },
                );
            }
            return Ok(());
        }
        let run = if let Some(n) = args.run {
            let ix = if n < 0 { runs.len() as i64 + n } else { n };
            usize::try_from(ix)
                .ok()
                .and_then(|ix| runs.get(ix))
                .with_context(|| format!("no run {n}; the log has {}", runs.len()))?
        } else {
            let mode = args.last.as_deref().unwrap();
            runs.iter()
                .rev()
                .find(|r| r.mode == mode && r.switched_at.is_some())
                .with_context(|| format!("no finished {mode} run in the log"))?
        };
        let (from, to) = run.window();
        args.session = run.session;
        args.from = Some(from);
        args.to = Some(to);
    }

    let Some(session) = sessions.get(args.session) else {
        bail!("the log has {} sessions", sessions.len());
    };
    let derived = &derived[args.session];
    // The replay uses today's tables.  The header only fingerprints the
    // Taipo and Dosh ones, so a change to Orsy's goes by unseen.
    if let Some(layout) = session.layout {
        if layout != fingerprint::layout_fingerprint() {
            eprintln!(
                "warning: session {} was typed with chord tables {layout:#x}, not today's; \
                 the clip may show chords that were not typed",
                args.session
            );
        }
    }
    let from = args
        .from
        .unwrap_or_else(|| session.events.first().map_or(0, |e| e.time_ms));
    let to = args
        .to
        .unwrap_or_else(|| session.events.last().map_or(0, |e| e.time_ms));
    let in_window = |t: u32| from <= t && t <= to;

    let keys = session
        .events
        .iter()
        .filter(|e| in_window(e.time_ms))
        .map(|e| KeyEvent {
            t: e.time_ms - from,
            key: bbq_keyboard::replay::key_name(e.key),
            down: e.press,
        })
        .collect();

    let mut state = State::new(session.mode, session.variant);
    let mut clip = Clip {
        source: args.log.display().to_string(),
        session: args.session,
        from_ms: from,
        to_ms: to,
        start_mode: String::new(),
        text_before: String::new(),
        keys,
        events: Vec::new(),
    };
    let mut base: Option<String> = None;
    for event in derived {
        let t = event.time_ms();
        if t > to {
            break;
        }
        if t >= from && base.is_none() {
            clip.start_mode = state.mode();
            let line_start = state.screen.rfind('\n').map_or(0, |i| i + 1);
            clip.text_before = state.screen[line_start..].to_string();
            base = Some(state.screen.clone());
        }
        let typed = state.apply(event);
        let Some(base) = &base else { continue };
        let since = &state.screen[common_prefix(base, &state.screen)..];
        match event {
            Derived::Chord(chord) => {
                let (left, right) = match chord.side {
                    Side::Left => (chord.code, 0),
                    Side::Right => (0, chord.code),
                };
                clip.events.push(Event {
                    t: t - from,
                    kind: "chord",
                    mode: state.mode(),
                    side: Some(match chord.side {
                        Side::Left => "L",
                        Side::Right => "R",
                    }),
                    left,
                    right,
                    first_key: chord.first_key_ms.saturating_sub(from),
                    last_key: chord.last_key_ms.saturating_sub(from),
                    spell: chord.key_names(),
                    label: String::new(),
                    dead: chord.is_dead(),
                    fallback: chord_label(chord.action.as_ref()),
                    text: since.to_string(),
                });
            }
            Derived::Stroke(stroke) => {
                clip.events.push(Event {
                    t: t - from,
                    kind: "stroke",
                    mode: state.mode(),
                    side: None,
                    left: stroke.chord.left,
                    right: stroke.chord.right,
                    first_key: stroke.first_key_ms.saturating_sub(from),
                    last_key: stroke.last_key_ms.saturating_sub(from),
                    spell: stroke.key_names(),
                    label: String::new(),
                    dead: stroke.is_dead(),
                    fallback: stroke_label(&stroke.outcome),
                    text: since.to_string(),
                });
            }
            Derived::Mode { .. } | Derived::Variant { .. } => {
                clip.events.push(Event {
                    t: t - from,
                    kind: "mode",
                    mode: state.mode(),
                    side: None,
                    left: 0,
                    right: 0,
                    first_key: t - from,
                    last_key: t - from,
                    spell: String::new(),
                    label: state.mode(),
                    dead: false,
                    fallback: String::new(),
                    text: since.to_string(),
                });
            }
            Derived::Key { .. } => {
                // Belongs to the chord or stroke before it, and what it typed
                // is that one's label -- except a syllable's, which is named
                // by its letters without the spaces around them.
                if let Some(last) = clip.events.iter_mut().rev().find(|e| e.kind != "mode") {
                    if let Some(typed) = typed {
                        if last.kind == "chord" || last.fallback.is_empty() {
                            last.label.push_str(&typed);
                        }
                    }
                    last.text = since.to_string();
                }
            }
            _ => {}
        }
    }
    for event in &mut clip.events {
        if event.label.is_empty() {
            event.label = std::mem::take(&mut event.fallback);
        }
    }
    if base.is_none() {
        bail!("nothing happens between {from} and {to} in session {}", args.session);
    }

    let json = serde_json::to_string_pretty(&clip)?;
    match &args.out {
        Some(path) => fs::write(path, json + "\n")
            .with_context(|| format!("writing {}", path.display()))?,
        None => println!("{json}"),
    }
    Ok(())
}

/// A transcript of a session: each line of text with the times it was typed
/// between, so a stretch can be chosen for a clip.
fn list(mode: Option<LayoutMode>, variant: Option<TaipoVariant>, derived: &[Derived]) {
    let mut state = State::new(mode, variant);
    // Where the line being built starts in the screen text, and when.
    let mut line_from = 0;
    let mut line_times: Option<(u32, u32)> = None;
    let mut line_mode = state.mode();
    for event in derived {
        let t = event.time_ms();
        if matches!(event, Derived::Chord(_) | Derived::Stroke(_)) {
            let (start, _) = line_times.get_or_insert((t, t));
            line_times = Some((*start, t));
            if state.screen.len() <= line_from {
                line_mode = state.mode();
            }
        }
        state.apply(event);
        line_from = line_from.min(state.screen.len());
        let line = &state.screen[line_from..];
        let full = line.ends_with('\n') || (line.len() > 70 && line.ends_with(' '));
        if full {
            if let Some((start, end)) = line_times.take() {
                println!("{start:>10} {end:>10} {line_mode:<5} {}", line.trim_end());
            }
            line_from = state.screen.len();
        }
    }
    if let Some((start, end)) = line_times {
        println!("{start:>10} {end:>10} {line_mode:<5} {}", &state.screen[line_from..]);
    }
}

/// A stretch typed in one mode, between two switches.
struct Run {
    session: usize,
    mode: String,
    /// When the switch into this mode was committed, or the session began.
    began: u32,
    /// The first key of its first chord or stroke.
    first_key: u32,
    /// When its last chord or stroke was committed.
    last: u32,
    /// When the first key of the switch that ended it went down; `None` for
    /// a run still going when the session ends.
    switched_at: Option<u32>,
    /// What it typed.
    text: String,
}

impl Run {
    /// How much quiet to keep either side of the typing.
    const LEAD_IN: u32 = 400;
    const TAIL: u32 = 800;

    /// The clip's window: the typing, a little either side, and none of the
    /// switches' keys.
    fn window(&self) -> (u32, u32) {
        let from = self.first_key.saturating_sub(Self::LEAD_IN).max(self.began);
        let to = self.last + Self::TAIL;
        let to = match self.switched_at {
            Some(switch) => to.min(switch.saturating_sub(1)),
            None => to,
        };
        (from, to.max(self.last))
    }
}

/// Whether a derived event is a chord or stroke that switches mode or table.
fn is_switch(event: &Derived) -> bool {
    match event {
        Derived::Chord(chord) => matches!(
            chord.action,
            Some(ChordAction::Orsy) | Some(ChordAction::Variant(_))
        ),
        Derived::Stroke(stroke) => stroke.outcome == StrokeOutcome::ToggleDosh,
        _ => false,
    }
}

/// Every run with something typed in it, in log order.
fn find_runs(sessions: &[bbq_keyboard::replay::LogSession], derived: &[Vec<Derived>]) -> Vec<Run> {
    let mut out = Vec::new();
    for (ix, (session, derived)) in sessions.iter().zip(derived).enumerate() {
        let mut state = State::new(session.mode, session.variant);
        let began = session.events.first().map_or(0, |e| e.time_ms);
        let new_run = |state: &State, began| Run {
            session: ix,
            mode: state.mode(),
            began,
            first_key: 0,
            last: 0,
            switched_at: None,
            text: String::new(),
        };
        let mut run = new_run(&state, began);
        let mut typed = false;
        let mut start = 0;
        let mut switch_key = None;
        for event in derived {
            let before = state.mode();
            state.apply(event);
            if is_switch(event) {
                let first = match event {
                    Derived::Chord(c) => c.first_key_ms,
                    Derived::Stroke(s) => s.first_key_ms,
                    _ => unreachable!(),
                };
                switch_key.get_or_insert(first);
                continue;
            }
            match event {
                Derived::Chord(_) | Derived::Stroke(_) => {
                    let (first, t) = match event {
                        Derived::Chord(c) => (c.first_key_ms, c.time_ms),
                        Derived::Stroke(s) => (s.first_key_ms, s.time_ms),
                        _ => unreachable!(),
                    };
                    if !typed {
                        run.first_key = first;
                        typed = true;
                    }
                    run.last = t;
                    // A switch chord that came to nothing is not a switch.
                    switch_key = None;
                }
                Derived::Mode { time_ms, .. } | Derived::Variant { time_ms, .. }
                    if state.mode() != before =>
                {
                    run.switched_at = Some(switch_key.take().unwrap_or(*time_ms));
                    run.text = state.screen.get(start..).unwrap_or("").to_string();
                    if typed {
                        out.push(run);
                    }
                    run = new_run(&state, *time_ms);
                    typed = false;
                    start = state.screen.len();
                }
                _ => {}
            }
        }
        if typed {
            run.text = state.screen.get(start..).unwrap_or("").to_string();
            out.push(run);
        }
    }
    out
}

/// The keyboard's mode and the text it has typed, followed through a replay.
struct State {
    mode: LayoutMode,
    variant: TaipoVariant,
    /// What the host would show: printable characters, newlines, and
    /// backspace taking one away.  Shortcuts type nothing.
    screen: String,
}

impl State {
    fn new(mode: Option<LayoutMode>, variant: Option<TaipoVariant>) -> State {
        State {
            mode: mode.unwrap_or(LayoutMode::Taipo),
            variant: variant.unwrap_or(TaipoVariant::Dosh),
            screen: String::new(),
        }
    }

    fn mode(&self) -> String {
        mode_name(self.mode, self.variant)
    }

    /// Follow one derived event, returning what it typed, for a label.
    fn apply(&mut self, event: &Derived) -> Option<String> {
        match event {
            Derived::Mode { mode, .. } => self.mode = *mode,
            Derived::Variant { variant, .. } => self.variant = *variant,
            Derived::Key {
                action: KeyAction::KeyPress(key, mods),
                ..
            } => return Some(self.press(*key, *mods)),
            _ => {}
        }
        None
    }

    fn press(&mut self, key: Keyboard, mods: Mods) -> String {
        let chords = Mods::CONTROL | Mods::ALT | Mods::GUI;
        if mods.intersects(chords) {
            // A shortcut: nothing on the screen, but worth naming.
            let mut name = String::new();
            for (flag, sym) in [
                (Mods::CONTROL, "⌃"),
                (Mods::ALT, "⌥"),
                (Mods::SHIFT, "⇧"),
                (Mods::GUI, "⌘"),
            ] {
                if mods.contains(flag) {
                    name.push_str(sym);
                }
            }
            name.push_str(&key_label(key));
            return name;
        }
        match key {
            Keyboard::DeleteBackspace => {
                self.screen.pop();
                "⌫".into()
            }
            Keyboard::ReturnEnter => {
                self.screen.push('\n');
                "⏎".into()
            }
            _ => match char_for_key(key, mods.contains(Mods::SHIFT)) {
                Some(ch) => {
                    self.screen.push(ch);
                    if ch == ' ' {
                        "␣".into()
                    } else {
                        ch.to_string()
                    }
                }
                None => key_label(key),
            },
        }
    }
}

fn mode_name(mode: LayoutMode, variant: TaipoVariant) -> String {
    match mode {
        LayoutMode::Taipo => match variant {
            TaipoVariant::Taipo => "taipo".into(),
            TaipoVariant::Dosh => "dosh".into(),
        },
        other => format!("{other:?}").to_lowercase(),
    }
}

/// A key with no character of its own, named for a label.
fn key_label(key: Keyboard) -> String {
    match key {
        Keyboard::Escape => "esc".into(),
        Keyboard::Tab => "⇥".into(),
        Keyboard::LeftArrow => "←".into(),
        Keyboard::RightArrow => "→".into(),
        Keyboard::UpArrow => "↑".into(),
        Keyboard::DownArrow => "↓".into(),
        Keyboard::DeleteBackspace => "⌫".into(),
        Keyboard::ReturnEnter => "⏎".into(),
        other => match char_for_key(other, false) {
            Some(ch) => ch.to_ascii_uppercase().to_string(),
            None => format!("{other:?}"),
        },
    }
}

/// What a chord that typed nothing did.
fn chord_label(action: Option<&ChordAction>) -> String {
    match action {
        None => "✗".into(),
        Some(ChordAction::OneShot(mods)) => {
            let mut name = String::new();
            for (flag, sym) in [
                (Mods::CONTROL, "⌃"),
                (Mods::ALT, "⌥"),
                (Mods::SHIFT, "⇧"),
                (Mods::GUI, "⌘"),
            ] {
                if mods.contains(flag) {
                    name.push_str(sym);
                }
            }
            name
        }
        Some(ChordAction::Release) => "release".into(),
        Some(ChordAction::Variant(TaipoVariant::Taipo)) => "→ dosh".into(),
        Some(ChordAction::Variant(TaipoVariant::Dosh)) => "→ taipo".into(),
        Some(ChordAction::Orsy) => "→ orsy".into(),
        Some(other) => format!("{other:?}"),
    }
}

/// What an Orsy stroke did, when it was more than typing characters.
///
/// A syllable is named by its letters, without the spaces the output stage
/// put around it.  Punctuation and the Dosh one-shot are left empty, so the
/// characters they type name them instead.
fn stroke_label(outcome: &StrokeOutcome) -> String {
    match outcome {
        StrokeOutcome::Text(t) => t.text(),
        StrokeOutcome::Punct(_) | StrokeOutcome::Dosh(..) => String::new(),
        StrokeOutcome::Space => "␣".into(),
        StrokeOutcome::Undo => "undo".into(),
        StrokeOutcome::CapNext => "Cap".into(),
        StrokeOutcome::Join => "join".into(),
        StrokeOutcome::AllCaps => "CAPS".into(),
        StrokeOutcome::CapPrevious => "cap ←".into(),
        StrokeOutcome::Uncap => "uncap".into(),
        StrokeOutcome::UncapPrevious => "uncap ←".into(),
        StrokeOutcome::ToggleDosh => "→ dosh".into(),
        StrokeOutcome::Dead => "✗".into(),
    }
}

/// How many bytes two strings share at the start, on a character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map_or(a.len().min(b.len()), |((i, _), _)| i)
}
