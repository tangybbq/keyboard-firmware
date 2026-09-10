# Improve the Taipo layout: sticky modifiers

This document is a work plan for enhancing the Taipo layout support in
`bbq-keyboard/src/layout/taipo.rs`.  It is written so that an agent can carry
out the work with minimal additional context.  Read this whole document before
starting.

An earlier version of this plan covered test coverage, same-side rollover, and
chord-window timing; all of that work is **done** (see commits `543c2c2`,
`378b1c4`, `8ce7b23`, `fa62604`, `dba38d1`).  The remaining feature is
**sticky modifiers**: the ability to keep a modifier held down while tapping
other keys — e.g. holding Cmd/Alt and tapping Tab repeatedly to cycle
windows.  TASKS.md tracks this as "Taipo: sticky modifiers"; check off that
item (and fold in anything noted there) when the work lands.

## Background

Taipo is a chorded layout where each half of the keyboard is complete.  The
implementation lives in `bbq-keyboard/src/layout/taipo.rs`:

- `SideManager` (one per half) accumulates keys pressed within a 50ms window
  into a chord and emits `TaipoEvent { is_press, code }` records into a
  queue.  Rollover and timing are already handled here — **this type should
  not need changes** for this task.
- `TaipoManager::tick()` drains the event queue, maps chord codes to actions
  via `TAIPO_ACTIONS`, and sends `KeyAction`s through `LayoutActions`.  All
  the modifier logic lives here.

### How modifiers work today

The module doc comment (lines ~12–26) *claims* double-pressing a modifier
makes all held modifiers sticky until the two thumbs are pressed together,
but **this is not implemented**.  The actual behavior, all in
`TaipoManager::tick()` and `release_nonmod()`:

- `oneshot: Mods` holds pending one-shot modifiers.  A modifier chord
  (`Action::OneShot(m)`: `0x088` shift, `0x011` gui, `0x044` ctrl, `0x022`
  alt — same codes on either side) sends `KeyAction::ModOnly(new_mods)`
  immediately and accumulates into `oneshot`.  A second press of a chord
  whose modifiers are already all in `oneshot` has `new_mods == oneshot`,
  so it sends nothing and changes nothing — **a silent no-op; this is the
  hook point for sticky detection**.
- `Action::Simple(k)` / `Action::Shifted(k)`: calls `release_nonmod()`,
  sends `KeyPress(k, oneshot)` (Shifted ORs in `Mods::SHIFT`), sets
  `down = true`, then clears `oneshot` — modifiers are one-shot because of
  this clear.
- The chord-*release* event (the `!tevent.is_press` branch at the top of the
  drain loop): if `down`, sends `KeyAction::KeyRelease` — which zeroes the
  **entire** HID report, modifiers included — and clears `down` and
  `taipo_latch`.
- `Action::Release` (`0x300`, both thumbs — the "null" chord): if `oneshot`
  is non-empty, sends `KeyRelease` and clears `oneshot` and `taipo_latch`.
- `release_nonmod()`: if a non-modifier key is down, sends `KeyRelease` when
  `oneshot` is empty, or `ModOnly(oneshot)` when it isn't — i.e. it already
  knows how to release a key while keeping modifiers held.  (Today `oneshot`
  is always empty by the time it runs after a keypress, so the `ModOnly` arm
  is only reached when a modifier chord follows a held key.)

The HID side needs no changes: `jolt-embassy-rp/src/usb.rs` (~line 174)
already handles `ModOnly` (report with only modifiers), and `KeyPress`
carries its modifiers.

### Existing test coverage

- `bbq-keyboard/tests/taipo.rs` drives the full `LayoutManager` via the
  `Script` builder (chained stimulus/expectation steps in terms of taipo
  chords — see the file's doc comment).  `test_oneshot_modifier`,
  `test_oneshot_accumulate`, and `test_modifier_release` pin the one-shot
  behavior.  **`test_sticky_modifier_unimplemented` is a characterization
  test documenting exactly the gap this task fills** — replace it with real
  sticky tests.
- Tests run on the host with `cargo test -p bbq-keyboard`.

## Desired behavior

Pressing a modifier chord whose modifiers are already pending (i.e. a double
press — the same chord twice, or its counterpart on the other hand) makes the
held modifiers *sticky*: they stay in the HID report across any number of
subsequent keypresses and releases, until the null chord (both thumbs,
`0x300`) is pressed, which releases everything.  Per the module doc comment,
stickiness is all-or-nothing: the double press makes **all** currently-held
modifiers sticky, and there is no support for releasing only some of them.

There is no timing window on the double press: any `OneShot` event that adds
no new modifiers promotes to sticky, whenever it happens.

## Plan

Work in the order below.  Each step should end with all tests passing
(`cargo test -p bbq-keyboard`) and the firmware building
(`cd jolt-embassy-rp && just build`).

### Step 1: Comment cleanup (separate, comment-only commit)

- Remove the stale `// TODO: Fn key support` comment near the top of
  `taipo.rs` — function keys are implemented (the `0x3xx` both-thumbs
  entries at the end of `TAIPO_ACTIONS`).

### Step 2: Implement sticky modifiers (functional commit, with tests)

Suggested design (the implementing agent may refine it; TASKS.md has a
matching sketch):

- Add a `sticky: Mods` field to `TaipoManager` (default empty), holding the
  subset of `oneshot` that survives keypresses.  Invariant:
  `sticky ⊆ oneshot` (`oneshot` remains "modifiers currently held in the
  HID report").
- **Detection** — in the `Action::OneShot(m)` branch: when
  `new_mods == self.oneshot` (the press adds nothing — today's no-op case),
  set `self.sticky = self.oneshot`, making everything held sticky.
- **Keypress** — in the `Simple`/`Shifted` branches: replace
  `self.oneshot = Mods::empty()` with `self.oneshot = self.sticky`.  Sticky
  mods keep riding along in every `KeyPress`; non-sticky one-shots still
  clear.
- **Key release** — in the `!tevent.is_press` branch: when `down`, send
  `ModOnly(self.oneshot)` instead of `KeyRelease` when `oneshot` is
  non-empty (mirror `release_nonmod`'s logic; consider factoring them into
  one helper).  With no sticky mods, `oneshot` is empty there today, so
  plain behavior is unchanged.
- **Null chord** — in the `Action::Release` branch: also clear
  `self.sticky`.  Because `sticky ⊆ oneshot`, the existing
  `!oneshot.is_empty()` condition already covers the sticky case.

Points to think through (make a reasonable choice, note it in the commit
message, flag it in the final summary):

- **`taipo_latch` interaction**: the key-release branch currently clears
  `taipo_latch` whenever it fires.  With sticky mods held (sending `ModOnly`
  rather than `KeyRelease`), decide whether the latch should still clear.
  Keeping the current clearing is the simpler, defensible choice.
- **One-shot on top of sticky**: with sticky shift held, a single press of
  a ctrl chord adds ctrl to `oneshot` but not `sticky` — the next key gets
  ctrl+shift, later keys just shift.  This falls naturally out of the design
  above; pressing ctrl *twice* promotes everything (including ctrl) to
  sticky.  Verify with a test rather than leaving it implicit.
- **`Mods` needs bit ops on borrowed/owned values** — it's a `bitflags`
  type (`bbq-keyboard/src/lib.rs` ~line 149), so `|`, `&`, and subset
  checks (`contains`) are available.

Tests (in `tests/taipo.rs`, using the `Script` builder; delete
`test_sticky_modifier_unimplemented` and cover at least):

- Double-press shift → next key carries SHIFT; its release sends
  `ModOnly(SHIFT)` (not `KeyRelease`); a *second* key still carries SHIFT.
- The motivating case: double-press alt, then tap Tab (`S | N | I`) several
  times — every tab carries ALT and alt never drops between taps.
- Null chord ends stickiness: sends `KeyRelease`, and the next key is
  unmodified.
- Double press across hands (left shift chord, then right shift chord)
  also promotes to sticky.
- One-shot on top of sticky (the ctrl-on-sticky-shift case above).
- Sticky with a `Shifted` action: key carries sticky-mods|SHIFT, release
  falls back to `ModOnly(sticky mods)`.
- Releasing the physical modifier-chord keys themselves still sends nothing
  extra.
- Cross-hand and same-side rollover while sticky (modifiers must persist
  through `release_nonmod`'s `ModOnly` path).

### Step 3: Update the module doc comment (can join Step 2's commit)

The doc comment's sticky paragraph (lines ~22–26) currently describes
behavior that doesn't exist; once Step 2 lands it becomes true.  Re-read it
against the implementation and adjust wording where the details differ
(e.g. exactly what a "double press" is, and that promotion covers all held
modifiers).

## Wrap-up

- Run `cargo test -p bbq-keyboard`.
- Build the firmware: `cd jolt-embassy-rp && just build`.
- Update TASKS.md: check off the sticky-modifiers item (and its comment-
  cleanup sub-item), noting any behavior decisions made.
- Do **not** flash hardware or assume it works on-device.  Per project
  policy, changes require manual testing on the keyboard before they are
  considered done — finish by summarizing what the developer should verify
  by hand.  Suggested manual checks: double-press Alt then Tab-Tab-Tab to
  cycle windows, ending with the null chord; one-shot modifiers still
  one-shot; the null chord with nothing held does nothing; sticky behavior
  from steno mode via the taipo latch.

## Process notes

- Follow `CLAUDE.md` conventions: incremental logical commits; don't mix
  refactoring/comment-only changes and functional changes in one commit;
  commit messages in simple present tense with a short summary line, a
  blank line, and a body wrapped at ~72 columns explaining what and why.
- Commits carry an `Assisted-by: Claude:<model>` trailer (per the user's
  global CLAUDE.md), with `<model>` derived from the model actually
  running.
- The commented-out `info!` logging lines in `taipo.rs` may be useful while
  debugging; leave them (or tidy them consistently) but don't turn them
  into permanent noise.
