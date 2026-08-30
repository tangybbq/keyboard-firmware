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
| Which table is live | Taipo or Posh; the device switches at runtime and the host is never told — and *when* you switch is itself a data point |

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
- **The single-MCU boards are the target.**  proto4 and mesa1 scan the whole keyboard from one
  MCU, so every key event is timestamped by the same 1 ms loop and the log is exact.  The split
  boards (jolt3, jolt2) work too, with a timing caveat on the remote half; fixing that is
  phase 5 and is deliberately off the critical path.
- **Two host implementations, on purpose.**  The Mac trainer is Swift all the way down,
  including its own minder client.  The collector and the statistics stay a Rust CLI over the
  `minder` crate.  Without steno there is no large Rust core worth bridging to — the protocol
  is a handful of CBOR messages over a bulk endpoint — and an FFI boundary would cost more than
  it saves.  What the two must agree on is pinned by tests rather than by shared code; see
  "Two implementations, kept honest".
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
| Boards | `jolt-embassy-rp/src/board.rs` | proto4 and mesa1 are `Inter::None` — one MCU, whole 30-key board, `two_row` |
| Split halves | `jolt-embassy-rp/src/inter.rs` | jolt3/jolt2 only: passive MCU debounces, sets a bitmap, asserts IRQ; active side reads over I2C |
| Scan → key code | `jolt-embassy-rp/src/translate.rs` | per-board table into the proto3 numbering, codes 0..47, 255 for unmapped |
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
- **On proto4 and mesa1, exact.**  One MCU scans all 30 keys in the same 1 ms loop and
  timestamps every event itself.  Both hands are measured on one clock, with no protocol in
  between.  These are the boards in daily use, so the interesting numbers are the good ones.
- **On the split boards, the remote half is quantized to the I2C read.**  The passive MCU
  debounces, sets a bit in a bitmap and asserts an IRQ; the active side then does a `ReadKeys`
  round trip.  A second key landing *during* that round trip folds into the same bitmap, so two
  keys get one timestamp and their order is lost — and that is exactly the case being measured,
  the fingers of a chord landing a millisecond or two apart.

The second point is the one real accuracy limit, it applies only to jolt3/jolt2, and phase 5
addresses it if and when a split board matters again.  Everything before phase 5 should be
developed and judged on a single-MCU board, where the question does not arise.

---

## Phase 0: chord table export and a host replay harness

No device change.  This exists so that phases 3 and 4 have something to develop against before
any firmware lands, using synthetic logs.

- [X] **Machine-readable chord table.**  A generator that emits `layouts.json` from
      `TAIPO_ACTIONS` and `POSH_ACTIONS`: per variant, per chord code, the finger keys involved
      (named as in `TAIPO.md`: `a o t e` bottom, `r s n i` top, plus `Sp`/`Bk`), the action
      (single key, shifted key, text, one-shot modifier, release), and for `Action::Text` the
      string.  Also the `SCAN_MAP` in both row positions and for each supported board, so a
      scan code can be resolved to (hand, finger, row).
      - Prefer a `bbq-keyboard` example or test binary that walks the real tables over another
        text parser; `words/ngrams.py` parses `taipo.rs` as text and can stay as it is, but the
        JSON the app depends on should come from the compiler.
      - Check the JSON in, with a test that fails when it is stale — the `bbq-consts` pattern.
- [X] **Replay harness.**  A host library that takes a sequence of `(scan code, press/release,
      time)` and drives a real `TaipoManager` through it, advancing the virtual clock with
      `tick()` exactly as `bbq-keyboard/tests/taipo.rs` already does, and emitting a stream of
      derived events: chord committed (code, hand, action, first-key time, last-key time,
      termination reason), key action sent, modifier change, dead chord, split chord.
      - `LayoutManager::row_event` and the variant toggle are part of the replay, so mode,
        variant and row position come out of it rather than being tracked separately.
- [X] **Synthetic log generator** for tests: turn a target string into the key events an ideal
      writer would produce, with configurable sloppiness (chord spread, same-hand runs,
      misfingerings, corrections).  This is what phase 4's analysis is unit-tested against, and
      it is the only way to have known ground truth.

### What landed

All of it, in `bbq-keyboard`, behind the crate's existing `std` feature so the
firmware is untouched.  Nine commits, and `cargo test` covers the lot.

- `bbq-keyboard/src/translate.rs` — the per-board scan tables **moved** out of
  `jolt-embassy-rp`, which is a `thumbv6m` binary crate a host tool cannot
  depend on.  Pure move, its own commit.  It also gains a `BOARDS` list, so
  nothing downstream has to hardcode the board names.  Note `jolt/src/mapping.rs`
  still carries its own divergent copy; that TASKS.md entry is now a
  deduplication rather than a fix.
- `bbq-keyboard/layouts.json` — generated by `cargo run --example gen-layouts`,
  checked in, guarded by `tests/layouts_json.rs`.  It has the whole scan code
  pipeline (per board, per row position, per variant), every chord with its keys
  named as `TAIPO.md` names them, and for the key actions the character they
  type, inverted out of the `usb_typer` table so it cannot disagree with what is
  actually sent.  Written by hand rather than through serde: no dependency, and
  the field order is what makes the checked-in file stable.
- `bbq_keyboard::replay` — the harness.  Ticks a real `LayoutManager` once per
  millisecond, as the dispatch loop does, and emits chords, key actions,
  modifier changes, mode changes, variant changes and row position changes.
  There is a plain text log format (`1234 + L.a`) with parse and print, which is
  what phase 2's on-disk format should be.
- `bbq_keyboard::synth` — the generator.  Target string in, key events plus the
  plan that produced them out.  Knobs for spread, hold, gap (negative rolls the
  next chord into this one), same-hand runs, spelled-out grams, split chords and
  three kinds of mistake with the corrections that follow.  Deterministic, not
  random: golden files have to be reproducible.
- `bbq-keyboard/tests/golden/` — a clean log and a sloppy one, with the derived
  events for each, in those text formats.  This is 4b's Swift contract, checked
  in now.  Regenerate with `UPDATE_GOLDEN=1 cargo test --test golden`.

Two small firmware-side additions were needed, both default-empty methods on
`LayoutActions`, so no behaviour changes and no implementation had to be
touched:

- `taipo_chord(side, code, end)`, reported for every chord the engine commits,
  before the table lookup, so a dead chord is reported like any other.  `ChordEnd`
  names the three terminations.  This is the one thing the replay could not
  derive without reimplementing `SideManager`, which is exactly what the "replay
  is the actual engine" decision exists to prevent.
- `set_row_position(lower)`, so the row position comes out of the layout rather
  than out of a second copy of the toggle key's solo-tap rule.  Phase 2's
  `RowShift` marker wants the same hook.

Notes for the later phases, from having written it:

- **A chord's first and last key times come from the log, not the engine.**  The
  replay knows when every key went down, so `SideManager` needs no timing state
  and the firmware gains no bytes.  Phase 2's record format needs nothing extra
  for spread.
- **Phase 2's key event tag needs to reach 47, not 63.**  Six bits is right, but
  the "reserve 63 for a physical key with no code" line should say that
  `translate.rs` returns 255 and the log has to substitute; `bbq_keyboard::replay`
  spells such a key `k63`.
- **The `Variant` marker is not enough on its own.**  A replay that starts
  mid-session also needs the *mode*, since taipo chords are assembled in steno
  mode too and are reported there; the plan already has a `Mode` marker, and it
  is not optional.
- **Phase 3's split-chord rule wants the `end` reason, which is now available.**
  `replay::split_chords` does the mechanical half; the plausibility judgement is
  still phase 3's.
- **The segmentation in `synth` is greedy, not the `ngrams.py` cost model.**  Good
  enough to exercise the multi-character entries; phase 3 should replace it with
  the real one, which is also the "precompute the segmentation for english_10k"
  open question.

## Phase 1: protocol groundwork

Lifted from the archived plan; nothing here is taipo-specific.

### 1a. Device push

The device needs to hand over log records without the host having to poll tightly.

- [X] `Request::GetEvent { timeout_ms }` → `Reply::Event { .. }` / `Reply::NoEvent`, with at
      most one outstanding at a time.  The firmware's minder loop becomes
      `select(reader.read(), event_channel.receive())`, and a request arriving while a
      `GetEvent` is pending is answered by replying `NoEvent` first and then handling the
      request normally.  That is what makes it work without request tags: ordering on the
      single bulk IN pipe stays unambiguous.
- [ ] The log drain rides this: the device raises an event when the buffer crosses a
      watermark, and the host answers with `GetEventLog`.  A trainer that wants low latency
      sets the watermark to one record and gets records within a USB transaction.
      *(Waits on phase 2; nothing raises a real event yet.)*
- [X] Flash program/erase on the RP2040 blocks with interrupts masked; the sequential loop
      already prevents a pending `GetEvent` from firing inside that window, but it stays true
      only while the loop is sequential.

#### What landed

Four commits.  The protocol, the device handler, and a `keyminder poll` to drive it.

- `minder/src/lib.rs` — `Request::GetEvent`/`TestEvent` and `Reply::Event`/`NoEvent`/`Ok`,
  all at unused `#[n(..)]` indices, plus an `Event` enum whose only variant so far is
  `Test { seq }`.  `Reply` gains `Eq, PartialEq` so replies round trip in tests like requests
  do, and a new `tests_bulk` module round trips over plain minicbor — which is what the vendor
  bulk endpoint carries, with neither the HID nor the serial framing involved.
  `test_hello_bytes_unchanged` pins the `Request::Hello` bytes recorded in the Swift spike's
  README, so adding variants cannot quietly break the spike or an old `keyminder`.
- `jolt-embassy-rp/src/minder.rs` — `bulk_read` split, in its own refactor commit, into
  `read_packet` (one packet) and `bulk_read_rest` (assemble, given a first packet).  **The
  select is over the first packet only**: winning the race midway through a multi-packet
  request would strand its remaining packets and misframe everything after it.  An
  interrupting packet is stashed and finished by the main loop *after* the `NoEvent` goes
  out.  `EventQueue` is a `heapless::Deque` behind a `CriticalSectionRawMutex` with a `Signal`
  to wake the waiter; `push_event` never blocks and drops the oldest at depth 8, which is what
  phase 2's key hook needs.  `EventQueue::wait` is cancel safe: an event leaves the queue only
  on the poll that returns it.
- `keyminder poll` — `--test N` raises test events after a delay that outlasts the first
  poll, so they land while it is pending; `--interrupt` sends a `GetEvent` and a `Hello`
  back to back and checks that `NoEvent` comes back first.  `VendorMinder::call` is split into
  `send`/`recv` for that, and its read timeout becomes a field, since a long poll outlasts the
  old hardcoded 15 seconds.

Notes from having written it:

- **`TestEvent` is a protocol variant, not a debug feature.**  Nothing raises a real event
  until phase 2, and an event path that cannot be exercised cannot be reviewed.  It costs one
  task and a `Signal`, and it stays useful afterwards as a way to check the transport when the
  log itself is the thing under suspicion.  Phase 1d's golden vectors should cover it like any
  other message.
- **The select's branch order is the priority order**, since `embassy_futures::select3` polls
  in order and returns the first ready: event, then new request, then timeout.
- **The sequential loop is now load bearing and says so in a comment.**  `get_event` only runs
  while a `GetEvent` is being handled, so it can never overlap `program`.
- **Tested on hardware.**  Long-poll timeout exact at 5.001 s; a test event raised while a poll
  was parked arrived at 0.500 s with the queued rest drained immediately; a request interrupting
  a parked poll answered `NoEvent` then its own reply in 0.4 ms.  Multi-packet requests were
  exercised from 1 to 65 packets using a `Program` at a non-erase-aligned offset, which the
  device rejects before it erases anything, and again as the *interrupting* request so the
  stashed first packet was followed by 64 more.
- **A host-side missing zero-length packet, found here and since fixed.**  `VendorMinder::send`
  used one `write_bulk` and nothing else, so a request whose encoding was an exact multiple of
  64 bytes never got the empty packet that ends a message.  Confirmed on hardware: the device
  returned no reply at all, and then swallowed the *next* request as a continuation of the
  stuck one — a silently lost request, not merely a stall.  64 of the 4096 possible `Program`
  payload sizes land there, and `Flasher::check` sends the final page at whatever size is left
  over, so `keyminder dict` had roughly a 1.6% chance of hitting it per flash.  Fixed by
  mirroring what `Minder::bulk_write` already does on the way back; all 64 sizes verified.

### 1b. Version and identity

Landed.  All three new `Reply::Hello` fields are `Option`, which turned out to be the
whole point rather than a detail: non-optional fields would have made a host update stop
being able to talk to an unflashed keyboard, which is exactly when talking to it matters.
Verified against a mesa1 still running the older firmware.  The layout fingerprint is
FNV-1a over the tables (`bbq-keyboard/src/layout/fingerprint.rs`), computed on the device
from the tables it actually has rather than reported from a constant, and mirrored into
`layouts.json` where the existing staleness test keeps it honest.

- [X] Bump `VERSION` in `minder/src/lib.rs`.
- [X] Add a capability list to `Reply::Hello`, so a new host against old firmware degrades
      rather than hanging on an unanswered request.
- [X] Add `boot_id` to `Reply::Hello` — changes every boot.  A log stream is only continuous
      within one `boot_id`.
- [X] Add a **layout table hash** to `Reply::Hello`: a hash over `TAIPO_ACTIONS` +
      `POSH_ACTIONS` + `SCAN_MAP`, computed at build time.  Replay against a table the firmware
      no longer has must be *detectable* rather than quietly wrong.  Same argument as the
      archived plan's dictionary hash.

### 1c. `keyminder` as a library

Still worth doing, but for the CLI's own sake now rather than for a Swift bridge: the collector
wants a long-running connection object, and `main.rs` is the wrong place for one.

- [X] Move `VendorMinder`, `Flasher`, `FlashImage` out of `keyminder/src/main.rs` into a
      library (`keyminder/src/lib.rs`, or a `minder-host` crate if the CLI should stay thin).
      Pure refactor, its own commit.
- [X] Give it a connection object owning the long-poll loop, handing events to a callback.

### 1d. Two implementations, kept honest

The Swift app speaks minder itself, so `minder/src/lib.rs` stops being the only definition of
the protocol and starts being the *reference* one.  That is a real cost and it needs a real
mechanism, not good intentions.

- [X] **The wire format is minicbor's, not "CBOR".**  `#[derive(Encode)]` with numbered fields
      produces a specific framing — variant index and field indices, arrays rather than maps
      unless asked — and a Swift CBOR library will happily encode something else that is still
      valid CBOR.  Write down the actual byte layout of each message in `minder/`, next to the
      enums.
- [X] **Golden byte vectors.**  A checked-in file of `(message, encoded bytes)` pairs, generated
      by a Rust test and consumed by a Swift test.  Every `Request` and `Reply` variant, with
      edge cases: an empty payload, a payload spanning several 64-byte packets, one that lands
      exactly on a packet boundary (the zero-length-packet case in `Minder::bulk_write`).  A
      protocol change that forgets Swift then fails a test rather than failing in the field.
- [X] **`Reply::Hello` is the guard rail.**  The version, capability list and table hash from 1b
      mean an app built against an older protocol refuses to derive rather than deriving
      wrongly.  Both clients must check it; neither may ignore it.
- [X] **USB access from Swift — spiked, and it works.**  `docs/spikes/swift-usb/` matches the
      vendor interface, claims it, and completes a `Hello` round trip, as an ordinary unsigned
      binary with no root and no entitlement.  Claiming the vendor interface does not disturb
      typing.  **The Rust `staticlib` fallback is not needed**; the Swift-native design stands.
      Three things it learned, all recorded in that directory's README:
      - `IOUSBHostInterface.createMatchingDictionary(...)` builds a dictionary that
        `IOServiceGetMatchingServices` matches *nothing* against, silently — the properties
        must be nested under `kIOPropertyMatchKey` instead.  This was most of the spike.
      - IOUSBHost has no Swift overlay, so the API is reached through its
        `NS_REFINED_FOR_SWIFT` `__` spellings, found by compiler error.
      - Round trip is **median 0.37 ms, p95 0.54 ms** to an idle device.
      Still untested: the App Sandbox path, which would need `com.apple.security.device.usb`.
      For a personal, locally built app, shipping unsandboxed avoids the question entirely.
- [ ] Pick the CBOR library.  SwiftCBOR and PotentCodables are the obvious candidates; what
      decides it is which one can be made to match minicbor's framing without a fight, which
      the golden vectors will answer in an afternoon.  The spike deliberately left this open by
      hard-coding the `Hello` bytes, so the USB answer does not depend on the CBOR answer.
      **The framing is now known concretely**, and it is worse than "numbered fields": minicbor
      encodes a variant as `[index, [fields...]]` where the inner array is *positional* and
      unused field numbers are filled with `null`.  `Request::Hello` is
      `82 01 82 f6 6b …` — the `f6` is position 0, unused because `version` is `#[n(1)]`.  A
      Swift encoder emitting the natural `[1, ["2024-11-01a"]]` is valid CBOR and will be
      rejected.

## Phase 2: the device-side event log

### Record format

Four bytes, fixed stride:

```
byte 0   tag
byte 1   dt, low byte
byte 2   dt, high byte
byte 3   aux
```

- `tag` bit 7 clear → **key event**.  Bit 6 is press (1) / release (0); bits 5..0 are the key
  code *after* `translate.rs` and *before* the row shift — the space `SCAN_MAP` is indexed in,
  so it is board-independent and directly replayable.  Codes run 0..47 across every board, so
  six bits fits with room to spare; reserve 63 for "some physical key with no code", which is
  what `translate.rs` returns 255 for, so a dead physical key still appears as an event.
- `tag` bit 7 set → **marker**, kind in bits 6..0, with the new value in `aux`:
  - `Variant` — **Taipo ⇄ Posh**.  Every derived event has to be attributed to the table that
    was live when it happened, or the two layouts' statistics silently pool.  It is also
    interesting on its own: when the switch happens, and how the numbers differ either side of
    it, is one of the things worth knowing.
  - `Mode` — taipo / qwerty / steno, so keystrokes that were never taipo are excluded.
  - `RowShift` — the 3-row row-position toggle.  Always 0 on proto4 and mesa1, which are
    `two_row`, but the log format should not care which board it came from.
  - `Resume` / `Pause` — logging turned on and off, so a gap is distinguishable from silence.

  Markers also make a *partial* drain self-describing: without them, a replay starting after
  the buffer dropped its oldest entries would not know which table to look chords up in.
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
- The markers are emitted from the places that already know: `LayoutActions::set_mode` for
  `Mode`, `TaipoManager::toggle_variant` (whose caller already reports `MinorMode::Posh` to the
  LED) for `Variant`, and `LayoutManager::row_event` for `RowShift`.  Emit a full set at the
  head of every drain as well, so a host that attaches mid-session is never guessing.

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

## Phase 3: analysis

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
- [ ] **Hand alternation, but only within a burst.**  Every letter is available on both hands,
      so consecutive chords on the same hand are always avoidable — *while you are mid-flow*.
      After a pause, which hand you restart on carries no information: the fingers are back at
      rest and either hand is equally correct.
      So the rule is: a same-hand pair counts only when the gap between the two chords is under
      `ALTERNATION_WINDOW`.  Start it at 2 s, make it a setting, and let the interval
      distribution set the real default — the gap histogram should be visibly bimodal (within a
      word versus between them), and the window belongs in the valley.
      The same rule governs what it is measured *against*: the denominator is eligible pairs,
      meaning consecutive chords inside the window, not every pair in the session.  Otherwise a
      session with a lot of thinking time scores better than one without.
      **Monitoring reports it, drilling scores it.**  In passive collection it is one metric
      among several; in a drill it is an error, counted like a wrong chord — see phase 4.
- [ ] **Spelled-out grams.**  A run of single-letter chords whose text has a chord in the
      table.  Directly measures whether the 18 `Action::Text` entries are being used at all,
      which the drills currently cannot check.
- [ ] **Hesitations.**  Inter-chord interval well above that *transition's* own baseline —
      the pause is the signal, and it needs the sequence context to be meaningful.  Not an
      error, and the more interesting category, since it finds what is not yet automatic
      without needing anything to go wrong.
- [ ] **Variant switching.**  Segment the log by the `Variant` marker and report each layout
      separately, never pooled.  Beyond that: how often the switch happens, what was being typed
      just before it, and whether the numbers immediately after a switch differ from the steady
      state — a cost to switching would be worth knowing about, and no other instrument can see
      it.  If one layout is being used far more than the other, say so plainly rather than
      presenting two equally thin sets of statistics.
- [ ] **Ranking.**  Weight by how often the item actually occurs in this writer's own text.
      Something missed twice out of two matters less than something missed ten times out of
      fifty.

### Model store

- [ ] Items are chords *and chord transitions* — the sequence case the whole thing is for.
      Trigrams only for transitions that already look bad, to keep the state space sparse.
- [ ] Per item: exponentially-weighted mean and variance of the interval, error count,
      exposure count, last seen.  Keyed by `(variant, item)`, since Taipo and Posh are
      different skills.
- [ ] SQLite in `~/Library/Application Support/`, written by the Rust CLI and read (and
      appended to, for drill results) by the app.  One store, two front ends, and SQLite is
      first class from both languages — no bridge needed for this either.  Schema versioned,
      with the app refusing a newer schema rather than corrupting it.
- [ ] Port the segmentation cost model from `words/ngrams.py` into this crate: the app needs
      "what is the cheapest chord sequence for this word" at runtime, to know what a word
      *should* have been typed as.  `words/ngrams.py` stays as the research tool; it does not
      need to change.

## Phase 4: the Mac app

SwiftUI, menu bar plus a main window, Swift all the way down — its own minder client, its own
chord assembly, sharing the model store and the generated `layouts.json` with the Rust CLI but
not linking against it.  See 1d for what keeps the two from drifting apart.

### 4a. Shell and passive collection — goal 1, standalone

The app is useful before any drill exists.

- [ ] Menu-bar item: connection state, recording on / off / paused, today's numbers.
- [ ] Background: connect, enable logging, drain continuously, append to the log file, update
      the model.
- [ ] Reset detection: the long-poll fails on USB re-enumeration; `boot_id` confirms it.  Mark
      a discontinuity in the log and re-enable logging.
- [ ] Table-hash mismatch: warn loudly and stop deriving, rather than deriving wrongly.
- [ ] Stats window: the phase 3 analysis, rendered.  Chord heatmap over both tables, per-finger
      breakdown, the transition matrix, alternation rate over time, corrections and hesitations
      ranked, "where the time actually goes".

### 4b. The trainer

- [ ] **Input.**  Render what is typed from the app's own key events, so there is zero added
      latency — a trainer with laggy echo is unusable.  *Possibly over-cautious:* the spike
      measured the transport at 0.37 ms median, and the log record and the HID report are
      produced at the same instant on the device, so rendering from the log may cost nothing
      measurable and would remove the two-stream join entirely.  Re-measure once the phase 1a
      push path exists, under real typing rather than against an idle device, and simplify if
      it holds.  Annotate from the log, which arrives
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
      number.  Accuracy split by the taxonomy in phase 3: wrong chord, dead chord, split chord,
      spelled-out gram, same-hand run, correction.  A correction counts as an error even when
      the final text is right, and alternation is judged **across** corrections rather than
      exempting them — hitting `Bk` on the same hand as the letter it deletes is the same
      technique fault as any other same-hand run.
      Alternation is an **error** here where it was only a metric in passive collection: a
      drill is where technique is deliberately being trained, and a soft number nobody reacts
      to trains nothing.  It keeps the `ALTERNATION_WINDOW` exemption from phase 3, so pausing
      to read the next word costs nothing — which is what stops the rule from punishing the one
      thing a trainer must never punish, stopping to think.
- [ ] **Chord hints.**  Two hands drawn, the next chord's keys lit, fading out as the item is
      learned — keybr's idea, and much more necessary here, since there is nothing on the keys.
      Live technique strip under the text: an L/R bead per chord, and a spread bar per chord.
- [ ] **Variant awareness.**  Read the active variant from the log's `Variant` marker and
      follow it, so a drill always matches what the keyboard is actually doing.  The model is
      per-variant, which incidentally makes Taipo-vs-Posh comparable.
- [ ] **Chord assembly in Swift, checked against Rust.**  The app needs derived chords live,
      so it assembles them itself rather than round-tripping through a Rust replay.  That is a
      second implementation of `SideManager` — the chord window, the cross-hand commit, the
      same-side rollover, `SCAN_MAP` and the row shift — and it is exactly the divergence the
      "replay is the actual engine" decision was meant to prevent.
      The answer is the same as for the protocol: **golden files.**  The phase 0 synthetic
      generator produces key-event logs, the Rust replay produces the derived events, both are
      checked in, and a Swift test must reproduce them exactly.  Divergence becomes a test
      failure instead of a slow drift in the statistics.
      The app is also allowed to be a little wrong in a way the CLI is not: it renders live
      feedback, while the Rust CLI is what the model and the reported numbers come from.  If
      the two ever disagree about a session, the CLI wins.

### 4c. Rough order

USB spike (1d) → shell + connection → live chord assembly against the golden files → test mode
→ stats window → adaptive practice → hints and technique strip → lessons.  Each is usable on
its own, and the spike comes first because it is the only step that can invalidate the design.

Note the app does *not* need the passive collector: that is the Rust CLI's job, running in the
background, and the app reads the model store it writes.  Splitting it that way means goal 1 is
delivered by phase 3 with no Swift at all, and the app is purely the trainer.

## Phase 5: split-board timing accuracy

**jolt3 and jolt2 only, and only if a split board becomes the daily driver again.**  proto4
and mesa1 have one MCU and one clock, so nothing here applies to them.  Kept in the plan
because the log format and the analysis are board-independent and should stay that way, and
because the measurement is cheap enough to be worth having before it is needed.

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
- [ ] Either way, record the residual skew in this file, and have the analysis refuse to report
      within-chord timing for a split-board log until it is known, rather than reporting a
      number whose error bar it cannot state.

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

- **Where does `ALTERNATION_WINDOW` actually belong?**  Settled in principle — a same-hand pair
  only counts inside a burst, a metric when monitoring and an error when drilling — but 2 s is
  a guess.  The inter-chord gap histogram from the first real corpus should show a valley
  between "within a word" and "between words"; put the window there.  Worth checking whether it
  wants to be a fixed number at all, rather than a multiple of the writer's own median gap,
  which would make it track improvement instead of needing to be retuned.
- **How much buffer?**  Falls out of how long the host is realistically detached.  16 bytes a
  character means a 64K ring is about 20 minutes of solid typing; if the app is running all
  the time, far less would do.
- **Is `CHORD_TIME = 100` right?**  It has been a "starting guess, not a measured value" since
  it was set.  The split-chord rate from phase 3 is the measurement that was missing.
- **Does the 20 ms debounce need to be 20 ms?**  Not this project's question, but this project
  is the instrument for asking it.
- **What does a correction actually look like?**  The taxonomy in phase 3 is a guess at the
  categories; the first real corpus will say whether they are the right ones.
- **Does the trainer need to drive the keyboard at all** — forcing a variant, or suppressing
  steno mode — or is reading enough?  Probably reading; revisit when the drill modes exist.
- **Where does the segmentation cost model live?**  The app needs "what is the cheapest chord
  sequence for this word" at runtime to generate material, and so does the CLI.  With the two
  no longer sharing code, that is a third implementation unless it is precomputed instead:
  generating the segmentation for every word in `english_10k` once, into a checked-in table
  beside `layouts.json`, would let both sides just look it up.  Probably the right answer;
  decide when phase 3's model store is written.  `words/ngrams.py` stays the research tool
  either way.
- **Does the Swift side need the whole engine, or only chord assembly?**  Actions, modifiers
  and the taipo latch may turn out not to matter for a trainer that already knows the target
  text.  The smaller the Swift reimplementation, the less there is to drift.

## Testing notes

Per the repo convention, changes are tested during review rather than before commit, and
refactors land separately from functional change.  The refactors here that should be their own
commits: splitting `keyminder` into a library (1c), and any `inter.rs` protocol change (5).

Hardware-only checks, collected:

- long-poll round trip and preemption: issue `GetEvent`, then immediately a `Hash`, confirm
  `NoEvent` then `Hash` arrive in order (1a)
- Swift `Hello` round trip over the vendor interface, sandboxed and not (1d)
- log fidelity: type a known passage, drain, replay, confirm it reproduces exactly what was
  typed — including a deliberate misfingering, a dead chord, and a `Bk` correction (2)
- gap handling: type with the host detached long enough to overflow the buffer, confirm the
  gap appears in the file and the keyboard itself never stutters (2)
- markers: switch Taipo ⇄ Posh, change mode, and toggle row position mid-session; confirm a
  drain that begins after a drop still replays correctly, and that a variant switch mid-word
  attributes the chords either side of it to the right table (2)
- hot path: confirm the 1 ms layout tick and the matrix scan are unaffected with logging on (2)
- split-board collapse rate, typing normally, on a jolt3 — the only check that needs a board
  other than proto4 or mesa1 (5)
- reset: unplug and replug mid-session, confirm the discontinuity is marked and logging
  resumes (4a)
- secure input: focus a password field, confirm recording pauses on the device (4a)
