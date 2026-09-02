//! Rendering an [`Analysis`] for a person to read.
//!
//! Kept apart from the arithmetic so that phase 4's app can take the numbers without
//! taking the prose.

use bbq_keyboard::layout::taipo::CHORD_TIME;

use crate::stats::{Analysis, At, CorrectionKind, Item};
use crate::Options;

/// The chord code as the keys it uses, e.g. `[r+s]`.
///
/// Named from the chord bits rather than from the letter it types, because the letter
/// depends on the table and the fingers do not.
fn chord_keys(code: u16) -> String {
    const BITS: [&str; 10] = ["a", "o", "t", "e", "r", "s", "n", "i", "Sp", "Bk"];
    let names: Vec<&str> = (0..10)
        .filter(|b| code & (1 << b) != 0)
        .map(|b| BITS[b])
        .collect();
    format!("[{}]", names.join("+"))
}

fn item_name(item: &Item) -> String {
    match item {
        Item::Chord { code, .. } => chord_keys(*code),
        Item::Transition { from, to, .. } => {
            format!("{} -> {}", chord_keys(*from), chord_keys(*to))
        }
    }
}

/// A point in the log, as `s2 +128456ms`.
///
/// Relative to its own session, because that is the only timeline it has: the sessions in
/// a file each count from their own zero, and the file's `# session` headers are what a
/// reader counts through to find `s2`.  Sessions are numbered from one here, so `s1` is
/// the first header in the first file given.
fn at_name(at: At) -> String {
    format!("s{} +{}ms", at.session + 1, at.time_ms)
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 * 100.0 / d as f64
    }
}

pub fn print(a: &Analysis, opts: &Options) {
    let typed = a.total_chords - a.non_taipo;

    println!("== Summary ==");
    println!("  chords            {}", a.total_chords);
    if a.sessions > 1 {
        println!(
            "  sessions          {} (each its own timeline; nothing is measured across a join)",
            a.sessions
        );
    }
    if a.non_taipo > 0 {
        println!(
            "  not taipo mode    {} (excluded from everything below)",
            a.non_taipo
        );
    }
    let minutes = a.span_ms as f64 / 60_000.0;
    if minutes > 0.0 {
        println!(
            "  span              {:.1} min, {:.0} chords/min while active",
            minutes,
            typed as f64 / minutes
        );
    }
    let mut variants: Vec<_> = a.variant_chords.iter().collect();
    variants.sort();
    for (v, n) in variants {
        println!("  {:<17} {} chords", v.name(), n);
    }
    if a.variant_switches > 0 {
        println!("  variant switches  {}", a.variant_switches);
    }

    println!("\n== How chords ended ==");
    let ends = a.ended_released + a.ended_timer + a.ended_other_hand;
    println!(
        "  all keys released {:>5}  {:>5.1}%",
        a.ended_released,
        pct(a.ended_released, ends)
    );
    println!(
        "  {}ms timer expired {:>4}  {:>5.1}%",
        CHORD_TIME,
        a.ended_timer,
        pct(a.ended_timer, ends)
    );
    println!(
        "  other hand started{:>5}  {:>5.1}%",
        a.ended_other_hand,
        pct(a.ended_other_hand, ends)
    );
    if ends > 0 && a.ended_other_hand == 0 {
        println!(
            "  Note: the cross-hand ending never fired.  It can only fire when the next\n  \
             chord starts within {}ms of this one, so at this pace the timer always wins\n  \
             and {:.0}% of chords wait out the full window before being sent.",
            CHORD_TIME,
            pct(a.ended_timer, ends)
        );
    }

    println!("\n== Chord spread (how much a chord is assembled rather than struck) ==");
    println!(
        "  median {}ms, p90 {}ms, p99 {}ms, max {}ms",
        a.spread_percentile(0.5),
        a.spread_percentile(0.9),
        a.spread_percentile(0.99),
        a.spread_percentile(1.0),
    );

    println!("\n== Hand alternation ==");
    println!(
        "  eligible pairs    {} (consecutive chords under {}ms apart)",
        a.eligible_pairs, opts.alternation_window_ms
    );
    println!(
        "  same hand         {} ({:.1}%)",
        a.same_hand.len(),
        a.same_hand_rate() * 100.0
    );
    if !a.same_hand.is_empty() {
        let left = a
            .same_hand
            .iter()
            .filter(|r| matches!(r.side, bbq_keyboard::Side::Left))
            .count();
        println!(
            "  by hand           {} left, {} right",
            left,
            a.same_hand.len() - left
        );
    }
    if let Some(median) = a.same_hand_median_gap() {
        println!(
            "  their median gap  {}ms  (mid-burst if this is small)",
            median
        );
    }
    if !a.same_hand.is_empty() {
        println!("  worst offenders:");
        let mut counts: std::collections::HashMap<(u16, u16), usize> = Default::default();
        for run in &a.same_hand {
            *counts.entry((run.from, run.to)).or_default() += 1;
        }
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for ((from, to), n) in rows.into_iter().take(opts.top) {
            println!(
                "    {:>3}x  {} -> {}",
                n,
                chord_keys(from),
                chord_keys(to)
            );
        }
    }

    println!("\n== Corrections ==");
    if a.corrections.is_empty() {
        println!("  none");
    } else {
        let count = |k: CorrectionKind| a.corrections.iter().filter(|c| c.kind == k).count();
        let total = a.corrections.len();
        println!("  total             {} ({:.1} per 100 chords)", total, pct(total, typed));
        println!(
            "  one key off       {:>4}   misfingered, a technique problem",
            count(CorrectionKind::OneKeyOff)
        );
        println!(
            "  different chord   {:>4}   the right chord was not recalled",
            count(CorrectionKind::Different)
        );
        println!(
            "  same chord again  {:>4}   the chord was right; something else was not",
            count(CorrectionKind::SameChord)
        );
        println!(
            "  nothing retyped   {:>4}   deleted and moved on",
            count(CorrectionKind::NoReplacement)
        );

        let mut worst: std::collections::HashMap<u16, usize> = Default::default();
        for c in &a.corrections {
            if let Some(d) = c.deleted {
                *worst.entry(d).or_default() += 1;
            }
        }
        let mut rows: Vec<_> = worst.into_iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        if !rows.is_empty() {
            println!("  most often corrected:");
            for (code, n) in rows.into_iter().take(opts.top) {
                // What it was replaced with, when that was consistent: a chord always
                // corrected to the same other chord is a different problem from one
                // corrected to something different every time.
                let mut repl: std::collections::HashMap<u16, usize> = Default::default();
                for c in a.corrections.iter().filter(|c| c.deleted == Some(code)) {
                    if let Some(r) = c.replacement {
                        *repl.entry(r).or_default() += 1;
                    }
                }
                let mut best: Vec<_> = repl.into_iter().collect();
                best.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
                match best.first() {
                    Some((r, rn)) => println!(
                        "    {:>3}x  {}  -> usually {} ({}x)",
                        n,
                        chord_keys(code),
                        chord_keys(*r),
                        rn
                    ),
                    None => println!("    {:>3}x  {}", n, chord_keys(code)),
                }
            }
        }
    }

    println!("\n== Dead chords (typed nothing at all) ==");
    if a.dead.is_empty() {
        println!("  none");
    } else {
        let mut counts: std::collections::HashMap<u16, usize> = Default::default();
        for (_, code, _) in &a.dead {
            *counts.entry(*code).or_default() += 1;
        }
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("  {} total", a.dead.len());
        for (code, n) in rows.into_iter().take(opts.top) {
            println!("    {:>3}x  {} (0x{:03x})", n, chord_keys(code), code);
        }
    }

    println!("\n== Grams spelled out that have a chord ==");
    if a.spelled.is_empty() {
        println!("  none -- every gram with a chord was chorded");
    } else {
        let mut counts: std::collections::HashMap<(&str, u16), usize> = Default::default();
        for g in &a.spelled {
            *counts.entry((g.text, g.code)).or_default() += 1;
        }
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!(
            "  {} runs, costing {} extra chords",
            a.spelled.len(),
            a.spelled
                .iter()
                .map(|g| g.text.chars().count() - 1)
                .sum::<usize>()
        );
        for ((text, code), n) in rows.into_iter().take(opts.top) {
            println!("    {:>3}x  {:<6} would have been {}", n, format!("\"{text}\""), chord_keys(code));
        }
    }

    println!("\n== Split chords (the window closed mid-chord) ==");
    println!("  {}", if a.split.is_empty() { "none".to_string() } else { format!("{}", a.split.len()) });

    println!("\n== Hesitations ==");
    if a.hesitations.is_empty() {
        println!("  none found (needs several samples of the same transition)");
    } else {
        println!(
            "  {} gaps over {}x the transition's own typical interval, of the {} pairs\n  \
             close enough together to be typing at all ({} were further apart than {}ms)",
            a.hesitations.len(),
            opts.hesitation_factor,
            a.gaps.len() - a.idle_pairs,
            a.idle_pairs,
            opts.idle_ms,
        );
        for h in a.hesitations.iter().take(opts.top) {
            println!(
                "    at {:>14}: {:>6}ms (usually {:>4}ms, {:>4.1}x)  {}",
                at_name(h.at),
                h.interval_ms,
                h.typical_ms,
                h.interval_ms as f64 / h.typical_ms.max(1) as f64,
                item_name(&h.item)
            );
        }
    }

    println!("\n== Slowest transitions with enough samples ==");
    println!("  (typical interval, over the samples that were typing rather than pauses)");
    let mut rows: Vec<_> = a
        .items
        .iter()
        .filter(|(i, s)| matches!(i, Item::Transition { .. }) && s.count >= 3)
        .filter_map(|(i, s)| s.median().map(|m| (m, s.count, i)))
        .collect();
    rows.sort_by_key(|(m, _, _)| std::cmp::Reverse(*m));
    if rows.is_empty() {
        println!("  not enough repeated transitions yet");
    } else {
        for (median, count, item) in rows.into_iter().take(opts.top) {
            println!("    {:>5}ms  {:>3}x  {}", median, count, item_name(item));
        }
    }

    if typed < 500 {
        println!(
            "\nNote: {typed} chords is a small sample.  Rates are indicative; the ranked\n\
             lists need a few thousand before they mean much."
        );
    }
}
