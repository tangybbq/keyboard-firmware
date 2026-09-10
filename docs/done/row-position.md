# Selectable row position for Taipo/steno on 3-row keyboards

This document is a work plan for adding a runtime toggle that selects which
two of the three rows the Taipo and steno layouts occupy on a 3-row keyboard.
It is written so that an agent can carry out the work with minimal additional
context.  Read this whole document before starting.

## Motivation

Taipo and steno are inherently 2-row layouts (two main rows plus thumbs).  On
the 3-row boards (jolt3/proto3 geometry) they currently sit on the **top two
rows**, with the bottom row carrying `#` in steno and nothing in Taipo.  After
using a 2-row board for a while, the developer suspects the **lower two rows**
may be more comfortable, and wants to switch between the two placements at
runtime — without reflashing — using the currently-dead key at the top-left
of the keyboard.  Qwerty mode is unaffected (it needs all three rows).

## Background

### Scancode geometry

Everything runs in the proto3 scancode space (48 codes).  Codes are
column-major: `code = column * 4 + row`.  Left half is 0–23 (columns 0–5,
outer to inner), right half is 24–47, mirrored.  Rows 0/1/2 are the
top/middle/bottom rows; row 3 exists only as the thumb keys (15, 19, 23 on
the left; 39, 43, 47 on the right).  Boards translate their native scan
order into this space in `jolt-embassy-rp/src/translate.rs` before events
reach the layout code, so all work here is board-independent.

### Current row usage (all under `feature = "proto3"`)

- **Steno** (`bbq-keyboard/src/layout/steno.rs`, `STENO_KEYS`): letters on
  rows 0 and 1 (e.g. `*`/`S` at 4/5, `T`/`K` at 8/9 … `-D`/`-Z` at 24/25),
  `#` across row 2, vowels on the thumbs.
- **Taipo** (`bbq-keyboard/src/layout/taipo.rs`, `SCAN_MAP`): chord bits on
  rows 0 and 1 of columns 1–4, thumbs at 19/23 and 43/47.  Everything else
  is `None`.
- **Special keys** (`bbq-keyboard/src/layout.rs`): `MODE_KEY = 2`
  (bottom-left, column 0 row 2) drives mode selection.  The Taipo escape
  keys are the inner-column top and bottom keys: `TAIPO_1/3 = 20/22` left,
  `TAIPO_2/4 = 44/46` right; the inner-column middle keys 21/45 are steno
  `^`/`+`.
- **The dead key**: physical code 0 (top-left, column 0 row 0) is dead in
  both Taipo (`None` in `SCAN_MAP`) and steno (`Some(Stroke::empty())`) —
  this is the key that becomes the row-position toggle.  (In qwerty it is
  Grave, and stays that way; the toggle only exists in the 2-row modes.)

### Event flow

`LayoutManager::handle_event` (`bbq-keyboard/src/layout.rs`) first gives the
event to `ModeSelector::event` (mode-select chords, and the tap-a-taipo-key
toggle between Taipo and steno), then dispatches it to the mode's handler.
In `Steno`/`StenoDirect` mode events go to **both** the steno handler and
the Taipo handler (the Taipo escape-key latch allows typing Taipo from
steno mode), so any remapping must be applied once, consistently, for both.

The `two_row` flag (from `BoardInfo`, threaded into `LayoutManager::new`)
marks boards that physically have only two rows; the toggle must be inert
there.

## Desired behavior

- A new piece of runtime state, "row position": **Upper** (today's layout,
  the default) or **Lower**.
- Tapping physical key 0 by itself, while in Taipo, Steno, or StenoDirect
  mode on a 3-row board, toggles the position.  The tap must be consumed —
  not passed to the mode handlers.  (Note: today an untouched tap of key 0
  in steno mode actually sends an empty stroke, since `STENO_KEYS[0]` is
  `Some(Stroke::empty())`; consuming the key fixes that quirk in passing.)
- In **Lower** position the whole 2-row layout moves down one row, and the
  old bottom-row content (steno `#`) moves to the now-free top row — per
  the developer: "map the old bottom row (except the very left) to the
  top".
- The setting is shared by Taipo and steno (they interoperate), survives
  mode switches, and lasts until power-off.  Flash persistence is out of
  scope.
- Qwerty/NKRO/Artsey modes and 2-row boards are completely unaffected.

## Suggested design

Do **not** duplicate `SCAN_MAP`/`STENO_KEYS`.  Instead remap incoming
scancodes when the Lower position is active, rotating each column's three
main-row keys so the existing tables apply unchanged:

- physical `4k+1` (middle) → `4k+0`  (middle row acts as the layout's top row)
- physical `4k+2` (bottom) → `4k+1`  (bottom row acts as the layout's second row)
- physical `4k+0` (top)    → `4k+2`  (top row picks up the old bottom-row codes, i.e. `#`)

Exceptions, both halves handled identically (right side is the same with
+24) **except** the left column 0 ("the very left"): codes 0/1/2 are never
remapped (0 is the toggle itself, 1 stays steno `#`, 2 stays `MODE_KEY`).
The right outer column (24/25/26) **does** rotate — it holds real steno
letters (`-D`/`-Z`/`#`).  Thumb codes (`4k+3`) are never remapped.  The
inner columns rotate too, which moves the Taipo escape keys and `^`/`+`
down a row along with everything else (physically: escapes land on the
top+middle inner keys, `^`/`+` on the bottom inner key).

Implementation points (the implementing agent may refine):

- Keep the state and remap in `LayoutManager` (or `ModeSelector`) in
  `layout.rs`, `#[cfg(feature = "proto3")]`; proto2 has no third row and
  compiles the whole feature out.
- Apply the remap at the **top of `LayoutManager::handle_event`, before
  `ModeSelector::event`**, gated on `!two_row` and the current mode being
  Taipo/Steno/StenoDirect.  Remapping before the mode selector keeps the
  Taipo-escape tap detection (which lives in `ModeSelector` *and* in both
  handlers via `taipo_map`) consistent — remapping only at dispatch would
  make the physical bottom inner key (22/46, which becomes `^`/`+` in Lower
  position) still toggle Taipo↔steno in `ModeSelector`, which is wrong.
- Toggle detection mirrors the existing taipo-tap detection in
  `ModeSelector::event`: arm on press of key 0 when nothing else is
  pressed, fire on release when everything is up, and consume the events
  (`ModeNext::Discard` or equivalent).  Requiring a solo tap also avoids
  ever flipping the mapping while keys are held mid-chord.
- Press/release pairing: because the toggle only fires with all keys up,
  a press can never be remapped differently from its matching release.
  Convince yourself of this and note it in a comment.

Decision points (make a reasonable choice, note it in the commit message,
flag it in the final summary):

- **Mode-select chords**: `ModeSelector::new_mode` matches physical codes
  17/41 ('f'/'j'), 13/37, 9/33 — middle-row home keys.  With the remap
  before the selector, in Lower position these chords shift down to the
  physical bottom row (physical 18 remaps to 17, etc.), which is arguably
  correct — the "home row" moved down.  Accepting that shift is the simple
  choice; just document it.
- **Indication**: there is no obvious LED for this.  `set_sub_mode` is a
  no-op in `jolt-embassy-rp/src/dispatch.rs`, so a `MinorMode`-based
  indicator would be dead code for now.  A brief `set_mode_select`-style
  flash on toggle is optional; skipping indication entirely is fine.

## Plan

Work in the order below.  Each step should end with all tests passing
(`cargo test -p bbq-keyboard`) and the firmware building
(`cd jolt-embassy-rp && just build`).

### Step 1: Implement the toggle and remap (functional commit, with tests)

Add the row-position state, the solo-tap detection on key 0, and the
column-rotation remap as described above, in `bbq-keyboard/src/layout.rs`.

### Step 2: Tests (may join Step 1's commit)

`bbq-keyboard/tests/taipo.rs` drives the full `LayoutManager` through the
`Script` builder; its `SCANS` table maps taipo chord bits to physical
scancodes.  Add a Lower-position variant of that table (each entry's main-row
scans shifted down one row; thumbs unchanged) and cover at least:

- Tap key 0 in Taipo mode, then type chords via the lower-row scancodes and
  see the normal letters; a chord on the *top*-row scancodes produces
  nothing (those are now `#`/dead codes for Taipo).
- Toggle back restores the original mapping.
- In Steno mode (`SendRawSteno` expectations): after toggling, pressing
  physical middle+bottom keys produces the letter stroke, and a physical
  top-row key produces `#`.
- The right outer column rotates: physical 25 in Lower position strokes
  `-D`.
- Key 0 tapped in Taipo/steno produces no key/stroke output (it is
  consumed) — including the empty-stroke quirk noted above.
- A press of key 0 while another key is held does *not* toggle.
- Taipo-from-steno still works in Lower position via the escape keys at
  their new physical positions.
- Behavior with `LayoutManager::new(true)` (two-row board): key 0 tap does
  not change anything.

### Step 3: Documentation (comment-only commit)

Update the module-level comments in `layout.rs` (and the `SCAN_MAP` /
`STENO_KEYS` comments if they claim fixed rows) to describe the two row
positions and the toggle key.

## Wrap-up

- Run `cargo test -p bbq-keyboard`; build with `cd jolt-embassy-rp && just
  build`.
- Update `TASKS.md` if it has (or should have) an entry for this work.
- Do **not** flash hardware or assume it works on-device.  Per project
  policy, changes require manual testing on the keyboard before they are
  considered done — finish by summarizing what the developer should verify
  by hand.  Suggested manual checks: toggle in Taipo and type on the lower
  rows; toggle in steno and stroke on the lower rows, including `#` on the
  top row and `-D`/`-Z` on the right outer column; Taipo↔steno tap-toggle
  via the inner keys in both positions; mode-select chords in Lower
  position; qwerty unaffected; toggle again to return to the upper rows.

## Process notes

- Follow `CLAUDE.md` conventions: incremental logical commits; don't mix
  refactoring/comment-only changes and functional changes in one commit;
  commit messages in simple present tense with a short summary line, a
  blank line, and a body wrapped at ~72 columns explaining what and why.
- Commits carry an `Assisted-by: Claude:<model>` trailer (per the user's
  global CLAUDE.md), with `<model>` derived from the model actually
  running.
