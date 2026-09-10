# Multi-key chords for Taipo

I'd like to begin implementing the ability to add chords to taipo that support more than one key
pressed.

To begin with, I would like to add the chord 'ent' that will translate to 'th' (most common English
bigram). When with the inner thumb, it should type 'Th'.

The two main challenges here are how to represent sequences it the table, and how to incorporate
typing longer sequences with how taipo sends currently.

---

# Design

Everything below is the implementation plan for the note above.  It is written to be picked up
cold: read the "Where things are" section, then work the commits in order.

## Where things are

| what | where |
|------|-------|
| The taipo engine | [taipo.rs](bbq-keyboard/src/layout/taipo.rs) |
| The taipo chord table | `TAIPO_ACTIONS`, bottom of the same file |
| The posh chord table | [posh.rs](bbq-keyboard/src/layout/posh.rs) (`POSH_ACTIONS`) |
| Char → HID key table | [usb_typer.rs](bbq-keyboard/src/usb_typer.rs) (`KEY_TABLE`) |
| Tests | [tests/taipo.rs](bbq-keyboard/tests/taipo.rs), plus a `tests` module in each table file |
| Cheat sheet | [TAIPO.md](TAIPO.md) |

There is no workspace at the repo root; run the tests from the `bbq-keyboard` directory with
`cargo test`.  Nothing in this change touches the firmware crates: `KeyAction` is unchanged, so
`jolt-embassy-rp`, `jolt`, and `zbbq` need no edits.

## How taipo types today

The relevant part of `TaipoManager::tick` is the loop that drains `self.keys` (the queue of
`TaipoEvent` press/release pairs the two `SideManager`s produce).  A press event looks its chord
code up in the table and, for a letter, does three things:

```rust
self.release_nonmod(actions).await;                                          // 1
self.send(actions, is_steno, KeyAction::KeyPress(*k, self.oneshot)).await;   // 2
self.down = true;                                                            // 3
self.oneshot = self.sticky;
```

The matching release event later sends `KeyRelease` (or `ModOnly(self.oneshot)`, when sticky
modifiers are held), clears `self.down`, and clears `self.taipo_latch`.

Three invariants come out of that, and the design keeps all three:

- **At most one HID key is ever held.**  `self.down` is the whole of that state.  `KeyPress`
  carries exactly one keycode, so a sequence has to be several reports, not one.
- **The chord's key is left held until the chord is released.**  This is what gives a held chord
  the host's auto-repeat, and it is what makes the release event do the `taipo_latch = 0`
  bookkeeping that steno-mode layer-shifting depends on.
- **Every report is gated through `self.send`**, which drops it when in steno mode with no taipo
  latch held.

Downstream, each `KeyAction` becomes exactly one HID report: `send_key` pushes into an 8-deep
channel that the USB task drains one report per 1 ms poll (`jolt-embassy-rp/src/usb.rs`,
`send_usb`).  Sending press/release/press back to back is therefore already a supported thing —
the steno path does exactly that for whole words via `usb_typer::enqueue_action`.  No new
plumbing is needed below the layout layer.

## Challenge 1: representing a sequence in the table

**Decision: add `Action::Text(&'static str)`, with the characters looked up in the table
`usb_typer` already has.**

```rust
pub(super) enum Action {
    Simple(Keyboard),
    Shifted(Keyboard),
    /// Type a short sequence of characters, one keypress each.
    Text(&'static str),
    OneShot(Mods),
    Release,
}
```

so the two new entries read:

```rust
// Multi-character chords.
Entry { code: 0x04c, action: Action::Text("th"), },
Entry { code: 0x14c, action: Action::Text("Th"), },
```

Why a string rather than `Multi(&'static [Keyboard])`: the shift is per character.  `"Th"` is a
shifted `T` followed by a plain `h`, and a bare list of keycodes cannot say that without pairing
each with a `Mods`, at which point the table entries stop being readable.  A string says what gets
typed, which is the whole point of the feature, and `usb_typer::KEY_TABLE` already maps ASCII to
(key, shift) for the steno path — the same mapping the developer would otherwise be transcribing
by hand.

What this deliberately cannot express: sequences containing non-printing keys (arrows, Tab,
Escape).  If that is ever wanted, add a second variant rather than contorting this one; the
engine change below is written so that either funnels into one helper.

`Simple` and `Shifted` stay.  They are the hot path, they cover keys that have no character, and
rewriting a hundred entries as one-character strings would be churn with no gain.

### The chord

`ent` is the `e`, `n`, and `t` keys of one hand: `0x008 | 0x040 | 0x004` = **`0x04c`**.  With the
inner thumb (`Sp`, `0x100`, the thumb that types Space alone and that adds shift on every other
chord) that is **`0x14c`**.

Both codes are unused in `TAIPO_ACTIONS` — verify with a grep before adding, the `test_codes_unique`
test in posh has no taipo counterpart yet.  Note that Posh *does* use `0x04c`/`0x14c` (for `z`/`Z`);
this change is to the taipo table only, and the two tables are independent.

`0x24c` (`+Bk`) and `0x34c` (both thumbs) are left unmapped, for a later decision.

## Challenge 2: emitting the sequence

**Decision: type each character as its own keypress, releasing between them, and leave the last
character held.**  For `th` that is:

```
KeyPress(T, mods)   KeyRelease   KeyPress(H, mods)      ... then, on chord release: KeyRelease
```

Leaving the last one held is what preserves the three invariants above: the chord's release event
finds `self.down` set and finishes exactly as it does for a single-key chord, including clearing
the taipo latch.  Releasing everything inside the sequence would leave `self.down` false and
silently skip that bookkeeping.

The consequence to accept: **holding the chord repeats only the last character.**  Hold `ent` past
the chord window and the host types `th`, then auto-repeats `hhhh`.  This is a direct consequence
of the one-key-held model and is not worth engineering around; document it in TAIPO.md.

### The code

In `TaipoManager`, add:

```rust
/// Type a sequence of characters, one keypress per character.
///
/// Each character is its own HID report.  All but the last are released as
/// they are typed; the last is left held, so that the chord's release
/// finishes the sequence exactly as it finishes a single-key chord.
///
/// Characters the key table has no key for are skipped.
async fn type_text<ACT: LayoutActions>(
    &mut self,
    actions: &ACT,
    is_steno: bool,
    text: &'static str,
) {
    for ch in text.chars() {
        let Some((key, shift)) = key_for_char(ch) else { continue };
        self.release_nonmod(actions, is_steno).await;
        self.send(actions, is_steno, KeyAction::KeyPress(key, self.oneshot | shift)).await;
        self.down = true;
        self.oneshot = self.sticky;
    }
}
```

and dispatch to it from the table match in `tick`:

```rust
Some(Entry { action: Action::Text(text), .. }) => {
    self.type_text(actions, is_steno, *text).await;
}
```

The `*text` is the same deref the neighbouring arms do: the tables are `'static`, so the match
binds a `&&'static str` and the copy out of it releases the borrow.

That body is the existing `Simple` arm with a loop and a per-character shift around it, which is
the point: the modifier behaviour is not re-invented, it falls out.

### Modifiers

Because `self.oneshot = self.sticky` runs per character, the existing rule applies to each
keypress in turn.  Concretely:

- **Nothing held.**  `ent` → `t`, `h`.
- **One-shot Shift held, then `ent`.**  The first character gets the shift, and `oneshot` drops to
  `sticky` (empty) for the rest: `Th`.  The `0x14c` entry is the same result reached without the
  one-shot, which is the behaviour a Taipo user would predict.
- **Sticky Ctrl held, then `ent`.**  Every character gets it: Ctrl-T, Ctrl-H, with the inter-key
  release sent as `ModOnly(CONTROL)` so the host never sees Ctrl lift.

A shift coming from the *text* (`"Th"`) is or-ed with whatever is held, so a sticky Ctrl plus
`0x14c` gives Ctrl-Shift-T then Ctrl-H.

### The steno gate, which must be fixed first

`release_nonmod` currently calls `actions.send_key` **directly**, bypassing `self.send`:

```rust
async fn release_nonmod<ACT: LayoutActions>(&mut self, actions: &ACT) {
    if self.down {
        if self.oneshot.is_empty() { actions.send_key(KeyAction::KeyRelease).await; }
        else { actions.send_key(KeyAction::ModOnly(self.oneshot)).await; }
        self.down = false;
    }
}
```

Today that is unobservable: `self.down` is set even when `send` suppressed the press, but a press
event is always immediately followed by its release event, so `release_nonmod` never actually sees
`down` set.  A multi-character sequence breaks that — the second character calls `release_nonmod`
with `down` set from the first — and in steno mode with no taipo latch it would leak a bare
`KeyRelease` to the host in the middle of a steno stroke.

So: give `release_nonmod` an `is_steno` parameter and route both of its sends through `self.send`.
Update its two existing call sites.  This is a prerequisite, not an optional cleanup.

Leave the "`self.down` is set even when the send was suppressed" quirk alone; it is pre-existing,
it self-corrects on the next release event, and with `release_nonmod` gated it is no longer
reachable in a way that matters.

### Length limits

The key channel on the RP2040 boards is 8 deep and drains at 1 ms per report, so a two-character
chord (4 actions worst case) never blocks.  A sequence long enough to fill it would make
`layout_loop` await inside a tick, which in turn stalls the matrix channel (depth 1) for a
millisecond or two.  The steno path already accepts exactly this trade-off when it types a word,
so it is not a new hazard, but **keep table sequences short** — a handful of characters.  If long
sequences ever become interesting, the fix is a pending-sequence buffer in `TaipoManager` drained
one keypress per `tick`; `type_text` is deliberately the only place that would need to change.

## Commits

Per the repo guidelines, refactors land separately from the functional change.

**1. `usb_typer`: extract the character lookup.**  Add

```rust
/// The keypress that types `ch`, if there is one.
///
/// The modifiers are those needed to produce the character itself — shift, or
/// nothing.  Characters outside ASCII, and those the table has no key for,
/// return `None`.
pub fn key_for_char(ch: char) -> Option<(Keyboard, Mods)> {
    if ch >= (128 as char) {
        return None;
    }
    let code = KEY_TABLE[ch as usize];
    if code == NONE {
        return None;
    }
    let mods = if code & SHIFT != 0 { Mods::SHIFT } else { Mods::empty() };
    Some((((code & 0xff) as u8).into(), mods))
}
```

and rewrite the lookup inside `enqueue_action` to call it.  Change only the lookup — leave that
function's loop structure and its `last_action` handling exactly as they are, so the commit is
provably behaviour-preserving.

**2. `TAIPO_ACTIONS` as a slice.**  It is declared `static TAIPO_ACTIONS: [Entry; 126]`, so every
entry added has to bump a hand-maintained count.  Change it to `static TAIPO_ACTIONS: &[Entry] =
&[...]` and drop the `&` at the use in `TaipoManager::actions`, matching how `POSH_ACTIONS` is
already declared.

**3. Gate `release_nonmod` through `send`.**  As described above.  Say in the message that the
behaviour is currently unobservable and that the next commit is what makes it matter.

**4. Multi-character chords.**  `Action::Text`, `type_text`, the `tick` arm, the two `ent`
entries, the tests, and the docs.  Also add a `Text` arm to `same_action` in the `posh` test module
(it matches on `Action` exhaustively and will stop compiling otherwise) — `(Action::Text(x),
Action::Text(y)) => x == y`.

## Tests

In `bbq-keyboard/tests/taipo.rs`, using the existing `Script` builder.  `chord()` presses, waits
out the chord window, releases, and ticks; expectations pop in order.

```rust
/// The `ent` chord types the `th` bigram: two keypresses from one chord, with
/// the last left held until the chord is released.
#[test]
fn test_multi_th() {
    let mut script = Script::taipo();
    script
        .chord(LEFT, E | N | T)
        .presses(Keyboard::T, Mods::empty())
        .releases()
        .presses(Keyboard::H, Mods::empty())
        .releases();
    script.run();
}
```

The set to write:

- `test_multi_th` — as above.
- `test_multi_th_tapped` — same, but with `tap()` instead of `chord()`, so the chord is committed
  by its release rather than by the timer.  Same output.
- `test_multi_th_capital` — `chord(LEFT, SP | E | N | T)` → `presses(T, SHIFT)`, `releases()`,
  `presses(H, empty)`, `releases()`.
- `test_multi_oneshot_shift` — one-shot Shift (`chord(RIGHT, I | E)`), then `ent` on the left.
  Expect `mod_only(SHIFT)`, then the same four actions as `test_multi_th_capital`.
- `test_multi_sticky_mod` — sticky Ctrl (the `n+t` chord twice), then `ent`.  Expect
  `presses(T, CONTROL)`, `mod_only(CONTROL)`, `presses(H, CONTROL)`, `mod_only(CONTROL)`.  This is
  the test that pins the inter-character release down as `ModOnly`, not `KeyRelease`.
- `test_multi_in_steno` — `Script::steno()`, then `ent`; `idle()`.  This is the regression test for
  commit 3: without it the second character leaks a `KeyRelease`.
- `test_multi_steno_latched` — the same in steno mode but with the taipo shift key held, expecting
  the full sequence to type through.  Model it on the existing latch tests.
- `test_multi_rollover` — `ent` immediately followed by another chord on the same hand, checking
  that the held `h` is released before the next key.

Also extend the two table tests: `test_codes_unique` should exist for `TAIPO_ACTIONS` too (add it
to a `tests` module in `taipo.rs` if there isn't one), and it will catch a mistyped `0x04c`.

## Docs

- **TAIPO.md** — a new "Multi-character chords" section after "Three-finger chords and thumbs":
  the `ent` chord (n top, t and e bottom), that it types `th` and `Th` with `Sp`, that held
  modifiers apply to the first character (all of them, if sticky), and that holding the chord
  repeats only the last character.
- **taipo.rs module docs** — a short paragraph next to the "Modifiers:" one, explaining that a
  table entry may type more than one character and that the last one is what stays held.
- **TASKS.md** — an entry in the same style as the other completed ones, including what still
  needs hardware testing: whether `th`/`Th` feel right in real typing, whether the chord collides
  with anything in the roll patterns the developer actually uses, and whether the auto-repeat
  behaviour is annoying in practice.

## Out of scope

- Posh entries.  The mechanism is in the shared engine, so a `Text` entry works in `POSH_ACTIONS`
  too, but Posh already uses `0x04c` for `z`; choosing chords there is a separate decision.
- More bigrams.  Land `th` first, use it, then decide.
- Sequences containing non-printing keys, and sequences long enough to need pacing.  Both are
  noted above with the shape of the change they would need.
