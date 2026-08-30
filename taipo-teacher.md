# Taipo Teacher: low-level key logging, and a trainer built on it

Two goals, sharing one mechanism.

1. **Know what real typing actually looks like.**  A host app collects, from the keyboard
   itself, the low-level detail of every Taipo/Posh chord written during normal work —
   per-key timing, which hand, which chord code — and works out what produces hesitations
   and what produces corrections.
2. **Drill the answer.**  A local Mac app, roughly keybr crossed with MonkeyType but
   Taipo/Posh-aware, that generates practice material weighted toward the trouble spots the
   first goal found — where a "trouble spot" may be a *sequence*, not a single letter — and
   that can score things no software-only trainer can see, including hand alternation and
   whether a gram with a chord was chorded or spelled out.

The two are one system: the drill material comes from the writer's own work rather than from
a corpus, because the passive collector and the trainer update the same model.

The steno plan this replaces is archived under [docs/archive/](docs/archive/); its phase 1
(long-poll event channel, `boot_id`, splitting `keyminder` into a library) is carried forward
here rather than shelved with it.

---

## Why this has to come from the device

Everything below is invisible to a host that only sees the HID stream, and most of it is the
whole point:

| What | Why the host can't see it |
|---|---|
| Which hand typed a letter | Both hands have the identical chord table; the HID report is the same either way |
| Which chord was pressed | Only the resulting keycode reaches the host |
| Per-key timing inside a chord | The chord is one report; the fingers landing over 60 ms look the same as landing together |
| A chord with no table entry | Types nothing at all — a pure error signal that produces no HID traffic whatsoever |
| Chorded vs spelled | `the` from the `ein` chord and `t`,`h`,`e` typed out are byte-identical to the host |
| How a chord ended | Timer expiry, all keys released, or the other hand starting — three quite different pieces of technique |
| Which table is live | Taipo or Posh; the device switches at runtime and the host is never told |

`docs/taipo-drills.md` already states the problem plainly: *"Nothing outside the keyboard can
see whether a word was chorded or spelled out, so the lists do the enforcing."*  The drills
are built the way they are because MonkeyType is blind.  This project removes the blindness,
and the drills stop having to be defensive.

## Decisions

Settled up front so the phases can assume them.

- **Log raw key events, not chords and not keystrokes.**  Scan code plus press/release plus a
  timestamp.  Chord assembly, the action taken, the hand, the termination reason, the split
  chords and the dead chords are all *derived* by replaying the log through the real
  `TaipoManager` on the host.  The device does the cheap part.  This mirrors the archived
  steno plan's "log strokes, not translations", and for the same reason: the derived forms are
  larger, lossier, and bake in whatever the table happened to be that day.
- **The replay is the actual engine.**  `bbq-keyboard` builds and tests on the host today
  (`bbq-keyboard/tests/taipo.rs` drives `TaipoManager` with a virtual clock).  The host tool
  uses the same code, not a reimplementation, so a firmware change can never silently
  invalidate the analysis.
- **Fixed 4-byte records, delta timing, RAM ring buffer, drop oldest.**  Reasoning carried over
  wholesale from the archived plan's phase 2; see "Record format" below for the one place the
  taipo case differs.
- **Logging is off at boot** and must be enabled by the host each session.  A keyboard with no
  host attached accumulates nothing.  This is a privacy decision, not a memory one.
- **Host app**: Swift/SwiftUI on macOS over a Rust core that owns the minder protocol, the
  replay, and the model.  `staticlib` + a small hand-written C ABI + cbindgen, per the archived
  plan's 1c.
- **One source of truth for the chord tables**: the Rust tables in `bbq-keyboard/src/layout/`,
  with a generated, checked-in JSON export for the Swift side.  Precedent is `bbq-consts`,
  which already extracts steno constants into a checked-in Rust file.
- **Nothing is synced anywhere.**  No account, no cloud, no leaderboard, no iOS.

## Current state, for reference

| Piece | Where | Notes |
|---|---|---|
| Chord engine | `bbq-keyboard/src/layout/taipo.rs` | `TaipoManager`, `SideManager` per hand, `CHORD_TIME = 100` |
| Posh table | `bbq-keyboard/src/layout/posh.rs` | `POSH_ACTIONS`; same engine, different table |
| Scan → (side, chord bit) | `taipo.rs`'s `SCAN_MAP` | plus `taipo_map` for the latch keys |
| Row-position shift | `LayoutManager::row_event` in `layout.rs` | rotates the three main-row scan codes on 3-row boards |
| Matrix scan | `jolt-embassy-rp/src/matrix.rs` | 1 ms full-matrix scan, `DEBOUNCE_COUNT = 20` |
| Right half | `jolt-embassy-rp/src/inter.rs` | passive MCU debounces, sets a bitmap, asserts IRQ; active side reads over I2C |
| Key dispatch | `jolt-embassy-rp/src/dispatch.rs` | `MatrixAction::handle_key` (local) and `active_task` (remote) both call `LayoutManager::handle_event` |
| Layout tick | `dispatch.rs:199` | `Ticker::every(1ms)`, `layout.tick(dispatch, 1)` |
| Protocol | `minder/src/lib.rs` | `Request`/`Reply` CBOR enums, `VERSION = "2024-11-01a"` |
| Device handler | `jolt-embassy-rp/src/minder.rs` | single sequential read → dispatch → write loop, `SIZE_LIMIT = 4200` |
| Host client | `keyminder/src/main.rs` | `VendorMinder::call()`, blocking, one request at a time |
| n-gram cost model | `words/ngrams.py` | segmentation cost, ease ranking; parses `taipo.rs` as text |
| Drill generation | `words/drills.py`, `docs/taipo-drills.md`, `docs/drills/` | MonkeyType custom-text lists |

**RAM is not scarce.**  From the archived phase 0 note: the RP2040 has 264K of SRAM,
`memory.x` leaves a 200K `RAM` region, and a build uses about 11K of it outside the 64K heap
array — roughly 189K idle.  The ring buffer is sized against that, not against `HEAP_SIZE`.

---

## Timing: what the numbers will actually mean

Worth settling before building anything that reports milliseconds, because two of these are
systematic and one is a genuine limitation.

- **Scan quantization, ±1 ms.**  The whole matrix is scanned once per millisecond.  This is the
  floor on resolution and is fine — chord spread is tens of milliseconds.
- **Debounce, +20 ms, constant.**  `DEBOUNCE_COUNT = 20` at one scan per millisecond delays
  every press and every release by 20 ms.  It is the same 20 ms for every key on both halves,
  so *intervals between keys are unaffected* and only absolute latency is shifted.  Nothing in
  the analysis depends on absolute latency.  (It does mean the keyboard has 20 ms of input lag,
  which is a separate conversation the log can inform.)
- **Left half, exact.**  The active MCU timestamps its own matrix events directly.
- **Right half, quantized to the I2C read.**  The passive MCU debounces, sets a bit in a
  bitmap, and asserts an IRQ; the active side then does a `ReadKeys` round trip.  A second key
  landing *during* that round trip is folded into the same bitmap, so two keys get one
  timestamp and their order is lost.  The round trip is short — a few bytes at 400 kHz — but
  the case where it matters is precisely the case being measured: the fingers of a right-hand
  chord landing within a millisecond or two of each other.

That last point is the one real accuracy limit, and phase 3 addresses it — measurement first,
then a fix only if the measurement says it matters.

---

## Phase 0: chord table export and a host replay harness

No device change.  This exists so that phases 4 and 5 have something to develop against before
any firmware lands, using synthetic logs.

- [ ] **Machine-readable chord table.**  A generator that emits `layouts.json` from
      `TAIPO_ACTIONS` and `POSH_ACTIONS`: per variant, per chord code, the finger keys involved
      (named as in `TAIPO.md`: `a o t e` bottom, `r s n i` top, plus `Sp`/`Bk`), the action
      (single key, shifted key, text, one-shot modifier, release), and for `Action::Text` the
      string.  Also the `SCAN_MAP` in both row positions and for each supported board, so a
      scan code can be resolved to (hand, finger, row).
      - Prefer a `bbq-keyboard` example or test binary that walks the real tables over another
        text parser; `words/ngrams.py` parses `taipo.rs` as text and can stay as it is, but the
        JSON the app depends on should come from the compiler.
      - Check the JSON in, with a test that fails when it is stale — the `bbq-consts` pattern.
- [ ] **Replay harness.**  A host library that takes a sequence of `(scan code, press/release,
      time)` and drives a real `TaipoManager` through it, advancing the virtual clock with
      `tick()` exactly as `bbq-keyboard/tests/taipo.rs` already does, and emitting a stream of
      derived events: chord committed (code, hand, action, first-key time, last-key time,
      termination reason), key action sent, modifier change, dead chord, split chord.
      - `LayoutManager::row_event` and the variant toggle are part of the replay, so mode,
        variant and row position come out of it rather than being tracked separately.
- [ ] **Synthetic log generator** for tests: turn a target string into the key events an ideal
      writer would produce, with configurable sloppiness (chord spread, same-hand runs,
      misfingerings, corrections).  This is what phase 4's analysis is unit-tested against, and
      it is the only way to have known ground truth.

## Phase 1: protocol groundwork

Lifted from the archived plan; nothing here is taipo-specific.

### 1a. Device push

The device needs to hand over log records without the host having to poll tightly.

- [ ] `Request::GetEvent { timeout_ms }` → `Reply::Event { .. }` / `Reply::NoEvent`, with at
      most one outstanding at a time.  The firmware's minder loop becomes
      `select(reader.read(), event_channel.receive())`, and a request arriving while a
      `GetEvent` is pending is answered by replying `NoEvent` first and then handling the
      request normally.  That is what makes it work without request tags: ordering on the
      single bulk IN pipe stays unambiguous.
- [ ] The log drain rides this: the device raises an event when the buffer crosses a
      watermark, and the host answers with `GetEventLog`.  A trainer that wants low latency
      sets the watermark to one record and gets records within a USB transaction.
- [ ] Flash program/erase on the RP2040 blocks with interrupts masked; the sequential loop
      already prevents a pending `GetEvent` from firing inside that window, but it stays true
      only while the loop is sequential.

### 1b. Version and identity

- [ ] Bump `VERSION` in `minder/src/lib.rs`.
- [ ] Add a capability list to `Reply::Hello`, so a new host against old firmware degrades
      rather than hanging on an unanswered request.
- [ ] Add `boot_id` to `Reply::Hello` — changes every boot.  A log stream is only continuous
      within one `boot_id`.
- [ ] Add a **layout table hash** to `Reply::Hello`: a hash over `TAIPO_ACTIONS` +
      `POSH_ACTIONS` + `SCAN_MAP`, computed at build time.  Replay against a table the firmware
      no longer has must be *detectable* rather than quietly wrong.  Same argument as the
      archived plan's dictionary hash.

### 1c. `keyminder` as a library

- [ ] Move `VendorMinder`, `Flasher`, `FlashImage` out of `keyminder/src/main.rs` into a
      library (`keyminder/src/lib.rs`, or a `minder-host` crate if the CLI should stay thin).
      Pure refactor, its own commit.
- [ ] Give it a connection object owning the long-poll loop, handing events to a callback.
- [ ] Note for packaging: the app talks to a vendor-specific USB interface via libusb, so it
      must be unsandboxed or carry `com.apple.security.device.usb`.

## Phase 2: the device-side event log

### Record format

Four bytes, fixed stride:

```
byte 0   tag
byte 1   dt, low byte
byte 2   dt, high byte
byte 3   aux
```

- `tag` bit 7 clear → **key event**.  Bit 6 is press (1) / release (0); bits 5..0 are the
  physical scan code.  Codes run 0..47 on the split boards (0..23 left, 24..47 right) and 0..29
  on the proto4, so six bits is enough with room to spare.
- `tag` bit 7 set → **marker**, kind in bits 6..0: `Mode`, `Variant`, `RowShift`, `Resume`,
  `Pause`.  `aux` carries the new value.  Markers exist so that a *partial* drain is
  self-describing: without them, a replay that starts after the buffer dropped its oldest
  entries would not know which table to look chords up in.
- `dt` is the time since the previous record.  **If bit 15 is clear, the low 15 bits are
  milliseconds (0..32.7 s); if set, they are seconds (0..9.1 h).**  This keeps the fixed stride
  and needs no escape record, while spending resolution only on gaps where nobody cares about
  the millisecond.  It is the one place this differs from the archived steno design, which
  could afford a `u32` delta because its records were 8 bytes and far rarer.
- `aux` is unused (zero) on key events, and reserved.

**Why a delta and not an uptime**, restated because it is the load-bearing choice: an absolute
`u32` millisecond uptime wraps at 49.7 days and a wrapped entry does not look wrong, it looks
recent.  A delta is bounded by human behaviour instead, and a saturated one is self-evidently
an enormous gap.  Overflow becomes impossible by construction rather than handled correctly
everywhere.

**Volume.**  A chord is two records per key, so an average two-key chord is four records, 16
bytes per character.  At 200 characters a minute that is about 3 KB/min, 190 KB/hour of active
typing — nothing on disk, and it means a 64K ring buffer holds roughly 20 minutes of solid
typing.  Size it from that, against the ~189K of idle RAM rather than against the heap.

### The hook

Both paths into the layout — `MatrixAction::handle_key` for the local half and `active_task`
for the remote half — call `LayoutManager::handle_event` in `jolt-embassy-rp/src/dispatch.rs`.
Log at those two call sites (or via one small helper on `Dispatch` that both use), where
`Instant::now()` is available and `bbq-keyboard` stays time-free and `no_std`.

- Logging must never block or drop a *key*.  A full buffer drops the oldest **record** and
  bumps a counter; the key being typed is never affected.
- The markers are emitted from the places that already know: `LayoutActions::set_mode`, the
  variant toggle, and the row-shift toggle.

### Tasks

- [ ] `minder/src/lib.rs`: `Request::SetLogging { enabled }`, `Request::GetEventLog {
      max_bytes }` → `Reply::EventLog { boot_id, seq, dropped, anchor_ms, records }`, and
      `Request::EventLogAck { through_seq }`.  Explicit acking rather than
      discarding-on-read, so a host crash mid-transfer loses nothing.  Watch `SIZE_LIMIT =
      4200` in `jolt-embassy-rp/src/minder.rs`.
- [ ] `anchor_ms` is the time since the *most recent* record in the reply.  The host stamps
      that record with "now minus `anchor_ms`" and walks backwards subtracting deltas.
      Anchoring at the new end rather than the old one means dropped entries at the far end
      cost nothing, and it needs no epoch and no wrapping arithmetic.
- [ ] `jolt-embassy-rp`: the ring buffer, the two hook sites, the markers, logging disabled at
      boot and gated on `SetLogging`.
- [ ] `dropped` in the reply, not as a record: drop-oldest always drops at the tail, so the gap
      is always at the start of a drained run and its position is unambiguous.
- [ ] `keyminder log`: a subcommand that enables logging, drains, and appends to a file.  Once
      1c lands this becomes a background loop in the host core rather than a one-shot command.
- [ ] **On-disk format.**  One record per line, text: timestamp, `+`/`-`, hand and key name
      (`L.n`, `R.Sp`), with `#` comment lines for session headers (boot id, board, table hash,
      wall clock anchor) and gap markers.  Greppable, diffable, cheap to append; a binary
      format saves space this corpus does not need.  Rotate daily.
- [ ] Confirm the layout tick still costs what it did; the hook is on the hot path.

## Phase 3: right-hand timing accuracy

Measure first.  The fix is real work and may not be needed.

- [ ] **Measure the collapse rate.**  In `inter.rs`'s `active_task`, count reads whose bitmap
      delta contains more than one changed bit, and report it (over minder, alongside the log,
      or just over RTT to start).  Type normally for a while.  If multi-bit reads are rare, the
      remaining error is one I2C round trip and can simply be documented.
- [ ] If they are not rare: **timestamp on the passive side.**  Replace (or supplement)
      `Request::ReadKeys` with `Request::ReadEvents`, returning a small FIFO of
      `(code, press, age_ms)` where `age_ms` is measured at reply time on the passive MCU.  The
      active side rebases each onto its own clock as `now - age`.  This preserves both order
      and inter-key intervals within the right hand, and the FIFO also removes the existing
      bitmap's inability to represent a press and release of the same key between two reads.
      - Keep `ReadKeys` working, so an old passive half against a new active half still types.
      - Note the passive half is also subject to its own 20 ms debounce, which is the same 20
        ms, so the halves stay comparable.
- [ ] Either way, record the residual skew in this file, because every "which hand was
      faster" number the app reports depends on it.

## Phase 4: analysis

A Rust CLI over the phase 0 replay, reading phase 2 logs.  This is the deliverable for goal 1,
and its output is what the Mac app's model consumes — the app should not be the only way to
see any of this.

### What it derives

Per chord committed: the code, the hand, the variant, the action, the time the first key
landed, the time the last key landed (**spread** — how much the chord was assembled rather
than struck), the time it was committed, and how it ended (all keys released / timer expired /
other hand started).

### What it looks for

- [ ] **Corrections.**  A `Bk` chord, and what followed it.  Reconstruct what was deleted and
      what replaced it.  Classify: was the replacement one key different from what was sent
      (a misfingering), a different chord entirely (the wrong chord was recalled), or the same
      chord again (a split or dropped chord)?  The bit patterns make this mechanical.
- [ ] **Dead chords.**  A chord code with no table entry types nothing; the writer sees a
      missing letter and corrects.  These are unambiguous errors and are invisible to any
      host-side approach.
- [ ] **Split chords.**  A chord committed by timer expiry, immediately followed on the same
      hand by keys that plausibly belonged with it.  This is the `CHORD_TIME = 100` window
      being hit, and it is a tuning signal for that constant as much as a technique signal.
- [ ] **Hand alternation.**  Every letter is available on both hands, so consecutive chords on
      the same hand are always avoidable.  Report the same-hand run rate overall, per bigram,
      and over time.  Treat it as a tracked metric with a tunable weight rather than as a hard
      error until the data says what a good rate actually looks like.
- [ ] **Spelled-out grams.**  A run of single-letter chords whose text has a chord in the
      table.  Directly measures whether the 18 `Action::Text` entries are being used at all,
      which the drills currently cannot check.
- [ ] **Hesitations.**  Inter-chord interval well above that *transition's* own baseline —
      the pause is the signal, and it needs the sequence context to be meaningful.  Not an
      error, and the more interesting category, since it finds what is not yet automatic
      without needing anything to go wrong.
- [ ] **Ranking.**  Weight by how often the item actually occurs in this writer's own text.
      Something missed twice out of two matters less than something missed ten times out of
      fifty.

### Model store

- [ ] Items are chords *and chord transitions* — the sequence case the whole thing is for.
      Trigrams only for transitions that already look bad, to keep the state space sparse.
- [ ] Per item: exponentially-weighted mean and variance of the interval, error count,
      exposure count, last seen.  Keyed by `(variant, item)`, since Taipo and Posh are
      different skills.
- [ ] SQLite in `~/Library/Application Support/`, written by the Rust core, readable by the
      CLI.  One store, two front ends.
- [ ] Port the segmentation cost model from `words/ngrams.py` into this crate: the app needs
      "what is the cheapest chord sequence for this word" at runtime, to know what a word
      *should* have been typed as.  `words/ngrams.py` stays as the research tool; it does not
      need to change.

## Phase 5: the Mac app

SwiftUI, menu bar plus a main window, over the Rust core.

### 5a. Shell and passive collection — goal 1, standalone

The app is useful before any drill exists.

- [ ] Menu-bar item: connection state, recording on / off / paused, today's numbers.
- [ ] Background: connect, enable logging, drain continuously, append to the log file, update
      the model.
- [ ] Reset detection: the long-poll fails on USB re-enumeration; `boot_id` confirms it.  Mark
      a discontinuity in the log and re-enable logging.
- [ ] Table-hash mismatch: warn loudly and stop deriving, rather than deriving wrongly.
- [ ] Stats window: the phase 4 analysis, rendered.  Chord heatmap over both tables, per-finger
      breakdown, the transition matrix, alternation rate over time, corrections and hesitations
      ranked, "where the time actually goes".

### 5b. The trainer

- [ ] **Input.**  Render what is typed from the app's own key events, so there is zero added
      latency — a trainer with laggy echo is unusable.  Annotate from the log, which arrives
      milliseconds later.  Alignment is not a general stream-joining problem, because during a
      drill both streams are matched independently against the *known target text*; the target
      is the join.  If alignment drifts, drop the annotations for that test rather than
      mis-attributing them.
- [ ] **Modes**:
      - *Adaptive practice* (keybr-shaped): no fixed lesson.  Sample words from
        `words/english_10k.json` such that the word's cheapest segmentation exercises the
        current weak items, and unlock new items as old ones pass a speed and accuracy
        threshold.
      - *Test* (MonkeyType-shaped): timed or word-count, results screen.
      - *Focus*: pick one item and drill its neighbourhoods — the "increase the amount of
        various troublespots" dial, made explicit.
      - *Lessons*: `docs/drills/` as native lessons, but now able to enforce what those lists
        could only encourage, since the app can see whether the gram was chorded.
- [ ] **Scoring.**  Chords per minute alongside WPM — for a chorded layout, CPM is the honest
      number.  Accuracy split by the taxonomy in phase 4: wrong chord, dead chord, split chord,
      spelled-out gram, same-hand run, correction.  A correction counts as an error even when
      the final text is right, and alternation is judged **across** corrections rather than
      exempting them — hitting `Bk` on the same hand as the letter it deletes is the same
      technique fault as any other same-hand run.
- [ ] **Chord hints.**  Two hands drawn, the next chord's keys lit, fading out as the item is
      learned — keybr's idea, and much more necessary here, since there is nothing on the keys.
      Live technique strip under the text: an L/R bead per chord, and a spread bar per chord.
- [ ] **Variant awareness.**  Read the active variant from the log's `Variant` marker and
      follow it, so a drill always matches what the keyboard is actually doing.  The model is
      per-variant, which incidentally makes Taipo-vs-Posh comparable.
- [ ] **Bridge.**  Keep the C ABI small: connect / disconnect, start-stop logging, a callback
      delivering derived events, model queries, material generation, session results.  cbindgen
      for the header.  `uniffi` is more machinery than this needs unless the surface grows.

### 5c. Rough order

Shell + connection → passive collector + log file → stats window → test mode → adaptive
practice → hints and technique strip → lessons.  Each is usable on its own.

## Phase 6: Zephyr parity

- [ ] Port the event log to `jolt`, per the repo convention that `jolt` is not a full
      replacement until it has what `jolt-embassy-rp` has.  Deliberately last: the plan is not
      to grow two implementations of an unproven format.

---

## Privacy

**This is a keylogger.**  Said plainly, because it is, and because the chord codes *are* the
letters — there is no redaction that keeps the data useful.

- Off at boot on the device, enabled by the host per session, RAM only, lost on power off.
- The host file is local, plaintext, and synced nowhere by this project.
- A visible menu-bar recording indicator, and a pause that takes effect on the device rather
  than only in the app.
- Auto-pause on macOS secure input (`IsSecureEventInputEnabled()`), which is what password
  fields assert.  This is the one that actually matters.
- An app blocklist, once there is a way to know the focused app.  The archived plan's window
  tracking (`NSWorkspace` `didActivateApplicationNotification`, bundle-ID granularity, no
  permission prompt) is the cheap way in, and tagging each chord with the focused app is
  independently valuable — errors in prose and errors in code are probably worth separating.

## Open questions

- **Is a same-hand run really an error?**  It is always avoidable, but "avoidable" is not
  "wrong", and some sequences may be genuinely fine or even faster.  Ship it as a metric with
  a tunable weight and let the first month of real data set the default.
- **How much buffer?**  Falls out of how long the host is realistically detached.  16 bytes a
  character means a 64K ring is about 20 minutes of solid typing; if the app is running all
  the time, far less would do.
- **Is `CHORD_TIME = 100` right?**  It has been a "starting guess, not a measured value" since
  it was set.  The split-chord rate from phase 4 is the measurement that was missing.
- **Does the 20 ms debounce need to be 20 ms?**  Not this project's question, but this project
  is the instrument for asking it.
- **What does a correction actually look like?**  The taxonomy in phase 4 is a guess at the
  categories; the first real corpus will say whether they are the right ones.
- **Does the trainer need to drive the keyboard at all** — forcing a variant, or suppressing
  steno mode — or is reading enough?  Probably reading; revisit when the drill modes exist.
- **How much of `words/ngrams.py` should move to Rust?**  The segmentation cost model must, for
  the app.  The ease ranking and the corpus work probably should not.

## Testing notes

Per the repo convention, changes are tested during review rather than before commit, and
refactors land separately from functional change.  The refactors here that should be their own
commits: splitting `keyminder` into a library (1c), and any `inter.rs` protocol change (3).

Hardware-only checks, collected:

- long-poll round trip and preemption: issue `GetEvent`, then immediately a `Hash`, confirm
  `NoEvent` then `Hash` arrive in order (1a)
- log fidelity: type a known passage, drain, replay, confirm it reproduces exactly what was
  typed — including a deliberate misfingering, a dead chord, and a `Bk` correction (2)
- gap handling: type with the host detached long enough to overflow the buffer, confirm the
  gap appears in the file and the keyboard itself never stutters (2)
- markers: switch variant, mode and row position mid-session, confirm a drain that begins
  after a drop still replays correctly (2)
- hot path: confirm the 1 ms layout tick and the matrix scan are unaffected with logging on (2)
- right-hand collapse rate, typing normally (3)
- reset: unplug and replug mid-session, confirm the discontinuity is marked and logging
  resumes (5a)
- secure input: focus a password field, confirm recording pauses on the device (5a)
