# Posh layout support

This document is a work plan for adding the Posh chord layout
(<https://inkeys.wiki/en/keymaps/posh>) as a runtime-selectable variant of the
existing Taipo layout.  It is written so that an agent can carry out the work
with minimal additional context.  Read this whole document before starting.

The "Requirements" section is the developer's original spec, verbatim.  The
rest of the document resolves each of those bullets into a concrete decision,
with the reasoning, so the implementing agent should not need to re-derive
anything from the wiki or guess at intent.  Where a decision is a judgement
call it is flagged as such, so that the developer can correct it during review.

## Requirements (from the developer, verbatim)

- Mostly as described on this page
- Swap the two thumb keys, it should match Taipo
- Both thumbs keep "null key" behavior of Taipo
- Modifiers keep Taipo code behavior
- Right alt binging is shift
- Keys that don't have existing Taipo bindings can be implemented if key to
  send is simple, otherwise can be left as a future task
- Code is the existing taipo.rs code, extended with this mapping
- Upper-left key still switches between steno and taipo
- In taipo mode, lower left key toggles taipo/posh
- In taipo mode, third led (num 2) has colors to indicate taipo/posh

## What Posh is

Posh is a Taipo-style chording layout by the same community (inkeys.wiki),
described as "a taipo style layout that excludes the pinkies in order to make
combos more accurate and long periods of work more comfortable".  Like Taipo,
each hand is a complete mirrored keyboard and the user alternates hands
freely.  Unlike Taipo it uses only **6 finger keys per hand (3 columns × 2
rows) plus the 2 thumbs**, so every chord lives on the ring, middle and index
fingers.  The chord set was chosen by letter frequency with "a slight
preference to keep combos similar to taipo" — and the thumb layering (a
capitals layer and a number/symbol layer) is the same idea as Taipo's.

The wiki's base layout, for reference (left hand, then right):

```
A N I           I N A
O T E           E T O
    BS SP   SP BS
```

The wiki page is a JavaScript app; `curl` returns the full table embedded in
the HTML, but the complete, already-decoded chord table is reproduced in
"The Posh chord table" below, so there is no need to fetch it.

## Background: the existing Taipo implementation

All paths are relative to the repository root.  The firmware that runs today
is `jolt-embassy-rp`; the layout logic it uses is in the `bbq-keyboard` crate.

### Chord codes and the action table

`bbq-keyboard/src/layout/taipo.rs` implements Taipo.  Key events arrive as
proto3 scancodes; `SCAN_MAP` turns each into `(Side, bit)` where the bit is
one of 10 per hand.  `SideManager` accumulates bits pressed within the 50 ms
chord window (`CHORD_TIME`) into a `u16` chord code, and `TaipoManager::tick`
looks that code up in `TAIPO_ACTIONS: [Entry; 126]`, a flat table of
`Entry { code: u16, action: Action }` with

```rust
enum Action {
    Simple(Keyboard),   // type this HID key
    Shifted(Keyboard),  // type it with shift
    OneShot(Mods),      // press these modifiers, one-shot / sticky
    Release,            // the "null key": release all held modifiers
}
```

The chord-code bits, identical for both hands (the right hand is mirrored by
`SCAN_MAP`):

| bit     | key (Taipo letter typed alone) | physical column (left hand) |
|---------|--------------------------------|-----------------------------|
| `0x010` | r (top row)                    | pinky, proto3 col 1         |
| `0x020` | s (top row)                    | ring, col 2                 |
| `0x040` | n (top row)                    | middle, col 3               |
| `0x080` | i (top row)                    | index, col 4                |
| `0x001` | a (bottom row)                 | pinky, col 1                |
| `0x002` | o (bottom row)                 | ring, col 2                 |
| `0x004` | t (bottom row)                 | middle, col 3               |
| `0x008` | e (bottom row)                 | index, col 4                |
| `0x100` | **Sp** thumb (Space alone)     | scan 19 / 43                |
| `0x200` | **Bk** thumb (Backspace alone) | scan 23 / 47                |

Taipo's four variants of each chord are: alone, `+0x100` (with Sp: capitals /
shifted), `+0x200` (with Bk: digits and symbols), `+0x300` (both thumbs:
function keys).  Both thumbs *alone* (`0x300`) is `Action::Release`, the
"null key".

**Which column is the pinky.**  Scancodes are column-major, `code = column *
4 + row`, columns numbered outer to inner on each half (left 0–5, right 6–11
as 24–47).  Column 0 is the extra outer column (mode key etc.).  The steno
table (`bbq-keyboard/src/layout/steno.rs`, proto3 `STENO_KEYS`) settles the
finger assignment: column 1 (codes 4/5) is steno `*`/`S`, the pinky column,
and column 4 (codes 16/17) is `H`/`R`, the index column.  Taipo `r`/`a` sit
on codes 4/5, so **`r`/`a` (`0x010`/`0x001`) are the pinky keys**, and
`i`/`e` are the index keys.  Note that `TAIPO.md`'s finger header and the
comments in `bbq-keyboard/tests/taipo.rs` ("index to pinky") have this
reversed; the scancode tables are the truth.  (Posh's own design confirms it:
dropping the pinky from Taipo's `r s n i / a o t e` leaves `s n i / o t e`,
and Posh is `a n i / o t e` — `n i t e` stay where they were and `a` moves
into the vacated ring spot.)

### Modifiers

`OneShot` modifiers are sent to the host immediately and released with the
next non-modifier key; pressing a modifier chord whose modifiers are all
already held makes the whole held set sticky; `Release` (both thumbs) clears
everything.  See the module doc comment at the top of `taipo.rs` and the
`sticky`/`oneshot` fields.  `TaipoManager` reports modifier changes through
`LayoutActions::set_mod_state` for an LED.  None of this needs to change for
Posh; the `Action` enum already expresses everything Posh's modifier chords
need (including `OneShot(Mods::GUI | Mods::SHIFT)` for "gui+shift").

### Mode switching and the outer-column keys

`bbq-keyboard/src/layout.rs` owns `LayoutManager` and `ModeSelector`.
`LayoutManager::handle_event` runs, in order: `row_event` (proto3 only; the
row-position toggle on key 0, and the Lower-position scancode remap),
`ModeSelector::event` (tracks `pressed`, handles the mode key and the
tap-a-taipo-key Steno↔Taipo toggle), then dispatches to the mode's handler.
In `Steno`/`StenoDirect` the event goes to both the steno handler and the
Taipo handler (the "taipo latch": holding a taipo escape key in steno mode
types Taipo chords).

The left outer column (proto3 codes 0, 1, 2 — never remapped by the row
position logic) is:

| code | 3-row boards (jolt3)                    | 2-row boards (proto4, mesa1)            |
|------|-----------------------------------------|-----------------------------------------|
| 0    | top-left: row-position toggle           | does not exist (`translate.rs` never produces it) |
| 1    | middle-left: steno `#`; **dead in Taipo** | **lower-left**: steno `#`; **dead in Taipo** |
| 2    | bottom-left: `MODE_KEY`                 | **upper-left**: `MODE_KEY`              |

On 2-row boards (`two_row == true`) `MODE_KEY` cycles only Steno↔Taipo
(`LayoutMode::next`).  On 3-row boards it cycles Taipo→Qwerty→Steno.  The
Steno↔Taipo *tap* toggle on the inner-column escape keys (20/44, 22/46) works
on every board.

Key 1 is `Some(stroke!("#"))` in steno and `None` in Taipo's `SCAN_MAP` on
every board, so in Taipo mode it currently does nothing at all.

### Indication

`jolt-embassy-rp/src/dispatch.rs` implements `LayoutActions`.  LED 0 shows
the mode, LED 1 the steno state, LED 3 the Taipo modifiers (`MODS_LED`, via
`LedManager::set_solid`).  **LED 2 is unused** — it shows `UNDEF_INDICATOR`
(a faint blink) on boards that have it.  `LedManager::set_base(index, ..)`
ignores indices the board doesn't have, so code can address LED 2
unconditionally.  LED counts: jolt3 has 2 (`LedStripGroup<_, _, 2>` in
`board.rs`), proto4 and mesa1 have 4.  Indicator colors are `static
Indication`s in `jolt-embassy-rp/src/leds/manager.rs`, written at 4× the
brightness the LED gets (`WS2812_DIM`), with peaks around 16–24.

`LayoutActions::set_sub_mode(MinorMode)` / `clear_sub_mode` exist for exactly
this kind of sub-state (Artsey uses them for `MinorMode::ArtseyNav`), and are
currently no-ops in `dispatch.rs`.  `MinorMode` is in `bbq-keyboard/src/lib.rs`
(`#[derive(EnumSetType, Debug)]`).  The Zephyr port `jolt/src/lib.rs` has an
exhaustive `match` on `MinorMode` in its `set_sub_mode`, which must get a new
arm.  (`proto/` and `archive/` are dead code and don't build; ignore them.)

### Tests

`bbq-keyboard/tests/taipo.rs` drives the full `LayoutManager` with a `Script`
builder (press/release/chord/tap in terms of chord bits, `tick`, positional
expectations like `.types(Keyboard::A)`, `.mod_only(..)`, `.idle()`,
`.expect(Actions::SetSubMode(..))`).  `Script::taipo()` starts in Taipo,
`Script::steno()` in steno, `Script::two_row()` builds a 2-row board.  Chord
bit constants `A O T E R S N I SP BK` are defined at the top; `SCANS` maps
them to scancodes.  Run with `cargo test -p bbq-keyboard`.  `SideManager`
unit tests live inside `taipo.rs`.

## Resolving the requirements

### Which keys Posh uses

Posh drops the pinky: the `r`/`a` column (`0x010`/`0x001`) is **dead** in
Posh (a chord that includes either bit matches nothing and types nothing).
The three Posh columns, in the wiki's `#--` notation "as viewed from the left
hand" (leftmost = ring):

| wiki column | finger | top-row bit | bottom-row bit |
|-------------|--------|-------------|----------------|
| `#--`       | ring   | `0x020` (Taipo s) | `0x002` (Taipo o) |
| `-#-`       | middle | `0x040` (Taipo n) | `0x004` (Taipo t) |
| `--#`       | index  | `0x080` (Taipo i) | `0x008` (Taipo e) |

So Posh reuses Taipo's bit assignments and `SCAN_MAP` **unchanged**; the only
new thing is a second action table.  The first line of each wiki `input` cell
is the top row, the second the bottom row.

### Thumbs ("Swap the two thumb keys, it should match Taipo")

The wiki says "Space and backspace are swapped" relative to Taipo.  The
developer wants them back where Taipo has them, i.e. **the thumbs behave
exactly as in the existing implementation**: `0x100` = Space, `0x200` =
Backspace, and `0x300` (both thumbs alone) = `Action::Release`, the null key
(the wiki's "space and backspace together is sticky shift" is *not*
implemented; Taipo's sticky-by-double-press already covers that need).

The wiki's chord table has columns "alone / outer / inner / both".  In
Posh-as-described, "outer" is the space thumb and gives capitals, "inner" is
the backspace thumb and gives digits/symbols — the same pairing as Taipo,
where the Sp thumb gives capitals and the Bk thumb gives symbols.  Therefore:

- wiki **"outer"** column → chord `+ 0x100` (with **Sp**), like Taipo's
  capitals layer,
- wiki **"inner"** column → chord `+ 0x200` (with **Bk**), like Taipo's
  symbol layer,
- wiki **"both"** column → chord `+ 0x300`.

This follows the thumb *function* (capitals ride on the space thumb in both
layouts) rather than the physical position, which is what "match Taipo"
means.  The alternative — capitals on Bk — would invert the existing Taipo
muscle memory and is rejected.  *(Judgement call; flag in the summary.)*

### Modifiers ("keep Taipo code behavior", "Right alt binding is shift")

The four modifier chords are `OneShot` actions with Taipo's one-shot /
double-press-sticky semantics, unchanged.  Posh has gui, ctrl, alt and ralt;
`Mods` has no right-alt, and the developer wants that slot to be **Shift**:
the `ralt` chord is `OneShot(Mods::SHIFT)`.  The "both" variants of the
modifier chords (`gui+shift` etc.) are `OneShot(GUI | SHIFT)` and so on; the
`ralt+shift` one would be shift+shift, so it is left unmapped.  The "outer"
and "inner" variants of the modifier chords are plain brackets and are
mapped as such.

### What is left out (future work)

The `Keyboard` page used by `KeyAction` has no media/consumer keys, so these
stay unmapped: play/pause, next/prev track, stop, volume up/down, mute,
brightness up/down.  Doing them needs a consumer-control HID report in
`jolt-embassy-rp/src/usb.rs` and a new `KeyAction` — a separate task; note it
in `TASKS.md`.  The wiki's "layer 0–3" chord is a QMK concept and does not
apply.  `print` (`Keyboard::PrintScreen`) and `insert` (`Keyboard::Insert`)
are simple keys and **are** mapped.  The five empty rows at the bottom of the
wiki table stay unmapped.

### Toggle key ("In taipo mode, lower left key toggles taipo/posh")

The developer is describing a 2-row board (proto4/mesa1): there the upper-left
key is `MODE_KEY` (which on those boards switches only between steno and
taipo — "still switches between steno and taipo" is asking that this be left
alone) and the lower-left key is proto3 **code 1**, steno `#`, which is dead
in Taipo mode on every board.  So:

- `POSH_TOGGLE_KEY: u8 = 1`.  A **solo tap** (press with nothing else down,
  release with nothing else down) of key 1 **while in `LayoutMode::Taipo`**
  toggles the variant.  In every other mode the key keeps its meaning (steno
  `#`, qwerty).  On the jolt3 this is the middle key of the outer left
  column, which is equally dead in Taipo, so the feature is identical on all
  boards.
- The variant is a property of the Taipo engine, not a new `LayoutMode`: it
  persists across mode changes and is used by the steno-mode taipo latch too.
  It is lost at power-off (no flash persistence; same as the row position).
- Nothing about the Steno↔Taipo switching (mode key, escape-key tap) changes.

### Indication ("third led (num 2) has colors to indicate taipo/posh")

While in `LayoutMode::Taipo`, LED 2 shows one color for the Taipo variant and
another for Posh; in every other mode LED 2 is off.  Only 4-LED boards have
LED 2; on the jolt3 the `set_base` is silently ignored, which is fine.

## The Posh chord table

Every code below was computed from the wiki table with the bit assignment
above and checked for duplicates (there are none; 41 wiki rows).  Codes are
written as Taipo-style `u16`s; the three thumb entries are copied from Taipo.
Each line is `code → Action`, US layout.  Use exactly these.

```
// Thumbs, as in Taipo.
0x100 Simple(Space)           0x200 Simple(DeleteBackspace)   0x300 Release

// Single keys: alone, +Sp, +Bk, +both.
0x008 Simple(E)   0x108 Shifted(E)   0x208 Simple(RightArrow)  0x308 Simple(End)
0x004 Simple(T)   0x104 Shifted(T)   0x204 Simple(DownArrow)   0x304 Simple(PageDown)
0x020 Simple(A)   0x120 Shifted(A)   0x220 Simple(Escape)      0x320 Simple(DeleteForward)
0x002 Simple(O)   0x102 Shifted(O)   0x202 Simple(LeftArrow)   0x302 Simple(Home)
0x080 Simple(I)   0x180 Shifted(I)   0x280 Simple(ReturnEnter) 0x380 Simple(Tab)
0x040 Simple(N)   0x140 Shifted(N)   0x240 Simple(UpArrow)     0x340 Simple(PageUp)

// Same-row pairs.
0x0c0 Simple(S)   0x1c0 Shifted(S)   0x2c0 Simple(Comma)       0x3c0 Simple(Apostrophe)   // , '
0x00c Simple(H)   0x10c Shifted(H)   0x20c Simple(Dot)         0x30c Shifted(Apostrophe)  // . "

// Letters whose +Bk is a digit and +both the matching function key.
0x048 Simple(R)   0x148 Shifted(R)   0x248 Simple(Keyboard0)   0x348 Simple(F10)
0x060 Simple(D)   0x160 Shifted(D)   0x260 Simple(Keyboard1)   0x360 Simple(F1)
0x084 Simple(L)   0x184 Shifted(L)   0x284 Simple(Keyboard2)   0x384 Simple(F2)
0x00a Simple(C)   0x10a Shifted(C)   0x20a Simple(Keyboard3)   0x30a Simple(F3)
0x006 Simple(U)   0x106 Shifted(U)   0x206 Simple(Keyboard4)   0x306 Simple(F4)
0x028 Simple(M)   0x128 Shifted(M)   0x228 Simple(Keyboard5)   0x328 Simple(F5)
0x082 Simple(W)   0x182 Shifted(W)   0x282 Simple(Keyboard6)   0x382 Simple(F6)
0x0a0 Simple(F)   0x1a0 Shifted(F)   0x2a0 Simple(Keyboard7)   0x3a0 Simple(F7)
0x042 Simple(G)   0x142 Shifted(G)   0x242 Simple(Keyboard8)   0x342 Simple(F8)
0x0c2 Simple(Y)   0x1c2 Shifted(Y)   0x2c2 Simple(Keyboard9)   0x3c2 Simple(F9)

// Letters whose +Bk / +both are symbols.
0x0e0 Simple(P)   0x1e0 Shifted(P)   0x2e0 Shifted(Equal)      0x3e0 Simple(Equal)        // + =
0x00e Simple(B)   0x10e Shifted(B)   0x20e Simple(Minus)       0x30e Shifted(Minus)       // - _
0x04a Simple(V)   0x14a Shifted(V)   0x24a Simple(ForwardSlash) 0x34a Simple(Backslash)   // / \
0x068 Simple(K)   0x168 Shifted(K)   0x268 Simple(Semicolon)   0x368 Shifted(Backslash)   // ; |
0x086 Simple(J)   0x186 Shifted(J)   0x286 Shifted(Semicolon)  0x386 Shifted(Keyboard8)   // : *
0x02c Simple(X)   0x12c Shifted(X)   0x22c Shifted(Keyboard4)  0x32c Shifted(Keyboard3)   // $ #
0x02a Simple(Q)   0x12a Shifted(Q)   0x22a Shifted(Keyboard2)  0x32a Simple(F11)          // @
0x04c Simple(Z)   0x14c Shifted(Z)   0x24c Shifted(Keyboard7)  0x34c Simple(F12)          // &

// Punctuation-only chords (no +both).
0x024 Shifted(ForwardSlash) 0x124 Shifted(Keyboard1) 0x224 Shifted(Keyboard6)             // ? ! ^
0x08a Simple(Grave)         0x18a Shifted(Grave)     0x28a Shifted(Keyboard5)             // ` ~ %

// Modifiers (same-finger vertical pairs, plus the ralt chord).
0x044 OneShot(GUI)     0x144 Shifted(Keyboard0) 0x244 Shifted(Keyboard9) 0x344 OneShot(GUI | SHIFT)      // ) (
0x088 OneShot(CONTROL) 0x188 Simple(RightBrace) 0x288 Simple(LeftBrace)  0x388 OneShot(CONTROL | SHIFT)  // ] [
0x022 OneShot(ALT)     0x122 Shifted(RightBrace) 0x222 Shifted(LeftBrace) 0x322 OneShot(ALT | SHIFT)     // } {
0x026 OneShot(SHIFT)   0x126 Shifted(Dot)       0x226 Shifted(Comma)     (0x326 unmapped)               // > <  (wiki: ralt)

// Simple extras.
0x046 Simple(PrintScreen)     // wiki "print"; 0x146/0x246/0x346 (volume, mute) unmapped
0x0a8 Simple(Insert)          // wiki "insert"; 0x1a8/0x2a8 (brightness) unmapped

// Unmapped: 0x08c (media keys), 0x0a4 (QMK layers), 0x064 0x062 0x0a2 0x0c4 0x0c8 (empty in wiki).
```

That is 130 entries.  All of the `Keyboard` variant names above exist in
`usbd_human_interface_device::page::Keyboard` (verified against the 0.4.5
source).

## Design

Keep it a small extension of `taipo.rs`, as the developer asked.

1. **Variant state in `TaipoManager`.**  Add

   ```rust
   #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
   pub enum TaipoVariant { #[default] Taipo, Posh }
   ```

   a `variant: TaipoVariant` field, `pub fn variant(&self)`, and `pub fn
   toggle_variant(&mut self) -> TaipoVariant`.  Replace the direct use of
   `TAIPO_ACTIONS` in `tick` with a `fn actions(&self) -> &'static [Entry]`
   that returns `&TAIPO_ACTIONS` or `POSH_ACTIONS`.  Because the tables are
   `'static`, binding the lookup result first (`let entry =
   self.actions().iter().find(..)`) keeps the borrow checker happy around the
   `&mut self` calls that follow.  Nothing else in the manager (side
   accumulators, modifiers, latch) changes.  Switching variants does not
   touch held modifiers; the toggle only fires with every key up anyway.

2. **The table in its own file.**  Put `POSH_ACTIONS: &[Entry]` in
   `bbq-keyboard/src/layout/posh.rs` (declare `mod posh;` in `layout.rs`),
   with `Entry` and `Action` made `pub(super)` in `taipo.rs`.  Use a slice,
   not a fixed-size array, so nothing has to count entries.  Give the file a
   doc comment like Taipo's: what the bits are, the thumb decision, what was
   left out and why.  Group and order entries as in the table above.

3. **Toggle detection in `LayoutManager`** (`layout.rs`), gated on
   `#[cfg(feature = "proto3")]` like the row toggle, since code 1 is a thumb
   in the proto2 scan space:

   - `const POSH_TOGGLE_KEY: u8 = 1;` and a `posh_arm: bool` field, mirroring
     `ROW_TOGGLE_KEY` / `row_arm`.
   - In `handle_event`, **after** `self.mode.event(..)` (so
     `self.mode.pressed` is current) and only when the result is
     `ModeNext::Normal` and `self.mode.get() == LayoutMode::Taipo`: on
     `Press(POSH_TOGGLE_KEY)` set `posh_arm = self.mode.pressed == 1 <<
     POSH_TOGGLE_KEY`; on `Release(POSH_TOGGLE_KEY)`, if `posh_arm &&
     self.mode.pressed == 0` call `self.taipo.toggle_variant()` and report it
     (next item); clear `posh_arm`; in both cases `return` — the key is dead
     in Taipo so consuming it changes nothing.  Any other key event clears
     `posh_arm` (also clear it in `row_event` when key 0 is consumed, so a
     0+1 chord can't fire either toggle).  Requiring a solo tap means the
     table can never change in the middle of a chord.
   - Do not put this in `ModeSelector`; it is not a mode change.

4. **Reporting.**  Add `MinorMode::Posh` to `bbq-keyboard/src/lib.rs`.  When
   the variant changes, `LayoutManager` calls
   `actions.set_sub_mode(MinorMode::Posh)` on entering Posh and
   `actions.clear_sub_mode(MinorMode::Posh)` on returning to Taipo.  Nothing
   is reported at startup (Taipo is the default and the firmware assumes it).
   Add the new arm to the exhaustive match in `jolt/src/lib.rs`
   (`set_sub_mode`) so the Zephyr port keeps compiling; pointing it at
   `OFF_INDICATOR` or an existing indicator is enough — that port has no LED
   plan for this yet.

5. **Firmware indication** (`jolt-embassy-rp`):

   - `leds/manager.rs`: two new solid `Indication`s, e.g.
     `VARIANT_TAIPO_INDICATOR` = `RGB8::new(16, 8, 24)` (the Taipo mode hue,
     so the LED reads as "taipo/taipo") and `VARIANT_POSH_INDICATOR` =
     `RGB8::new(0, 16, 16)` (cyan; nothing else uses it).  Colors are a
     guess to be judged on hardware — say so in the commit message.
   - `dispatch.rs`: `const VARIANT_LED: usize = 2;` a `posh:
     Mutex<CriticalSectionRawMutex, bool>` (or `AtomicBool`) on `Dispatch`;
     set LED 2's base to `OFF_INDICATOR` in `Dispatch::new` like `MODS_LED`
     so it stops blinking `UNDEF`; a helper `update_variant_led` that reads
     `current_mode` and `posh` and sets LED 2 to off / taipo / posh.  Call it
     at the end of `set_mode` (after `current_mode` is updated) and from
     `set_sub_mode` / `clear_sub_mode`, which now `match` on the `MinorMode`
     (`Posh` updates the flag; `ArtseyNav` stays a no-op).  Take the mutexes
     one after another, never nested, as `set_mode` already does.

## Plan

Work in the order below.  Each step should end with all tests passing
(`cargo test -p bbq-keyboard`) and the firmware building (`cd jolt-embassy-rp
&& just build`).  Separate refactoring from functional changes, per
`CLAUDE.md`.

### Step 1: Select the action table through a variant (refactor commit)

Design item 1 without Posh: `TaipoVariant`, the field, `actions()`, and the
`tick` change, with only the `Taipo` arm populated (the `Posh` arm can map to
the same table for now, or the enum can have a single variant — whichever
avoids dead-code warnings).  No behavior change; the existing tests prove it.

### Step 2: Add the Posh table and the toggle (functional commit, with tests)

Design items 2–4 together: `posh.rs`, `MinorMode::Posh`, the solo-tap toggle
in `LayoutManager`, the `jolt` match arm.  (Adding the table without the
toggle would leave it as warned-about dead code, which is why these land
together.)

Tests to add:

- In `posh.rs`, a unit test over `POSH_ACTIONS`: every code is unique; no
  code uses the pinky bits (`code & 0x011 == 0`); every code has at least one
  thumb or finger bit.
- In `bbq-keyboard/tests/taipo.rs`, a `mod posh` of chord constants named by
  the Posh letter (`A = 0x020, N = 0x040, I = 0x080, O = 0x002, T = 0x004, E
  = 0x008`; reuse `SP`/`BK`), a `POSH_TOGGLE_KEY` constant, a
  `Script::toggle_posh()` step (press/release scan 1, `tick(1)`, expect the
  `SetSubMode`/`ClearSubMode` action — give it a parameter or two helpers),
  and `Script::posh()` (= `taipo()` then `toggle_posh()`).  Cover at least:
  - every single key and both thumbs on both hands, alone and `+Sp`;
  - a sample of multi-key chords, including same-row (`s`, `h`, `p`, `b`),
    cross-row (`r`, `k`, `x`), `+Bk` digits and symbols, and `+both` F-keys
    and symbols (`F1`, `F12`, `=`, `|`, `"`);
  - the modifier chords: one-shot GUI/Ctrl/Alt, the `ralt` chord producing
    `Mods::SHIFT`, a `+both` variant producing `GUI | SHIFT`, the bracket
    variants, and both thumbs alone still releasing modifiers;
  - `PrintScreen` and `Insert`;
  - pinky keys are dead in Posh (Taipo `R`, `A`, and a chord including them,
    produce nothing); an unmapped Posh code such as `0x064` (posh `A | N |
    T`, one of the wiki's empty rows) produces nothing;
  - toggling back restores Taipo (and reports `ClearSubMode`), and the
    variant survives Taipo→Steno→Taipo;
  - the steno-mode taipo latch uses the Posh table once Posh is selected
    (mirror `test_steno_taipo_latch`);
  - the toggle needs a solo tap: key 1 pressed while a chord key is held, or
    a chord key pressed while key 1 is held, does not toggle;
  - key 1 in Steno mode still strokes `#` and in Qwerty still types its key
    (no toggle, no `SetSubMode`);
  - `Script::two_row()`: the toggle works on a 2-row board;
  - with the row position toggled to Lower (`toggle_rows()`), key 1 still
    toggles Posh and the Posh chords work on the lower-row scancodes.

### Step 3: Show the variant on LED 2 (functional commit, `jolt-embassy-rp`)

Design item 5.  Build with `just build`; there are no host tests for this
crate.

### Step 4: Add `POSH.md` (documentation commit)

Add `POSH.md` at the repository root, next to `TAIPO.md`, and commit it on
its own.  It is the user-facing cheat sheet for the layout as actually
implemented, in the same style and structure as `TAIPO.md` (read that file
first and match its voice, headings, and table format):

- an intro naming the source (`https://inkeys.wiki/en/keymaps/posh`) and the
  local differences: thumbs as in Taipo, no sticky-shift thumb chord, `ralt`
  is Shift, media keys unmapped;
- the physical layout as a finger table (ring / middle / index, top and
  bottom rows), noting the pinky keys are dead;
- the thumb rule: `+Sp` capitals, `+Bk` digits/symbols/navigation, `+both`
  function keys and the remaining symbols; both thumbs alone releases held
  modifiers;
- the chord tables by group, written in terms of the Posh letters (e.g.
  `a+n`, `n(top)+e(bot)`), with all four variants where they exist: single
  keys and navigation, same-row pairs, the digit/F-key letters, the symbol
  letters, the punctuation-only chords, the modifiers and brackets, and the
  extras (`print`, `insert`);
- modifier behavior: a short pointer to the "Modifiers: how they behave"
  section of `TAIPO.md`, since it is identical;
- switching: the solo tap of the lower-left key (steno `#`; the middle
  outer-left key on the jolt3) in Taipo mode, that the choice persists across
  mode switches and the steno-mode taipo latch until power-off, and LED 2's
  two colors on the 4-LED boards;
- what is unmapped and why (media, volume, brightness, layers, `ralt+shift`,
  the wiki's empty rows).

Generate the tables from the Rust `POSH_ACTIONS` table so they can't drift
from it; check a few entries against "The Posh chord table" above.

### Step 5: Other documentation (comment-only commit)

- Update the module comments in `layout.rs` (the "Row position" discussion is
  the model) and the `taipo.rs` header to mention the variant.
- While there, fix two stale points in `TAIPO.md`: its finger header is
  reversed (`r`/`a` are the pinky, `i`/`e` the index — see "Which column is
  the pinky" above), and the "Not currently supported: holding a modifier
  across multiple keys" paragraph predates the sticky-modifier work
  (`fe4ce59`), which `taipo.rs`'s doc comment describes.  Fix the "index to
  pinky" comments in `tests/taipo.rs` in the same commit.
- `TASKS.md`: add a Posh entry in the established style (what was done,
  decisions, tests, "needs testing on hardware"), and a follow-up item for the
  media/brightness keys (consumer-control HID report).

## Wrap-up

- Run `cargo test -p bbq-keyboard`; build with `cd jolt-embassy-rp && just
  build`.  Do not flash hardware or assume it works on-device; per project
  policy, changes require manual testing on the keyboard before they count as
  done.
- Finish by summarizing what the developer should verify by hand: tap the
  lower-left key in Taipo on the mesa1 and see LED 2 change; type the
  alphabet, digits, symbols and F-keys from the table on both hands; the
  `ralt` chord acting as one-shot Shift and double-press sticky; both thumbs
  releasing modifiers; pinky keys doing nothing in Posh; mode key and the
  escape-key tap still switching Steno↔Taipo, with Posh still selected on
  return; the taipo latch in steno mode typing Posh; the colors on LED 2 being
  tellable apart (and from LED 0's Taipo color); on the jolt3, the middle
  outer-left key toggling with no visible indication.
- Flag the judgement calls in the summary: the `+Sp`/`+Bk` column mapping,
  `ralt+shift` left unmapped, the LED colors, and the `TAIPO.md` finger-header
  correction.

## Process notes

- Follow `CLAUDE.md` conventions: incremental logical commits; don't mix
  refactoring/comment-only changes and functional changes in one commit;
  commit messages in simple present tense with a short summary line, a
  blank line, and a body wrapped at ~72 columns explaining what and why.
- Commits carry an `Assisted-by: Claude:<model>` trailer (per the user's
  global `CLAUDE.md`), with `<model>` derived from the model actually running.
- This file is `posh-task.md` rather than `posh.md` because macOS's
  filesystem is case-insensitive and `POSH.md` (Step 4) would collide with
  it.  It is listed in `.git/info/exclude`, not tracked; it can move to
  `docs/done/` when the work is complete, as the earlier plans did.
  `POSH.md`, by contrast, is a tracked file.
