# Posh cheat sheet

This describes the Posh implementation in `bbq-keyboard/src/layout/posh.rs`, which follows the
layout at <https://inkeys.wiki/en/keymaps/posh> with local differences: the thumbs are Taipo's
rather than the wiki's (see "Physical layout" below), both thumbs together is Taipo's "release
modifiers" null key rather than a sticky shift, the wiki's `ralt` chord is Shift, and the media,
volume and brightness keys are not mapped. Two more local swaps: `s` and `h` trade their
punctuation layers, and `o` and `e` trade their navigation layers. In both cases the letters
themselves stay where the wiki has them; only the thumb layers move.

Posh is a Taipo-style layout by the same community, described there as "a taipo style layout that
excludes the pinkies in order to make combos more accurate and long periods of work more
comfortable". It runs on the same engine as Taipo, with the same chord timing and the same
modifier behavior; only the chord table differs. See TAIPO.md for the layout it derives from.

## Physical layout

Each hand has 6 finger keys (3 fingers × 2 rows) and 2 thumb keys. The hands are identical
(mirrored); chords are formed within one hand, and you can alternate hands freely — even for
double letters. Keys are named here by the letter they type alone:

| finger:    | ring | middle | index |
|------------|------|--------|-------|
| top row    | a    | n      | i     |
| bottom row | o    | t      | e     |

**The pinky keys are dead.** Leaving them out is the point of the layout, so a chord that includes
either one matches nothing and types nothing.

Thumbs: **Sp** = the thumb that types Space alone, **Bk** = the thumb that types Backspace alone.
These are where Taipo has them; the wiki has the two swapped. The layers follow the thumb's
function rather than its position, again as in Taipo: **+Sp** gives capitals, **+Bk** gives the
digits, symbols and navigation keys, and **+Sp+Bk** gives the function keys and the remaining
symbols.

Both thumbs pressed alone releases any held modifiers without typing anything.

## Single keys

+Bk turns the six keys into the navigation cluster, and both thumbs into the far-motion keys.
Arrow mnemonic: up and down are the middle finger's two keys, and left and right are the bottom
row's outer two, with the index finger giving left and the ring finger right. (The wiki has those
two the other way around.) That leaves the ring and index top keys for Escape and Enter.

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| a | a | A | Escape | Delete (fwd) |
| n | n | N | ↑ | Page Up |
| i | i | I | Enter | Tab |
| o | o | O | → | End |
| t | t | T | ↓ | Page Down |
| e | e | E | ← | Home |

## Middle+index pairs

The two chords whose thumb layers carry the comma, the period and the quotes.

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| n+i | s | S | `.` | `"` |
| t+e | h | H | `,` | `'` |

## Letters whose +Bk is a digit

Both thumbs gives the matching function key.

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| a+n | d | D | 1 | F1 |
| i(top)+t(bot) | l | L | 2 | F2 |
| o+e | c | C | 3 | F3 |
| o+t | u | U | 4 | F4 |
| a(top)+e(bot) | m | M | 5 | F5 |
| i(top)+o(bot) | w | W | 6 | F6 |
| a+i | f | F | 7 | F7 |
| n(top)+o(bot) | g | G | 8 | F8 |
| n(top)+i(top)+o(bot) | y | Y | 9 | F9 |
| n(top)+e(bot) | r | R | 0 | F10 |

## Letters whose +Bk is a symbol

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| a+n+i | p | P | `+` | `=` |
| o+t+e | b | B | `-` | `_` |
| n(top)+o(bot)+e(bot) | v | V | `/` | `\` |
| a(top)+n(top)+e(bot) | k | K | `;` | `\|` |
| i(top)+o(bot)+t(bot) | j | J | `:` | `*` |
| a(top)+t(bot)+e(bot) | x | X | `$` | `#` |
| a(top)+o(bot)+e(bot) | q | Q | `@` | F11 |
| n(top)+t(bot)+e(bot) | z | Z | `&` | F12 |

## Punctuation-only chords

These have no both-thumbs variant.

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| a(top)+t(bot) | `?` | `!` | `^` |  |
| i(top)+o(bot)+e(bot) | `` ` `` | `~` | `%` |  |

## Modifiers and brackets

The three same-finger vertical pairs are modifiers, and their thumb layers are the brackets.
The fourth chord is the wiki's `ralt`, which is Shift here, since there is no right-alt modifier
to send; that leaves its both-thumbs variant unmapped, as it would be shift plus shift.

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| n(top)+t(bot) | GUI | `)` | `(` | GUI+Shift |
| i(top)+e(bot) | Ctrl | `]` | `[` | Ctrl+Shift |
| a(top)+o(bot) | Alt | `}` | `{` | Alt+Shift |
| a(top)+o(bot)+t(bot) | Shift | `>` | `<` |  |

## Extras

| chord | alone | +Sp | +Bk | +Sp+Bk |
|-------|-------|-----|-----|--------|
| n(top)+o(bot)+t(bot) | Print Screen |  |  |  |
| a(top)+i(top)+e(bot) | Insert |  |  |  |

The thumb layers of these two are the volume, mute and brightness keys, which need a
consumer-control HID report the firmware doesn't have yet.

## Modifiers: how they behave

Identical to Taipo — see the "Modifiers: how they behave" section of TAIPO.md. In short: a
modifier chord is sent immediately and stays held until the next key consumes it, modifiers
accumulate, pressing a modifier chord whose modifiers are all already held makes the whole held
set sticky, and both thumbs together releases everything.

## Switching between Taipo and Posh

While in Taipo mode, tapping the **lower-left key by itself** switches between the Taipo and Posh
chord tables. This is the steno `#` key of the outer left column, which has no meaning in Taipo;
on the 3-row jolt3 it is the middle key of that column rather than the lower one. Only a solo tap
counts — pressed with nothing else down and released with nothing else down — so the table can
never change in the middle of a chord.

Two chords do the same job, and are in both tables, so either can be reached from either:

| chord     | keys                 | selects |
|-----------|----------------------|---------|
| `r+s+n+i` | the whole top row    | Taipo   |
| `a+o+t+e` | the whole bottom row | Posh    |

The names are Taipo's; the chord is named by its shape, and in Posh its two pinky keys spell
nothing. They **select** rather than toggle — the one you are already in does nothing — and the
thumb variants are unmapped, so a thumb accidentally included makes the chord do nothing.

Using a pinky is what makes them safe to put in this table: Posh has no pinky chords by
definition, so neither can ever collide with something Posh types. They are the only entries in
the Posh table that touch a pinky key.

These exist for the boards that have only the 20 Taipo keys — the mesa2 — and so no spare key to
put the toggle on.

The choice belongs to the Taipo engine rather than to the mode. It survives switching out to
steno or qwerty and back, and it applies to the taipo layer shift in steno mode (see "Taipo from
steno mode" in TAIPO.md) as well. It is lost at power off, coming back up in Taipo.

On the boards with 4 LEDs, the third one shows which table is selected while in Taipo mode: the
Taipo mode color for Taipo, cyan for Posh. It is dark in every other mode. The jolt3 has only two
LEDs, so it gives no indication.

## What is not mapped

- The media and consumer keys — play/pause, next and previous track, stop, volume up, volume
  down, mute, and brightness up and down. These need a consumer-control HID report, which is a
  separate piece of work.
- `ralt`+shift, which would be shift plus shift.
- The wiki's "layer 0-3" chord, which is a QMK concept that doesn't apply here.
- The five empty rows at the bottom of the wiki's table, which have nothing to map.
