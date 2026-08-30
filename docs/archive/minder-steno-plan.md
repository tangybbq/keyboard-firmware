> **Shelved, 2026-08-30.**  Steno is no longer being pursued, so this plan is
> kept for reference rather than execution.  It was the working plan for the
> minder protocol, written when window tracking and steno stroke logging were
> the goal.
>
> Three parts of it were carried forward into `taipo-teacher.md`, which is the
> live plan now, and are *not* superseded by being here:
>
> - **Phase 1a**, the long-poll `GetEvent` channel — the device-push mechanism
>   the taipo event log streams over.
> - **Phase 1b**, `boot_id` and capability negotiation — needed to mark a log
>   discontinuity across a reset.
> - **Phase 1c**, splitting `keyminder` into a library — still worth doing, though
>   for the collector's own sake rather than for the Swift bridge this plan
>   recommended.  Without a steno dictionary to bridge to, the Mac app speaks
>   minder itself and the two host implementations are kept in step by golden
>   byte vectors instead of by shared code.
>
> The reasoning about ring buffers, delta-vs-absolute timestamps, drop-oldest
> accounting, and anchoring wall clock at the newest entry (phase 2) applies
> unchanged to key events and is reused rather than re-derived.  Everything
> about `Lookup`/`Joiner`, per-window contexts, the user dictionary, and the
> lookup modal is shelved with steno.

# Minder improvements

Summary: The minder protocol currently supports dictionary updates, and some simple logging.

We want to extend this with:
- Window tracking. A MacOS app monitors which window has focus, and informs the firmware on focus
  changes.  The firmware can maintain seperate steno state for each, allownig caps next and space
  next to be tracked.
- Lookup. A stroke(s) can bring up a modal to query for a word and the strokes. It should allow
  testing in the dialog, and a way to make exercises for srs drilling.
- User dictionary. Allow the dictionary to be extended and maybe edited from a modal.
- Stroke logging. Log every steno stroke written, so the real writing -- misstrokes, corrections
  and all -- can be gathered as a personal corpus, mined for what is actually worth drilling, and
  fed to the steno-flow project.

The last will require a bit more:
- The user dictionary should be loaded into RAM on start. The Mac program should detect a reset on
  the keyboard, and load an up to date one. It should all entries to be added dynamically, and when
  requested, update the flashed user dictionary.

---

## Decisions

Settled up front, so the phases below can assume them:

- **Host app**: a Swift/SwiftUI menu-bar app for focus tracking and the modals, over a Rust core
  that owns the minder protocol.  The protocol definitions stay in the `minder` crate; they are not
  reimplemented in Swift.
- **Device push**: a long-poll `GetEvent` request.  The strict request/reply model is kept.
- **Per-window state**: a full `Lookup` + `Joiner` per context, with the history sizes reduced to
  fit.  How many contexts, and how small the histories can get, is decided by measurement
  (phase 0), not guessed.
- **Lookup dictionary search**: host side, from `bbq-tool/full.bin`.  Reverse lookup
  (word -> strokes) is a linear scan the RP2040 should not be doing.
- **User dictionary**: the host is the master.  The device never invents entries.  The flashed copy
  exists so the keyboard is useful before the Mac app connects, and on a machine that isn't running
  it at all.
- **Stroke logging**: log raw strokes, not translations.  Everything else is derivable by
  replaying them through the same dictionary on the host, so the device does the cheap part.
  Buffered in RAM and drained by the host; nothing is written to flash.
- **SRS**: no new scheduling/drilling store.  Exercises feed the sibling `steno-flow` project.

## Current state, for reference

| Piece | Where | Notes |
|---|---|---|
| Protocol | `minder/src/lib.rs` | `Request`/`Reply` CBOR enums, `VERSION = "2024-11-01a"` |
| Transports | `minder/src/{encode,decode}/{hid,serial}.rs` | plus `cobs.rs`; USB bulk is framed by the endpoint itself |
| Device handler | `jolt-embassy-rp/src/minder.rs` | single sequential read -> dispatch -> write loop |
| USB wiring | `jolt-embassy-rp/src/usb.rs:66` | vendor interface 0xFF, one bulk IN + one bulk OUT, 64 bytes |
| Host client | `keyminder/src/main.rs` | `VendorMinder::call()`, blocking, one request at a time |
| Steno engine | `bbq-keyboard/src/dict.rs` | one `Lookup` + one `Joiner`, dict bases hardcoded at `0x10300000` (main) and `0x10200000` (user) |
| Steno task | `jolt-embassy-rp/src/main.rs:176` | lowest-priority executor, fed by `STROKE_QUEUE` (`Channel<Stroke, 10>`) |
| Heap | `jolt-embassy-rp/src/main.rs:108` | `HEAP_SIZE = 65535`; `heap_stats()` logs used/free every 60s |

Flashed dictionaries are XIP-mapped and cost no RAM.  Anything moved into RAM competes with the
window contexts inside the heap.

**The 64K heap is a guess, not a hardware limit.**  It is a static array chosen at some point in
the past, and there is no reason for it to be smaller than the RAM actually left over.  The RP2040
has 264K of SRAM; `jolt-embassy-rp/memory.x` reserves 64K for the stack and leaves a 200K `RAM`
region, of which a current build uses about 11K for non-heap statics -- roughly 189K unused.  So
the phase 0 output is "bytes per context", and the heap is sized to fit the contexts wanted, not
the other way round.  Raising `HEAP_SIZE` is a one-line change and should be preferred over
shrinking `HISTORY_LEN` / `MAX_HISTORY` / `MIN_TYPED`, which cost undo depth and correction
quality.  (The 64K stack reservation is equally a guess, and is the other dial if it comes to it.)

---

## Phase 0: memory profiling

This phase exists because two open questions ("how many window contexts?" and "how much can the
history shrink?") both need numbers, and both are cheap to get on the host.

### What one context actually costs

From the code, per context:

- `Joiner.typed`: a `String` bounded by `MAX_TYPED = 576` (`MIN_TYPED * 2 + 64`), shrunk back to
  `MIN_TYPED = 256` when it overflows.
- `Joiner.history`: `VecDeque<Add>`, `MAX_HISTORY = 50`.  `Add` is 32 bytes inline
  (`remove: usize`, two `String`s, `State`) plus two heap allocations for typically short strings.
- `Joiner.actions`: drained promptly by the steno task, negligible.
- `Lookup.history`: `heapless::Deque<Entry, 32>`, so `HISTORY_LEN * size_of::<Vec>()` = 384 bytes
  inline, plus a `Vec<Box<dyn Selector>>` per entry.  Each `BinarySelector` is 20 bytes boxed
  (`Rc<dyn DictImpl>` fat pointer + three `usize`).
- The dictionaries themselves are `Rc<dyn DictImpl>` and shared, so a second context adds nothing
  for them.

The unknown is the **node count per `Lookup` history entry** -- the number of surviving partial NFA
matches.  `add_stroke` seeds a fresh epsilon selector per dictionary on every stroke, so this grows
with the number of loaded dictionaries as well as with genuinely ambiguous prefixes.

### Tasks

- [ ] Add a tracking global allocator to a host harness and report live/peak bytes.  `typey` and
      `dict-test` both build a `Lookup` + `Joiner` over `../bbq-tool/full.bin`, but neither works
      unmodified: `dict-test` builds a *fresh* pair per drill entry (a few strokes each, so
      nothing reaches steady state) and `typey write` reads strokes interactively from a raw
      terminal.  Write a non-interactive driver with one persistent context fed the whole drill
      corpus.  See `NEXT.md` for the hand-off.
- [ ] Feed it a realistic stroke corpus (long enough for `typed` to reach steady state and for the
      `Lookup` deque to fill), and record:
      - steady-state live bytes attributable to one context
      - peak live bytes
      - the distribution of nodes per `Lookup` history entry (mean, p95, max)
- [ ] Re-measure with reduced constants to get a cost curve:
      - `HISTORY_LEN` (lookup.rs, currently 32) -- this is undo depth
      - `MAX_HISTORY` (joiner.rs, currently 50)
      - `MIN_TYPED` / `MAX_TYPED` (joiner.rs, currently 256 / 576)
- [ ] Get the on-device baseline: what `HEAP.free()` reports at steady state today, with the real
      dictionary set loaded.  `heap_stats()` already prints this; add a `Request::HeapStats` reply
      so it can be read over minder rather than RTT (also useful later for watching context churn).
- [ ] Measure `bbq-tool/user-dict.bin` size, for the phase 4 budget.

### Decision gate

`N contexts = (heap available - safety margin - RAM user-dict overlay) / per-context cost`

"Heap available" is a decision, not a measurement -- see the note above.  Size the heap to the
contexts wanted, and only then check the total against the ~189K of unused RAM.

Write the resulting numbers into this file before starting phase 3.  If N lands below ~4 *after*
the heap has been sized to the memory actually available, revisit: the fallback is per-context
`Joiner` (cheap, and it is what carries cap/space state) with a single shared `Lookup` flushed on
focus change.

---

## Phase 1: protocol groundwork

### 1a. Long-poll event channel

The device needs to say "open the lookup modal" without breaking the host-initiated model.

Design:

- New `Request::GetEvent { timeout_ms: u32 }`.
- New `Reply::Event { .. }` and `Reply::NoEvent`.
- The host keeps **at most one** `GetEvent` outstanding.
- The firmware's minder loop becomes `select(reader.read(), event_channel.receive())`:
  - event wins -> reply `Event`
  - timeout -> reply `NoEvent`
  - **reader wins while a `GetEvent` is pending** -> reply `NoEvent` immediately, then handle the
    new request normally

That last case is what makes this work without request tags.  The host never has to wait out a poll
timeout to send something latency-sensitive; it just sends, and expects a `NoEvent` followed by the
real reply.  Ordering on the single bulk IN pipe stays unambiguous, so no sequence numbers are
needed.  Poll timeout can then be long (seconds) rather than tuned against focus-change latency.

- [ ] Add the requests/replies to `minder/src/lib.rs`, with an event payload enum that starts with
      just the lookup-modal trigger.
- [ ] Implement the `select` in `jolt-embassy-rp/src/minder.rs::main_loop`, with a small bounded
      event queue (dropping oldest on overflow -- events are UI triggers, not data).
- [ ] Verify the round trip on hardware, including the preemption case (issue `GetEvent`, then
      immediately a `Hash`, confirm `NoEvent` then `Hash` arrive in order).

Note: flash program/erase on the RP2040 blocks with interrupts masked (see the comment in
`minder.rs::program`).  A pending `GetEvent` must not be able to fire during that window -- with the
sequential loop above it can't, but keep it in mind if the loop is ever made concurrent.

### 1b. Versioning and capability negotiation

- [ ] Bump `VERSION` in `minder/src/lib.rs`.
- [ ] Add a feature list (or bitmask) to `Reply::Hello` so a new host against old firmware degrades
      instead of hanging on an unanswered `GetEvent`.  minicbor's numbered fields already tolerate
      unknown variants; the explicit list is about the host knowing what to *use*.
- [ ] Add a `boot_id` to `Reply::Hello` -- a value that changes on every boot.  This is the cheap
      half of reset detection for phase 4.

### 1c. Split keyminder into a library

The Swift app and the CLI need the same code.

- [ ] Move `VendorMinder`, `Flasher`, `FlashImage` out of `keyminder/src/main.rs` into
      `keyminder/src/lib.rs` (or a new `minder-host` crate if keyminder-the-CLI should stay thin).
- [ ] Give it a connection object that owns the long-poll loop and hands events to a callback.
- [ ] Decide the Swift bridge.  Recommendation: build the core as a `staticlib` with a small
      hand-written C ABI (connect, set_context, poll_event, user-dict ops, flash ops) plus a
      cbindgen header.  The surface is small enough that `uniffi` is more machinery than it's worth,
      but revisit if it grows.  A local-socket daemon is the other option and is easier to debug,
      at the cost of a second process to supervise.
- [ ] Note for packaging: the app talks to a vendor-specific interface via libusb.  It must either
      be unsandboxed or carry the `com.apple.security.device.usb` entitlement.

---

## Phase 2: stroke logging

Log every steno stroke written on the keyboard, so the actual writing -- including the misstrokes
and corrections -- can be studied later.  The goal is to find out **what is worth drilling**, from
what actually goes wrong in real writing rather than from what a drill file guesses; the corpus
also feeds the sibling `steno-flow` project.  This is independent of window tracking; it is placed here only because the corpus accrues from the day it
lands, so there is value in it going in early.  Reorder freely against phase 3.

`TASKS.md` also carries this as "Steno stroke logging (corpus collection for ML)"; that entry is
the summary, this section is the plan.

### Log raw strokes, not translations

The stroke sequence is the primitive.  Everything else -- what was typed, what got backspaced,
where an undo happened -- is derivable by replaying the strokes through the same dictionary on the
host, which is exactly what `dict-test` and `typey` already do.  Logging translations instead
would be larger, lossier, and would bake in whatever the dictionary happened to be that day.

Consequences to keep in mind:

- **Derivation needs the dictionary that produced it.**  Record a session header carrying the
  `boot_id` (phase 1b) and the dictionary hash, so a replay against a since-changed dictionary is
  detectable rather than silently wrong.  `Request::Hash` already exists for the flashed region.
- **The undo stroke is data, not noise.**  `*` is a stroke like any other and must be logged
  verbatim; it is the clearest correction signal there is.
- **The steno delay makes this more valuable, not less.**  Now that dictionary corrections
  coalesce before being typed (`bbq-keyboard/src/steno_delay.rs`), many corrections never reach
  the host as keystrokes at all.  A keystroke-level log would have lost them; the stroke log
  still has them.

### The hook point

Every stroke the layout resolves goes through one place: `LayoutActions::send_raw_steno`
(`jolt-embassy-rp/src/dispatch.rs:306`), which pushes onto the stroke channel.  Logging there
catches everything, including strokes written while `RA*U` raw mode is on (the toggle is
downstream, in `bbq-keyboard/src/dict.rs:52`).

`LayoutMode::StenoDirect` is `todo!()` (`dispatch.rs:264,283`) and is not expected to be
implemented: what it was meant to do is what `RA*U` raw mode already does.  It needs no logging
support, and is a candidate for removal rather than a hook point to preserve.

### Buffer in RAM, drop oldest, record the gap

The host will not always be attached, so the device buffers.  Start with a RAM ring buffer and no
flash persistence: flash program/erase on the RP2040 blocks with interrupts masked (see the
comment in `minder.rs::program`), and the wear and complexity are not worth it for data whose
worst case is losing a few minutes of strokes.

- **Fixed-size 8-byte entries: `(stroke: u32, delta_ms: u32)`, where the delta is the time since
  the previous logged stroke.**  A `Stroke` is a `u32` (`bbq-steno/src/stroke.rs:35`), and the
  delta is computed in `u64` and saturated into the `u32` field.
- **Log the delta, not the uptime.**  This is a wraparound argument, and it is the whole reason
  for the choice.  `embassy_time::Instant` holds `u64` ticks (`embassy-time/src/instant.rs:9`) and
  `as_millis()` returns `u64`, so the device's clock does not wrap in any timeframe worth
  discussing -- any wrap is introduced by *us*, when the value is narrowed to fit an entry.
  The two candidates narrow very differently:

  - An absolute `u32` millisecond uptime wraps at 49.7 days.  The keyboard genuinely can be left
    powered that long once the firmware settles, and the failure is the bad kind: a wrapped entry
    does not look wrong, it looks recent.  Every consumer then has to be written in terms of
    `wrapping_sub` with a documented "no two points of interest are more than 49.7 days apart"
    precondition -- which the ring buffer itself can violate, since a rarely used keyboard with
    the host detached can hold entries older than that.
  - A delta is bounded by human behaviour rather than by uptime.  Consecutive strokes are
    milliseconds apart while writing and hours apart across sessions; 49.7 days between two
    strokes is not a thing that happens, and if it somehow does, the saturated value is
    self-evidently "an enormous gap" rather than a plausible small number.  Overflow becomes
    impossible-by-construction instead of handled-correctly-everywhere.

  The delta also costs no more space, keeps the fixed stride, and needs only one `u64` of device
  state (the previous stroke's tick).
- **Keep the fixed stride.**  Do not pack the delta down to a `u16` (65.5s) with escape records
  for longer gaps: a fixed stride makes the ring buffer trivial -- index it, drop-oldest is a
  pointer bump, a partial drain needs no resynchronisation -- whereas variable-length records need
  framing and a recovery story for each of those.  The heap note above established that RAM is not
  the scarce resource; simplicity is worth more than two bytes an entry.
- **Keep the timestamps at all**, in whatever form: how long the writer paused before hitting `*`
  is a large part of what separates "misstroke, corrected immediately" from "wrote it, read it
  back, decided against it" -- and the second is the more interesting one for drill selection.
- **Anchor to wall clock at the new end of the buffer.**  There is no RTC, so absolute time has to
  come from the host at drain.  Have the drain reply carry the time since the *most recent* logged
  stroke; the host stamps that entry with "now minus that", then walks backwards subtracting
  deltas.  Anchoring at the newest end rather than the oldest means dropped entries at the far end
  cost nothing, and it needs no epoch, no uptime field, and no wrapping arithmetic.  The session
  header's `boot_id` marks where the delta chain restarts.
- On buffer overflow, drop the oldest and bump a counter; report the count in the drain reply so
  the host records a gap rather than assuming continuity.  The delta chain survives this, because
  reconstruction runs backwards from the anchor.
- Size it from the phase 0 numbers.  At 8 bytes an entry, 16K holds ~2000 strokes and 32K holds
  ~4000 -- and the heap note above says there is far more room than the current 64K implies.  This
  is not where the memory pressure is.

### Tasks

- [ ] `minder/src/lib.rs`: add `Request::GetStrokeLog { max_bytes }` -> `Reply::StrokeLog { seq,
      dropped, entries }`, plus `Request::StrokeLogAck { through_seq }`.  Explicit acking (rather
      than the device discarding on read) means a host crash mid-transfer does not lose data.
      Watch `SIZE_LIMIT = 4200` in `jolt-embassy-rp/src/minder.rs`.
- [ ] `jolt-embassy-rp`: the ring buffer, fed from `send_raw_steno`, drained by the minder
      handler.  Logging must never block or drop a stroke on the keyboard path -- a full buffer
      drops the oldest logged entry, never the stroke being written.
- [ ] `jolt-embassy-rp`: session header on boot (`boot_id`, dictionary hash) so replay can be
      validated.
- [ ] `keyminder`: a subcommand that drains the log and appends to a file on the host, in a format
      that is trivially appendable and replayable.  Once phase 1c lands, this becomes a background
      loop in the host core rather than a one-shot command.
- [ ] Decide and document the on-disk format.  Suggestion: one line per stroke in RTF/CRE-ish
      steno notation plus a millisecond offset, with `#` comment lines for session headers and gap
      markers -- greppable, diffable, and cheap to append.  A binary format saves space this
      corpus does not need.
- [ ] `jolt` (Zephyr): port the same support.  Required before jolt can be considered a full
      replacement for the embassy firmware.

### Later: what should I be drilling?

This is the point of the log.  Not part of the first landing, and it constrains nothing about the
format above, because everything below is derived rather than recorded.

The question to answer is "what am I actually getting wrong, repeatedly, in real writing" --
which is a different and better question than "what does a drill file think I should practice".
Corrections are the signal, so the analysis is about separating the kinds of correction that mean
"drill this" from the kinds that mean nothing.

- [ ] A host tool that replays a log through `Lookup` + `Joiner` and reports the `Joined` stream
      alongside the strokes.  Any entry with `remove > 0` is a correction; the strokes and timings
      around it say what kind.
- [ ] Separate corrections that indicate a *problem* from corrections that are just how steno
      works:
      - orthographic corrections driven by a suffix stroke (the case the steno delay exists for)
        are the dictionary doing its job -- **not** drill material, and they will be the bulk of
        the `remove > 0` events, so they have to be filtered out first or they drown everything.
      - `*`-and-rewrite: the writer produced a stroke, saw it was wrong, undid it, and wrote
        something else.  This is the core signal.
      - hesitation without correction: an unusually long pause before a stroke that then comes out
        right.  Not an error, but it is a word that isn't yet automatic, and it is invisible to any
        approach that only looks at corrections.
- [ ] Rank by what would actually pay off: the same word or brief missed repeatedly, weighted by
      how often it comes up in the writing.  A word missed twice out of two attempts matters less
      than one missed ten times out of fifty.
- [ ] Distinguish "I don't know the brief" (writer spells it out or strokes something else
      entirely, then corrects) from "I know it but misfinger it" (the corrected stroke is one key
      off the intended one).  These want different drills -- the first is vocabulary, the second is
      technique -- and the stroke bit patterns make them mechanically distinguishable.
- [ ] Feed the ranked result to `steno-flow` (phase 6) as drill material, rather than building any
      scheduling here.
- [ ] Once phase 3 lands, tag each logged stroke with the active context slot.  Which app the
      writing happened in is cheap to record and hard to reconstruct later -- and errors in prose
      and errors in code are probably worth drilling separately.

### Privacy

This logs everything written in steno, verbatim.  The device holds it in RAM only and loses it on
power-off; the host file is local and is not synced anywhere by this project.  Worth stating
plainly in the host app's UI, and worth an off switch there before the corpus gets large.

---

## Phase 3: window tracking and per-window contexts

### Key insight: the device never sees a window identity

The host maintains the map from "whatever identity policy we're using today" to a small integer
slot, and the device only ever receives `SetContext { slot: u8 }`.  This keeps string storage and
LRU bookkeeping off the RP2040, and -- more importantly -- means the open question about window
identity granularity and garbage collection becomes a host-side policy that can be changed without
reflashing.

### Host side

- [ ] Start with **app bundle ID** granularity.  `NSWorkspace.shared.notificationCenter`
      `didActivateApplicationNotification` gives this with no permission prompt, and the set is
      bounded by the number of running apps -- no unbounded ID allocation, no GC problem.
- [ ] Maintain an LRU of bundle ID -> slot, sized to the N from phase 0.  Evicting a slot just means
      that window starts fresh next time it's focused, which is the same as the behaviour today.
- [ ] Leave a seam for finer granularity later.  Per-window focus needs `AXObserver` with
      `kAXFocusedWindowChangedNotification`, which requires the user to grant Accessibility
      permission, and window IDs *are* ephemeral -- which is exactly why the slot indirection is
      worth having.  Defer this until app-level granularity has been lived with; it may well be
      enough.

### Device side

- [ ] `bbq-keyboard/src/dict.rs`: `Dict` holds `contexts: Vec<Context>` where
      `Context { lookup: Lookup, joiner: Joiner }`, plus an `active: usize`.  Build the shared
      `Vec<bbq_steno::dict::Dict>` once in `Dict::new()` and clone the `Rc`s into each context.
- [ ] **Route context switches through the stroke channel, not a side channel.**  Change
      `STROKE_QUEUE` in `jolt-embassy-rp/src/main.rs` from `Channel<Stroke, 10>` to a
      `Channel<StenoCmd, 10>` with `Stroke(Stroke)` and `SetContext(u8)` variants.  A focus change
      delivered out of band could otherwise overtake strokes already queued and apply the wrong
      window's state to them.
- [ ] On switch, emit `Event::StenoState` for the newly active context so the LED indicator follows.
- [ ] Out-of-range slot: clamp to 0 (the default/unknown context) and log, rather than erroring.
- [ ] Apply the history-size reductions chosen in phase 0.

### Known limitation, worth writing down

The `Joiner.typed` buffer is a *model* of what is on screen.  Editing with the mouse, typing on the
laptop keyboard, or any output the device didn't produce, desynchronises it -- and per-window
contexts don't fix that, they just stop windows from corrupting *each other*.  No action, but it
sets expectations for what this feature does and doesn't buy.

### Testing

- [ ] Alternate between two apps mid-sentence; confirm cap-next and space-next follow the window.
- [ ] Confirm undo (`*`) after a window switch undoes within that window's history.
- [ ] Confirm rapid focus changes don't reorder against strokes.
- [ ] Watch `HeapStats` while cycling through more windows than there are slots.

---

## Phase 4: user dictionary

Host is master.  The device holds a **RAM overlay containing only the delta** -- entries added since
the last flash -- rather than the whole user dictionary.  The flashed copy at `0x10200000` is
XIP-mapped and free; copying it into RAM would spend heap the window contexts need.

Because `Lookup` iterates `self.dicts` in priority order and later dictionaries override earlier
ones of the same match length, appending the overlay last is sufficient for it to shadow both the
flashed user dictionary and the main one.

### Tasks

- [ ] Device: build the overlay with `MapDictBuilder` -> `into_ram_dict()` (`RamDict` already
      implements `DictImpl`), and append it to the dict list of every context.
- [ ] Protocol: `UserDictClear`, `UserDictAdd { strokes, text }` (batched -- several entries per
      request to stay near the 64-byte packet economics), `UserDictCommit` to rebuild the `RamDict`
      and re-seat it in the contexts.  Keep an eye on `SIZE_LIMIT = 4200` in
      `jolt-embassy-rp/src/minder.rs`.
- [ ] Device: enforce a hard cap on overlay size, and report usage via `HeapStats`.  This overlay
      and the phase 3 contexts share the heap, whatever it has been sized to by then.
- [ ] Host: own the user dictionary file.  An add from the modal writes the file, pushes the single
      entry to the device overlay, and is immediately live.
- [ ] Host: reset detection.  The long-poll `GetEvent` fails on USB re-enumeration, so the primary
      signal is reconnect; the `boot_id` from phase 1b is the confirmation.  On reconnect, replay
      the whole delta.
- [ ] Host: "flash it" action.  Regenerate `user-dict.bin` (bbq-tool, as a subprocess initially),
      run the existing hash-compare + program path in `Flasher`, then `UserDictClear` since those
      entries are now in flash.  Most of this already exists as `keyminder dict --dict user`.
- [ ] Consider un-hardcoding the dictionary base addresses in `bbq-keyboard/src/dict.rs:27-34`
      while in there.

---

## Phase 5: lookup modal

### Trigger

- [ ] A reserved stroke checked in `Dict::handle_stroke`, mirroring how `RA*U` toggles raw mode
      today (`bbq-keyboard/src/dict.rs:52`).  It enqueues a minder event rather than producing
      output.  A dictionary entry emitting a new `Replacement` variant is the alternative, but the
      reserved-stroke precedent is simpler and needs no encoder changes in bbq-tool.
- [ ] Also give the modal a plain macOS hotkey, so it works when the keyboard isn't in steno mode.

### Give the modal its own context slot

When the modal has focus, the keyboard is still in steno mode and its output lands in the modal's
text field.  That is exactly what makes "test in the dialog" work -- but it means the modal must
occupy its own context slot, or practicing will corrupt the state of the window underneath.  Reserve
a slot for it.

### Search

- [ ] Load `bbq-tool/full.bin` in the host core (`MemDict::from_raw_ptr` over the read file, as
      `dict-test` and `typey` already do).
- [ ] Forward lookup (strokes -> translation) uses the existing binary search directly.
- [ ] Reverse lookup (word -> strokes) needs a scan of `len()`/`key(i)`/`value(i)`.  full.bin is
      several MB, so build a reverse index once at app start; cache it to disk if startup latency is
      noticeable.
- [ ] Drift check: `Request::Hash` already exists and is implemented on the device.  Use it to
      confirm the flashed region matches what the host thinks is there, and warn in the UI if not.
      Note that full.bin is the combined *host* file and is not byte-identical to the flashed
      `dicts.bin`; work out which artifact the hash is actually compared against.

### UI

- [ ] Search field that accepts either a word or steno, showing matches from both directions.
- [ ] A practice mode within the dialog: show a target, capture strokes, compare, don't leak the
      output into the app underneath.
- [ ] An "add to user dictionary" action, wired to phase 4.
- [ ] A "make an exercise" action, wired to phase 6.

---

## Phase 6: exercises -> steno-flow

No new SRS store.  `steno-flow/` is a sibling checkout with its own CLAUDE.md and TASKS.md.

- [ ] Read `steno-flow/CLAUDE.md` and `steno-flow/TASKS.md` and find out what it wants as input.
- [ ] Define the handoff -- most likely a file or directory the modal appends to, so the two
      projects stay decoupled and neither has to import the other.
- [ ] The modal's "make an exercise" action writes there.  Scheduling, review history, and drilling
      all stay in steno-flow.

---

## Open questions

- **Window granularity.**  Deferred deliberately.  Start at bundle ID; the slot indirection means
  moving to per-window later is a host-side change only.  Revisit after living with app-level
  granularity.
- **N contexts and history sizes.**  Blocked on phase 0.  Record the measured numbers here.
- **Swift <-> Rust bridge.**  staticlib + C ABI recommended; decide for real when writing 1c.
- **Does the modal need to suppress device output entirely** in some sub-modes, rather than relying
  on its own context slot?  Probably answerable only by using it.
- **How much stroke log should the device hold?**  Falls out of how long the host is realistically
  detached.  If that turns out to be hours rather than minutes, flash persistence comes back on
  the table despite the reasons against it in phase 2.
- **What does a correction actually look like in the data?**  The classification in phase 2 is a
  guess at the categories; the first real corpus will say whether they are the right ones, and in
  particular whether suffix-driven corrections can be filtered cleanly enough to leave a usable
  signal underneath.

## Testing notes

Per the project convention, changes need manual testing on hardware before commit, and functional
changes should be separate commits from refactoring.  The refactors here that should land on their
own: splitting keyminder into a library (1c), the `StenoCmd` channel change (phase 3), and any
history-constant changes that come out of phase 0.

Hardware-only checks, collected:

- long-poll round trip and preemption (1a)
- stroke log fidelity: write a known passage, drain it, replay it, confirm it reproduces what was
  actually typed -- including a deliberate misstroke and `*` (phase 2)
- stroke log gap handling: write with the host detached long enough to overflow the buffer,
  confirm the gap marker appears and the keyboard itself never stutters (phase 2)
- per-window cap/space and undo behaviour (phase 3)
- heap headroom while cycling contexts and with the overlay loaded (phases 3, 4)
- reset detection and delta replay across an unplug/replug (phase 4)
- typing into the modal without disturbing the window underneath (phase 5)
