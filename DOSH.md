# Dosh cheat sheet

This describes the Dosh implementation in `bbq-keyboard/src/layout/dosh.rs`. Dosh is a local
layout, derived from Posh (<https://inkeys.wiki/en/keymaps/posh>), a Taipo-style layout by the
same community, described there as "a taipo style layout that excludes the pinkies in order to
make combos more accurate and long periods of work more comfortable". It runs on the same engine
as Taipo, with the same chord timing and the same modifier behavior; only the chord table
differs. See TAIPO.md for the layout it ultimately derives from.

**Dosh keeps the lower pinky key**, which is the difference from Posh that matters. What that key
buys is not more chords but agreement with Taipo: every letter Taipo types without its *upper*
pinky sits on exactly the chord Taipo has it on. Nineteen of the twenty-six letters are shared,
and only seven have to be learned twice. See "Differences from Taipo" below.

The other differences from the Posh wiki: both thumbs together is Taipo's "release modifiers"
null key rather than a sticky shift, the wiki's `ralt` chord is Shift, and the media, volume and
brightness keys are not mapped. Two more local swaps: `s` and `h` trade their punctuation
layers, and `o` and `e` trade their navigation layers.

## Physical layout

Each hand has 7 finger keys and 2 thumb keys. The hands are identical (mirrored); chords are
formed within one hand, and you can alternate hands freely — even for double letters. Each key is
named by the letter it types alone, which is also the name Taipo gives it:

| finger:    | pinky    | ring | middle | index |
|------------|----------|------|--------|-------|
| top row    | (unused) | s    | n      | i     |
| bottom row | a        | o    | t      | e     |

**The upper pinky key is dead.** A chord that includes it matches nothing and types nothing (the
one exception is `r+s+n+i`, which selects the Taipo table; see below). Taipo calls that key `r`,
and this document does too when it has to name it.

Thumbs: **Sp** and **Bk** are Taipo's names for the two thumb keys, and this document keeps
them, the same way it keeps `r` for the dead upper pinky — they name the physical key, not what
Dosh does with it, and the JSON export, the event logs and the drills all speak them.

**Dosh has the two swapped**, which is the wiki's arrangement and the mirror of Taipo's: the `Bk`
key types Space here, and the `Sp` key types Backspace. The layers follow the thumb's function
rather than the key's name, again as in Taipo: the space thumb (**+Bk**) gives capitals, the
backspace thumb (**+Sp**) gives the digits, symbols and navigation keys, and **+Sp+Bk** gives the
function keys and the remaining symbols. So nothing about which chord means what changed with the
swap — only which thumb the hand reaches for.

Both thumbs pressed alone releases any held modifiers without typing anything.

## Single keys

+Sp turns six of the seven keys into the navigation cluster, and both thumbs into the far-motion
keys. Arrow mnemonic: up and down are the middle finger's two keys, and left and right are the
bottom row's inner two, with the index finger giving left and the ring finger right. (The wiki
has those two the other way around.) That leaves the index top key for Enter and the lower pinky
for Escape.

`s` is the exception: it carries the period and the double quote, which it kept when it moved
here from `n+i`.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| a | a | A | Escape | Delete (fwd) |
| o | o | O | → | End |
| t | t | T | ↓ | Page Down |
| e | e | E | ← | Home |
| s | s | S | `.` | `"` |
| n | n | N | ↑ | Page Up |
| i | i | I | Enter | Tab |

## The comma

The one same-row pair still carrying punctuation.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| t+e | h | H | `,` |  |

## The apostrophe

A bare chord of its own. It was `h`'s both-thumbs layer — two fingers and both thumbs for
one of the commonest marks in English — and it measured accordingly: 1330ms and a fifth of
its uses taken back, where the letters around it sat at 300ms and a twentieth.

`+Bk` is deliberately left free. It is the cheapest thing still unspoken for on this shape,
and it is being kept for a chord that needs to be cheap.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| t+e+i | `'` |  |  |  |

## Letters whose +Sp is a digit

Both thumbs gives the matching function key.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| e+n | r | R | 0 | F10 |
| a+e | d | D | 1 | F1 |
| a+o | l | L | 2 | F2 |
| o+e | c | C | 3 | F3 |
| o+t | u | U | 4 | F4 |
| e+s | m | M | 5 | F5 |
| a+i | w | W | 6 | F6 |
| s+i | f | F | 7 | F7 |
| o+n | g | G | 8 | F8 |
| n+i | y | Y | 9 | F9 |

## Letters whose +Sp is a symbol

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| s+n | p | P | `+` | `=` |
| o+t+e | b | B | `-` | `_` |
| e+s+n | v | V | `/` | `\` |
| o+i | k | K | `;` | `\|` |
| a+n | j | J | `:` | `*` |
| t+e+s | x | X | `$` | `#` |
| a+t | q | Q | `@` | F11 |
| t+e+n | z | Z | `&` | F12 |

## Punctuation-only chords

These have no both-thumbs variant.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| t+s | `?` | `!` | `^` |  |
| o+e+i | `` ` `` | `~` | `%` |  |

## Modifiers and brackets

**The modifiers are Taipo's.** Three of the four are the same-finger vertical pairs, exactly where
Taipo has them: index is Shift, middle is Control, ring is Alt. Taipo's GUI is the *pinky* pair,
which needs the upper pinky, so here it is the Alt chord plus the lower pinky instead.

Shift has no both-thumbs variant, as it would be shift plus shift.

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| e+i | Shift | `]` | `[` |  |
| t+n | Ctrl | `)` | `(` | Ctrl+Shift |
| o+s | Alt | `}` | `{` | Alt+Shift |
| a+o+s | GUI |  |  | GUI+Shift |
| o+t+s |  | `>` | `<` |  |

The brackets stay on the chords they were learned on rather than following the modifiers that
moved, so the mnemonic is still positional: `()` on the middle finger, `[]` on the index, `{}` on
the ring. The last row is the wiki's `ralt` chord, which kept the angle brackets but no longer
has a modifier on its base.

## Extras

| chord | alone | +Bk | +Sp | +Sp+Bk |
|-------|-------|-----|-----|--------|
| o+t+n | Print Screen |  |  |  |
| e+s+i | Insert |  |  |  |

The thumb layers of these two are the volume, mute and brightness keys, which need a
consumer-control HID report the firmware doesn't have yet.

## Differences from Taipo

**The thumbs are swapped** — see "Physical layout" above. That is the one difference that touches
every chord rather than a few of them: a Dosh capital is the thumb Taipo puts its digits on, and
the other way round.

The finger keys are a closer match. Nineteen letters are on the same chord in both layouts, and
need learning only once:

| chord | letter |     | chord | letter |     | chord | letter |
|-------|--------|-----|-------|--------|-----|-------|--------|
| a | a |  | a+o | l |  | o+t | u |
| o | o |  | a+t | q |  | o+e | c |
| t | t |  | a+e | d |  | o+i | k |
| e | e |  | a+n | j |  | t+e | h |
| s | s |  | a+i | w |  | s+i | f |
| n | n |  |       |   |  | s+n | p |
| i | i |  |       |   |  | n+i | y |

Seven still differ. Six of them are on Taipo's upper pinky, so Dosh cannot reach them at all:

| letter | Dosh chord | Taipo chord |
|--------|------------|-------------|
| r | e+n | `r` alone |
| b | o+t+e | r+s |
| g | o+n | r+i |
| m | e+s | e+r |
| x | t+e+s | t+r |
| z | t+e+n | r+n |

The seventh is `v`, which is the only one that isn't forced. Taipo types `v` with `e+s`, and Dosh
has `m` there; Taipo's `m` needs the upper pinky, so the two cannot simply trade. Dosh's `v` sits
on `e+s+n` instead, one of the chords freed by the letters that moved.

| letter | Dosh chord | Taipo chord |
|--------|------------|-------------|
| v | e+s+n | e+s (Dosh's `m`) |

Taipo's multi-character chords (`the`, `in`, `ing`, …) have no Dosh equivalent, and Taipo's
punctuation layers are different throughout; only the letters are shared.

## Chords left free

The move to Taipo's chords vacated seven chords, which are deliberately unmapped:

| chord | held in Posh |
|-------|--------------|
| s+n+i | `p` |
| t+i | `l` |
| e+n+i | `l` (the local alias) |
| o+t+i | `j` |
| o+n+i | `y` |
| o+e+s | `q` |
| o+e+n | `v`, until it moved to `e+s+n` |

`e+s+n` held Posh's `k`, and now holds `v`. The base of `o+t+s` is free as well, though the chord
still carries the angle brackets on its thumb layers.

## Modifiers: how they behave

Identical to Taipo — see the "Modifiers: how they behave" section of TAIPO.md. In short: a
modifier chord is sent immediately and stays held until the next key consumes it, modifiers
accumulate, pressing a modifier chord whose modifiers are all already held makes the whole held
set sticky, and both thumbs together releases everything.

## Switching between Taipo and Dosh

While in Taipo mode, tapping the **lower-left key by itself** switches between the Taipo and Dosh
chord tables. This is the steno `#` key of the outer left column, which has no meaning in Taipo;
on the 3-row jolt3 it is the middle key of that column rather than the lower one. Only a solo tap
counts — pressed with nothing else down and released with nothing else down — so the table can
never change in the middle of a chord.

Two chords do the same job, and are in both tables, so either can be reached from either:

| chord     | keys                 | selects |
|-----------|----------------------|---------|
| `r+s+n+i` | the whole top row    | Taipo   |
| `a+o+t+e` | the whole bottom row | Dosh    |

They **select** rather than toggle — the one you are already in does nothing — and the thumb
variants are unmapped, so a thumb accidentally included makes the chord do nothing.

`r+s+n+i` is the only Dosh chord that touches the upper pinky, which is what keeps it from ever
colliding with something Dosh types. `a+o+t+e` is on keys Dosh does use — `a+o`, `a+t` and `a+e`
are `l`, `q` and `d` — but all four together spell nothing in either table.

These exist for the boards that have only the 20 Taipo keys — the mesa2 — and so no spare key to
put the toggle on.

The choice belongs to the Taipo engine rather than to the mode. It survives switching out to
steno or qwerty and back, and it applies to the taipo layer shift in steno mode (see "Taipo from
steno mode" in TAIPO.md) as well. It is lost at power off, coming back up in Taipo.

On the boards with 4 LEDs, the third one shows which table is selected while in Taipo mode: the
Taipo mode color for Taipo, cyan for Dosh. It is dark in every other mode. The jolt3 has only two
LEDs, so it gives no indication.

## What is not mapped

- The media and consumer keys — play/pause, next and previous track, stop, volume up, volume
  down, mute, and brightness up and down. These need a consumer-control HID report, which is a
  separate piece of work.
- `ralt`+shift, which would be shift plus shift.
- The wiki's "layer 0-3" chord, which is a QMK concept that doesn't apply here.
- The five empty rows at the bottom of the wiki's table, which have nothing to map.
