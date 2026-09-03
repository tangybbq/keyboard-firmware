# Mesa 2 support, and chorded Taipo/Dosh switching

This is a work plan for two related changes.  They are related only because
the mesa2 forces the second one: the board has just the 20 Taipo keys, so
every key the layout manager currently handles itself — the mode key, the
row-position toggle, the Taipo/Dosh toggle — is physically gone.

1. **Board support for the mesa2.**  Electrically it is a mesa1: one Tiny 2040
   scanning a unibody board, four ws2812 LEDs, no inter-board link.  The matrix
   is wired differently, and it has 20 keys rather than 30.
2. **A chord that selects the Taipo variant**, so the choice survives on a
   board with no spare keys.  `rsni` selects Taipo, `aote` selects Dosh.

The two parts are independent and should be separate commits.  Part 2 can be
done first and tested on the mesa1, which is what the developer intends to do.

Read this whole document before starting.  The hardware facts in part 1 were
read out of the KiCad board rather than typed from memory; the derivation is
given so they can be re-checked rather than trusted.

---

## Requirements (from the developer, verbatim)

- I have a new keyboard design coming, the mesa2.
- We need to add support to it in the firmware.  It is very similar to mesa1
  electrically, but the matrix is hooked up differently.
- The mesa2 only has the 20 keys defined for Taipo.  That means we will need to
  reserve a chord itself to switch between Taipo and Dosh.
- I'm thinking of trying 'rsni' for Taipo and 'aote' for dosh.  We can change
  these later if I am too likely to type them.
- But I can test the new switch chords on the mesa1.

---

# Part 1 — mesa2 board support

## Where the hardware facts come from

The board lives outside this repository, at
`~/Documents/Keyboards/mesa2/`.  Two files matter:

- `LAYOUT.md` — the key geometry, and a "Matrix → finger" section that names
  the nets.
- `mesa2/mesa2.kicad_pcb` — the authoritative netlist.  Every pad carries its
  net name, so the matrix can be read off directly.

To re-derive the tables below, list each footprint's pads with their nets:

```sh
cd ~/Documents/Keyboards/mesa2/mesa2
python3 - <<'EOF'
import re
s = open('mesa2.kicad_pcb').read()
for p in s.split('(footprint ')[1:]:
    m = re.search(r'\(property "Reference"\s+"([^"]+)"', p)
    ref = m.group(1) if m else '?'
    if not ref.startswith(('SW_', 'D_', 'A1')):
        continue
    pads = []
    for pm in re.finditer(r'\(pad\s+"([^"]*)"', p):
        seg = p[pm.end():pm.end() + 1200]
        nm = re.search(r'\(net\s+"([^"]*)"\)', seg)
        if pm.group(1):
            pads.append((pm.group(1), nm.group(1) if nm else None))
    print(ref, pads)
EOF
```

**Do not use `mesa2.dsn`.**  It is stale — it still holds the mesa1 netlist
(`L-STAR0`, `LF1`, …), and will send you somewhere very wrong.

## The board

A unibody 20-key board, one Pimoroni Tiny 2040 (`A1`) scanning all of it, four
ws2812 LEDs, an SWD header (`J2`) and a reset button.  Like the proto4 and the
mesa1, it is `Inter::None`: physically one piece, so there is no inter-board
protocol at all.  `two_row: true`.

## Pin assignment

The Tiny 2040's analog pads are `A0`=GP26, `A1`=GP27, `A2`=GP28, `A3`=GP29 —
the same identification the existing `mesa1` module relies on (it uses GP26 for
RGB and GP27/28/29 for three of its columns).

| Tiny 2040 pad | RP2040 | net       | role                |
|---------------|--------|-----------|---------------------|
| `0`           | GP0    | `ROW_C`   | driven              |
| `1`           | GP1    | `ROW_D`   | driven              |
| `2`           | GP2    | —         | unconnected         |
| `3`           | GP3    | —         | unconnected         |
| `4`           | GP4    | `COL_1`   | sensed              |
| `5`           | GP5    | `COL_2`   | sensed              |
| `6`           | GP6    | `COL_3`   | sensed              |
| `7`           | GP7    | `COL_4`   | sensed              |
| `A0`          | GP26   | `RGB`     | ws2812 data         |
| `A1`          | GP27   | `ROW_B`   | driven              |
| `A2`          | GP28   | `ROW_A`   | driven              |
| `A3`          | GP29   | `COL_5`   | sensed              |

## Which way the diodes point — this is the difference from the mesa1

Each key is `ROW_x → SW pad 1 … SW pad 2 → diode pad 2 → diode pad 1 → COL_n`.
KiCad names the intermediate net after the diode pin on it, and it comes out as
`Net-(D_LR1-A)` — the **anode**.  So pad 2 is the anode, pad 1 the cathode, and
the diode conducts **from the row to the column**.

The scanner drives the anode side and senses the cathode side.  On the mesa2
that means **drive `ROW_A..ROW_D`, sense `COL_1..COL_5`**.

This is the inversion the developer meant by "hooked up differently".  On the
mesa1 the switch sits on `COL_n` and the cathode on `ROW_x`, so that board
drives its columns and senses its rows — and its `board.rs` module carries a
long comment apologising for the swap.  The mesa2 needs the same comment with
the roles the other way round: the scanner's `cols` argument is the mesa2's
`ROW_A..D`, and the scanner's `rows` argument is its `COL_1..COL_5`.

Getting this backwards is not a subtle failure — nothing will scan at all — so
it is safe to just try it, but it is worth writing down correctly the first
time.

## The matrix

Five columns are shared by both hands; each hand has two rows.  `COL_5` is the
thumb pair.

| | `COL_1` | `COL_2` | `COL_3` | `COL_4` | `COL_5` |
|---|---|---|---|---|---|
| `ROW_A` (left, far)  | `SW_LR1` | `SW_LS1` | `SW_LN1` | `SW_LI1` | `SW_LSP1` |
| `ROW_B` (left, near) | `SW_LA1` | `SW_LO1` | `SW_LT1` | `SW_LE1` | `SW_LBK1` |
| `ROW_C` (right, far) | `SW_RI1` | `SW_RN1` | `SW_RS1` | `SW_RR1` | `SW_RBK1` |
| `ROW_D` (right, near)| `SW_RE1` | `SW_RT1` | `SW_RO1` | `SW_RA1` | `SW_RSP1` |

This agrees with `LAYOUT.md`: the two hands number their columns in opposite
order (left `COL_1..4` = pinky, ring, middle, index; right `COL_1..4` = index,
middle, ring, pinky) so that each finger carries the same key pair on both
hands.  The switch reference names are already the Taipo letter names, which
makes the table below almost a transcription.

## Scan codes

`Matrix::new(cols, rows, …)` numbers a position `driven_index * rows.len() +
sensed_index`.  With drivers `[ROW_A, ROW_B, ROW_C, ROW_D]` and sensors
`[COL_1 … COL_5]` that is `row * 5 + col`, giving 20 codes, 0..19, with no
holes — the first board where every matrix position is a real key.

The translation maps those to the proto3 key code numbering that
`SCAN_MAP` and the whole layout stack speak:

| scan | switch    | proto3 code | Taipo key |
|------|-----------|-------------|-----------|
| 0    | `SW_LR1`  | 4           | L-r       |
| 1    | `SW_LS1`  | 8           | L-s       |
| 2    | `SW_LN1`  | 12          | L-n       |
| 3    | `SW_LI1`  | 16          | L-i       |
| 4    | `SW_LSP1` | 19          | L-Sp      |
| 5    | `SW_LA1`  | 5           | L-a       |
| 6    | `SW_LO1`  | 9           | L-o       |
| 7    | `SW_LT1`  | 13          | L-t       |
| 8    | `SW_LE1`  | 17          | L-e       |
| 9    | `SW_LBK1` | 23          | L-Bk      |
| 10   | `SW_RI1`  | 40          | R-i       |
| 11   | `SW_RN1`  | 36          | R-n       |
| 12   | `SW_RS1`  | 32          | R-s       |
| 13   | `SW_RR1`  | 28          | R-r       |
| 14   | `SW_RBK1` | 47          | R-Bk      |
| 15   | `SW_RE1`  | 41          | R-e       |
| 16   | `SW_RT1`  | 37          | R-t       |
| 17   | `SW_RO1`  | 33          | R-o       |
| 18   | `SW_RA1`  | 29          | R-a       |
| 19   | `SW_RSP1` | 43          | R-Sp      |

Cross-check the right-hand column against `SCAN_MAP` in
`bbq-keyboard/src/layout/taipo.rs`: 4/5/8/9/12/13/16/17/19/23 are the left
hand's `r a s o n t i e Sp Bk`, and 28/29/32/33/36/37/40/41/43/47 the right
hand's.  Every other proto3 code has no key on this board.

## Files to change

### `bbq-keyboard/src/translate.rs`

- Add `"mesa2"` to `BOARDS`.
- Add `"mesa2" => mesa2,` to `get_translation`.
- Add a `static MESA2: [u8; 20]` from the table above, plus
  `fn mesa2(code: u8) -> u8` following the `mesa1` pattern
  (`*MESA2.get(code as usize).unwrap_or(&255)`).
- Doc comment: 20 keys, exactly the Taipo set; shares five columns between the
  hands and gives each hand two rows; the scanner drives the *rows* and senses
  the *columns*, which is the opposite of the mesa1, so scan codes run
  `ROW_x * 5 + COL_n`.

`test_boards_translate` covers the new entry with no change.

### `jolt-embassy-rp/src/board.rs`

- Add `mod mesa2`, modelled closely on `mod mesa1` — same LED driver (4 ws2812
  on GP26 through PIO0/DMA_CH0), same USB init, `Inter::None`, `two_row: true`,
  `Side::Left` passed to `Matrix::new` because one MCU scans everything and
  there is no bias to apply.
- `MatrixResources` should name the mesa2's nets, as the mesa1 module names
  its own: `row_a: PIN_28`, `row_b: PIN_27`, `row_c: PIN_0`, `row_d: PIN_1`,
  `col_1: PIN_4`, `col_2: PIN_5`, `col_3: PIN_6`, `col_4: PIN_7`,
  `col_5: PIN_29`.
- In `matrix_init`, the **driven** array is `[row_a, row_b, row_c, row_d]` and
  the **sensed** array is `[col_1 … col_5]`.  Comment the swap.
- Add the dispatch arm in `Board::new`:
  `BoardInfo { name, side: None } if name == "mesa2"`, with the same four
  startup LED colors the proto4 and mesa1 use.

Note there is no `Side` in the board info: like the proto4 and mesa1, the
mesa2 is `side: None`.

### `bbq-tool/sides.sh`

Add `gen_files mesa2 --name mesa2` so the board info blob can be built and
flashed.

### `AGENT.md` / `CLAUDE.md`

The "Firmware implementations" bullet lists the board modules and names the
`Inter::None` boards.  Add mesa2 to both lists.

### Regenerate `bbq-keyboard/layouts.json`

`boards_json` walks `BOARDS`, so adding mesa2 changes the checked-in document
and the layout fingerprint.  Regenerate and commit it:

```sh
cd bbq-keyboard && cargo run --example gen-layouts > layouts.json
```

`tests/layouts_json.rs` fails until this is done.  The fingerprint changing is
correct and expected — it is exactly what tells a host replay that the tables
moved.

## Bring-up

`just build-debug` in `jolt-embassy-rp` logs the raw and translated scan code
for every press over RTT, which is the tool for this.  Flash the board info
first (`bbq-tool/sides.sh` output, `data/mesa2.elf` via gdb or the uf2), then
press each key in a known order and check the raw code against the scan column
of the table above and the translated code against the proto3 column.

The mesa1 bring-up memo is worth re-reading: the failure there was mechanical
(unsoldered socket legs that the meter's probe pressure hid), not a firmware
mistake, and the netlist and firmware were correct the whole time.  Expect a
key that does nothing to be the board, not this table — but confirm the
matching row and column both work through their other keys before reaching for
a soldering iron.

---

# Part 2 — selecting the Taipo variant with a chord

## Why

Today the variant is toggled by tapping the steno `#` key of the outer left
column (`DOSH_TOGGLE_KEY`, proto3 code 1), handled in `LayoutManager::dosh_event`.
The mesa2 has no such key.  Nor does it have the mode key (code 2) or the
row-position toggle (code 0), but those do not matter: a taipo-only build has
nothing to switch modes to, and a 2-row board never moves rows.  The variant is
the one runtime choice that has to survive.

## The chords

Taipo's chord bits, from `TAIPO.md` and the module docs:

| bit     | pinky | ring | middle | index |
|---------|-------|------|--------|-------|
| top row | `0x010` r | `0x020` s | `0x040` n | `0x080` i |
| bottom  | `0x001` a | `0x002` o | `0x004` t | `0x008` e |

So the developer's two chords are the two full rows of one hand:

- **`rsni` = `0x0f0`** — all four fingers on the top row — selects **Taipo**.
- **`aote` = `0x00f`** — all four fingers on the bottom row — selects **Dosh**.

Both are free today, in both tables, and so are all four thumb variants of each
(`0x1f0`, `0x2f0`, `0x3f0`, `0x10f`, `0x20f`, `0x30f`).  Verified by grep; the
implementing agent should re-check before adding entries rather than assume
this document is still current.

Two things fall out of the choice that are worth stating, because they are the
reason it is a good one:

- Each chord uses a **pinky**, and Dosh uses no pinky at all.  So in Dosh both
  chords are guaranteed dead whatever else changes in that table — the only
  way to collide is to add a pinky chord to Dosh, which would defeat the point
  of Dosh.
- The shape is memorable without knowing which table is live: top row means
  Taipo, bottom row means Dosh, in both layouts.  The letter names are Taipo's;
  in Dosh the same physical keys spell something else, and that is fine because
  the chord is named by its shape.

**Selection, not toggling.**  `rsni` always selects Taipo and `aote` always
selects Dosh, regardless of what is current.  Pressing the chord you are
already in does nothing.  This is better than a single toggle chord: there is
no state to remember, and no way to end up inverted after a chord that was not
felt.  It costs one extra entry per table.

Only the bare chords are mapped.  The thumb variants stay unmapped, so a chord
with a thumb accidentally included does nothing rather than switching layouts.

**The risk the developer already named.**  Four fingers landing on one row is a
shape a sloppy roll could produce, and today that types nothing at all — it is
a pure error.  After this change it silently changes the layout, which is a
much worse failure than typing nothing.  If it happens in practice, changing
the chords is a two-line edit in each table; the mechanism does not care what
the codes are.  Something like `0x3f0`/`0x30f` (the same row plus both thumbs)
would be far harder to hit by accident, and is the obvious fallback.

## Design: a new `Action`

Put the switch in the chord tables as a new action, rather than special-casing
the codes in `LayoutManager`:

```rust
pub enum Action {
    // …
    /// Select a chord table.  The change takes effect for the next chord;
    /// this one was already looked up in the table that named it.
    Variant(TaipoVariant),
}
```

Four new entries — in `TAIPO_ACTIONS` and in `DOSH_ACTIONS`, the same pair in
each:

```rust
Entry { code: 0x0f0, action: Action::Variant(TaipoVariant::Taipo), },
Entry { code: 0x00f, action: Action::Variant(TaipoVariant::Dosh), },
```

This is the right shape because everything downstream already walks the tables:
the fingerprint, `layouts.json`, and the host replay all pick the new action up
by construction, and a chord that switches variant is visible to the key-log
analysis as a chord rather than as a mysterious gap.  Special-casing the codes
in `LayoutManager` would hide it from all three.

### Handling it in `TaipoManager::tick`

Add an arm to the match over `entry` in `tick`:

- Set `self.variant`.
- Report the change through `actions.set_sub_mode(MinorMode::Dosh)` /
  `clear_sub_mode`, **only when it actually changes**.  This is not optional
  bookkeeping: `Recorder` in `replay.rs` tracks the variant solely through
  those two calls and uses it to choose which table to look chords up in, and
  `dispatch.rs` uses them to drive the LED and to write the `Marker::Variant`
  record into the key log.  A silent change would make every later chord in a
  replay resolve against the wrong table.
- Do **not** touch `oneshot`, `sticky`, `down`, or the taipo latch.  The chord
  types nothing, so there is nothing to release; leaving `self.down` false means
  the matching release event does nothing, which is what we want.

**Gate it the same way keys are gated.**  `send` drops everything when
`is_steno && self.taipo_latch == 0`; the variant switch should follow the same
rule, so that in steno mode the chord only means anything when the taipo latch
is held — exactly like every other taipo chord in that mode.  Factor the
predicate into a small helper (`fn active(&self, is_steno: bool) -> bool`) and
use it in both places rather than repeating it.

**Ordering is already correct.**  `actions.taipo_chord(…)` is awaited before
the table lookup, so the chord is recorded against the variant it was looked up
in, and the switch lands after it.  Do not move that call.

### Everything the new `Action` variant touches

The compiler will find most of these — the matches are exhaustive — but the
list is here so none is answered with a lazy wildcard arm:

| file | what to add |
|---|---|
| `bbq-keyboard/src/layout/taipo.rs` | the `Action` variant, the two entries, the `tick` arm, the `active` helper |
| `bbq-keyboard/src/layout/dosh.rs` | the two entries |
| `bbq-keyboard/src/layout/fingerprint.rs` | `Action::Variant` → tag byte `5`, plus a byte for which variant.  Leave `SCHEME` at 1: the encoding scheme is unchanged, only the table content, and the fingerprint moving is the point |
| `bbq-keyboard/src/layout/export.rs` | `{ "kind": "variant", "variant": "taipo" \| "dosh" }` |
| `bbq-keyboard/src/replay.rs` | `ChordAction::Variant(TaipoVariant)`, the `of()` arm, and the `format!` arm near the bottom of the file |
| `taipo-analyze/src/stats.rs` | `typed_text` returns `None` for it, and it must not count as a typing chord in the `matches!` around line 186 |
| `bbq-keyboard/layouts.json` | regenerate |
| `TAIPO.md`, `DOSH.md` | document the two chords in both cheat sheets |

### `layouts.json` format version

`export.rs` has `FORMAT_VERSION: u32 = 1`, with the rule that a new field with
an obvious meaning does not need a bump but a re-interpreted one does.  A new
`kind` of action is a judgement call.  **Recommendation: bump it to 2.**  A
consumer that switches on `kind` and falls through to "types nothing" would
silently mis-model a chord that changes the layout out from under it, which is
the same class of quiet wrongness the fingerprint exists to prevent.  Flag the
bump in the commit message so the developer can overrule it cheaply.

### Keep the key toggle

`DOSH_TOGGLE_KEY` and `dosh_event` stay exactly as they are.  On the 3-row
boards and on the mesa1 the lower-left key keeps working; the chord is an
addition, not a replacement.  Both paths converge on
`set_sub_mode`/`clear_sub_mode`, so the LED, the key log and the replay do not
need to know which one fired.

One asymmetry to note in the commit message, not to fix: the key toggles, the
chords select.  Making the key select is impossible (there is only one of it),
and making the chords toggle would throw away the property that makes them
good.

## Tests

In `bbq-keyboard/tests/taipo.rs`, alongside the existing
`test_row_toggle_*` and the Dosh toggle tests:

- `rsni` while in Dosh switches to Taipo, and emits
  `Actions::ClearSubMode(MinorMode::Dosh)`.
- `aote` while in Taipo switches to Dosh, and emits
  `Actions::SetSubMode(MinorMode::Dosh)`.
- The chord you are already in emits **nothing** — no redundant sub-mode call.
- The chord itself types no keys.
- The chord following the switch is looked up in the **new** table.  This is
  the test that would catch a switch applied one chord too early or too late:
  chord `aote`, then a chord whose two tables disagree, and check what was
  typed.
- A thumb variant (`0x1f0`) still does nothing.

In `bbq-keyboard/tests/replay.rs`, one test that a log containing the switch
chord replays with subsequent chords resolved against the new table, and that
the switching chord itself is reported with the *old* variant.

## Testing on the mesa1

All 20 Taipo keys are present on the mesa1, so both chords work there
unchanged — this is what the developer plans to do before the mesa2 arrives.
Worth checking by hand:

- Both chords, both hands, both directions, watching LED 2 for the variant.
- The lower-left key toggle still works and agrees with the LED.
- Both thumb variants of each chord do nothing.
- How often the chords fire by accident during ordinary typing.  This is the
  real question the mesa1 is being used to answer, and the key log plus
  `taipo-analyze` can answer it retroactively: the chords appear as
  `Derived::Variant` in the replay.

---

## Commit plan

Four commits, in this order.  Parts 1 and 2 do not depend on each other, but
part 2 first means the mesa1 testing can start immediately.

1. **Add the Taipo variant selection chords.**  `Action::Variant`, the four
   table entries, the `tick` arm, and the tests.  Touches the fingerprint, so
   `layouts.json` is regenerated in this commit.
2. **Teach the host tools about the variant action.**  `replay.rs`,
   `export.rs`, `taipo-analyze`, and the `FORMAT_VERSION` bump — if the
   compiler forces these into commit 1 to keep the tree building, fold them in
   and say so.
3. **Add the mesa2 scan code translation.**  `translate.rs` plus the
   `layouts.json` regeneration and the `sides.sh` entry.
4. **Add mesa2 board support.**  `board.rs`, and the `AGENT.md`/`CLAUDE.md`
   board lists.

Per the project's testing convention: none of this can be verified on hardware
before it is committed, so commit it and say plainly in the response what was
compiled, what was covered by tests, and what is waiting on the developer's
review — in particular that no mesa2 exists yet to run commit 4 against.

## Open questions

- **The chords may be too easy to type by accident.**  Named by the developer,
  answered by using it.  The fallback is `0x3f0`/`0x30f`.
- **`FORMAT_VERSION`.**  Recommended bump to 2; the developer's call.
- **The mesa2 board files are still moving.**  `LAYOUT.md` lists the outline,
  the diode placement and the socket clearance as open, and the board was last
  saved 2026-08-28.  The *matrix* is unlikely to change, but re-run the netlist
  extraction above before writing `board.rs` rather than trusting the tables
  here, and say in the commit message which revision of the board they came
  from.
