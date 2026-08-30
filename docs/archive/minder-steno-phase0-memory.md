> **Shelved, 2026-08-30.**  This was the hand-off for phase 0 of
> `minder-steno-plan.md`: measuring what one steno window context costs so the
> number of contexts could be chosen.  Steno is no longer being pursued and
> nothing here was started.
>
> One observation in it outlives the rest and is reused by `taipo-teacher.md`:
> **the 64K heap is a guess, not a limit.**  The RP2040 has 264K of SRAM, the
> linker leaves a 200K `RAM` region, and a build uses about 11K of that outside
> the heap — so roughly 189K sits idle.  The taipo event-log ring buffer is
> sized against that number rather than against `HEAP_SIZE`.

# Next: minder phase 0 — memory profiling

The minder work plan is `tasks-minder.md`; this file is the hand-off for its **phase 0** only.
Phase 0 exists to answer two questions with numbers rather than guesses, both of which gate
phase 2 (per-window contexts):

1. What does one window context actually cost, in bytes?
2. How far can the lookup/joiner history constants shrink before writing gets worse — and does
   anything need to shrink at all, given that the 64K heap is an old guess rather than a limit
   (see the Notes at the end)?

Nothing in phase 0 changes device behavior.  It is host-side measurement plus one small
read-only addition to the minder protocol.

## What a context is, and where the pieces live

A context is one `Lookup` + one `Joiner` (`tasks-minder.md` phase 2 makes `Dict` hold a `Vec`
of them).  Sizes to attribute, all from the code:

- `Joiner.typed`: `String` bounded by `MAX_TYPED = 576` (`MIN_TYPED * 2 + 64`), truncated back
  to `MIN_TYPED = 256` on overflow — `bbq-steno/src/dict/joiner.rs:40-45`.
- `Joiner.history`: `VecDeque<Add>`, `MAX_HISTORY = 50` (`joiner.rs:48`).  `Add` is 32 bytes
  inline plus two heap allocations for typically short strings.
- `Lookup.history`: `heapless::Deque<Entry, HISTORY_LEN>`, `HISTORY_LEN = 32`
  (`bbq-steno/src/dict/lookup.rs:35`).  Inline cost is `32 * size_of::<Vec>()`; the real
  variable is `Entry.nodes: Vec<Box<dyn Selector>>` (`lookup.rs:50-55`).
- The dictionaries are `Rc<dyn DictImpl>` over XIP flash and shared, so a second context adds
  nothing for them.

**The unknown is the node count per `Lookup` history entry.**  `add_stroke` seeds a fresh
epsilon selector per dictionary on every stroke (`lookup.rs:110`), so the surviving-NFA-state
count grows with the number of loaded dictionaries as well as with ambiguous prefixes.  That
is the number to measure.

## The harness: build a new one, don't reuse dict-test as-is

Both existing host tools build a `Lookup` + `Joiner` over `../bbq-tool/full.bin`, but neither
is usable for steady-state measurement without change:

- `dict-test` (`dict-test/src/main.rs`) is non-interactive and already walks the drill corpus
  at `~/steno/steno-drill/phoenix` (544 files), but `Entry::check` constructs a **fresh**
  `Lookup` and `Joiner` per drill entry — a handful of strokes each.  `typed` never fills and
  the history deque never wraps, so it measures startup cost, not steady state.
- `typey write` (`typey/src/main.rs`) does keep one persistent `Lookup`/`Joiner`, but reads
  raw steno interactively from a terminal in raw mode.

The cheapest path is a new binary (or a `profile` subcommand on `typey`) that:

- installs a tracking `#[global_allocator]` wrapper around the system allocator, counting live
  bytes and peak live bytes, with a way to snapshot the counters;
- loads `bbq-tool/full.bin` via `MemDict::from_raw_ptr` exactly as the two tools above do;
- constructs **one** `Lookup` + `Joiner` and feeds it the whole corpus — the drill files
  concatenated are the obvious source, parsed with `dict-test`'s existing regex and
  `Stroke::from_text`;
- drains the joiner each stroke (as `Entry::check` does) so `typed` behaves as it does in
  firmware.

Measure the delta between "dictionaries loaded, no context" and "dictionaries loaded, one
context at steady state" so the shared XIP/`Rc` cost isn't counted against the per-context
number.

## Tasks

- [ ] Build the harness above; report live and peak bytes.
- [ ] Run the corpus long enough for `typed` to reach `MAX_TYPED` at least once and for the
      `Lookup` deque to fill, and record:
      - steady-state live bytes for one context
      - peak live bytes
      - nodes per `Lookup` history entry: mean, p95, max
- [ ] Re-measure with reduced constants to get a cost curve, one variable at a time:
      `HISTORY_LEN` (32, undo depth), `MAX_HISTORY` (50), `MIN_TYPED`/`MAX_TYPED` (256/576).
      Check the drill pass/fail output doesn't change — `dict-test`'s comparison is the
      regression signal that a smaller history broke translation.
- [ ] Device baseline: add `Request::HeapStats` / `Reply::HeapStats { used, free }` to
      `minder/src/lib.rs` and `jolt-embassy-rp/src/minder.rs`, reporting `HEAP.used()` /
      `HEAP.free()` (`jolt-embassy-rp/src/main.rs:67,226-230` already logs these over RTT every
      60s).  Read it from `keyminder` at steady state with the real dictionary set loaded.
      This is also the instrument for watching context churn in phases 2 and 3.
- [ ] Record `bbq-tool/user-dict.bin` size for the phase 3 budget: currently **7504 bytes**
      (`bbq-tool/dicts.bin` 4098768, `full.bin` 4105760 — the difference is close to it).
      Confirm whether the RAM overlay in phase 3 is a delta only, per that phase's design.

## Decision gate

    N contexts = (heap available - safety margin - RAM user-dict overlay) / per-context cost

**"Heap available" is not 64K.**  See the note below: growing the heap is a one-line change and
is very likely the right answer before shrinking any history constant.  Measure the per-context
cost first, then decide how big the heap needs to be — not the other way round.

Write the measured numbers into `tasks-minder.md` (the "N contexts and history sizes" open
question) before starting phase 2.  If N lands below ~4 *after* the heap has been sized to the
memory actually available, take the fallback recorded there: per-context `Joiner` only (it is
what carries cap/space state) with a single shared `Lookup` flushed on focus change.

## Notes

- **The 64K heap is a guess, not a constraint.**  `HEAP_SIZE = 65535`
  (`jolt-embassy-rp/src/main.rs:107-110`) is a static array picked at some point in the past;
  there is no reason for it to be smaller than the RAM actually left over.  The RP2040 has 264K
  of SRAM; `jolt-embassy-rp/memory.x` reserves `STACK_SIZE = 64K` for the MPU-guarded stack and
  leaves a 200K `RAM` region for statics.  A current debug build reports `bss = 76356`, and the
  heap array is 65535 of that — so roughly **11K of other statics and ~189K of the RAM region
  sitting unused**.  Treat the heap size as a dial to be turned once the per-context cost is
  known, and report the numbers as "bytes per context" rather than "contexts that fit in 64K".
  Shrinking `HISTORY_LEN` / `MAX_HISTORY` / `MIN_TYPED` costs undo depth and correction quality;
  raising `HEAP_SIZE` costs nothing that isn't already idle.
- Two things do bound how far the heap can grow, and both should be checked rather than assumed:
  the 64K stack reservation (also a guess — `_stack_end`/`_stack_start` in `memory.x`, with the
  stack guard installed at `main.rs:116`), and whatever headroom the allocator needs beyond
  steady-state live bytes.  The allocator is `embedded_alloc::LlffHeap` (linked-list first-fit,
  `main.rs:35-36`), which fragments; `TlsfHeap` is sitting commented out one line above if the
  measurements suggest it matters.  Peak live bytes from the harness is the input to that margin.
- The `HeapStats` addition is the only device change in phase 0 and is read-only; it can land
  as its own commit ahead of the protocol work in phase 1.  Bumping `VERSION` is phase 1b's
  job, not this one.
- Follow the repo commit conventions (CLAUDE.md): imperative present tense, body wrapped at 72
  columns, refactors separate from functional change.
