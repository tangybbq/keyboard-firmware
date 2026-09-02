# Taipo cheat sheet

This describes the Taipo implementation in `bbq-keyboard/src/layout/taipo.rs`, which follows the
layout at <https://inkeys.wiki/en/keymaps/taipo> with local differences: function keys use both
thumbs (instead of a shift-like prefix), and modifiers are sent to the host immediately (see
"Modifiers" below).

## Physical layout

Each hand has 8 finger keys (4 fingers × 2 rows) and 2 thumb keys. The hands are identical
(mirrored); chords are formed within one hand, and you can alternate hands freely — even for
double letters. Keys are named here by the letter they type alone:

| finger:   | pinky | ring | middle | index |
|-----------|-------|------|--------|-------|
| top row   | r     | s    | n      | i     |
| bottom row| a     | o    | t      | e     |

Thumbs: **Sp** = the thumb that types Space alone, **Bk** = the thumb that types Backspace alone.

Every chord has up to four variants: alone, with Sp added, with Bk added, or with both thumbs
added.

## Single keys

Sp adds shift (capital); Bk gives brackets. Bottom row opens, top row closes, matching by finger:
a/r = `<` `>`, o/s = `{` `}`, t/n = `[` `]`, e/i = `(` `)`.

| chord | alone | +Sp | +Bk |
|-------|-------|-----|-----|
| a     | a     | A   | `<` |
| o     | o     | O   | `{` |
| t     | t     | T   | `[` |
| e     | e     | E   | `(` |
| r     | r     | R   | `>` |
| s     | s     | S   | `}` |
| n     | n     | N   | `]` |
| i     | i     | I   | `)` |

## Letter combos

+Sp capitalizes any of these. +Bk gives the digit or symbol shown.

Same-row pairs (digits: bottom-row combos give 1 2 3 4 0, top-row give 5 6 7 8 9):

| chord   | alone | +Bk |
|---------|-------|-----|
| o+e     | c     | 1   |
| o+t     | u     | 2   |
| a+t     | q     | 3   |
| a+o     | l     | 4   |
| n+i     | y     | 5   |
| s+i     | f     | 6   |
| s+n     | p     | 7   |
| r+n     | z     | 8   |
| r+s     | b     | 9   |
| t+e     | h     | 0   |
| a+e     | d     | `@` |
| r+i     | g     | `#` |

Cross-row pairs:

| chord         | alone | +Bk |
|---------------|-------|-----|
| n(top)+a(bot) | j     | `=` |
| i(top)+o(bot) | k     | `+` |
| i(top)+a(bot) | w     | `&` |
| r(top)+e(bot) | m     | `$` |
| r(top)+t(bot) | x     | `^` |
| s(top)+e(bot) | v     | `*` |

## Punctuation combos

| chord         | alone | +Sp | +Bk |
|---------------|-------|-----|-----|
| s(top)+t(bot) | `/`   | `\` | `\|` |
| n(top)+o(bot) | `-`   | `_` | `%` |
| r(top)+o(bot) | `;`   | `:` |     |
| i(top)+t(bot) | `?`   | `!` |     |
| n(top)+e(bot) | `,`   | `.` | `~` |
| s(top)+a(bot) | `'`   | `"` | `` ` `` |

## Vertical pairs: modifiers and cursor keys

Pressing a finger's two keys together (same finger, both rows) is a modifier; +Sp turns the same
pair into an arrow, +Bk into paging/home/end:

| chord  | alone       | +Sp | +Bk       |
|--------|-------------|-----|-----------|
| r+a    | GUI (Cmd)   | →   | Page Up   |
| s+o    | Alt         | ↑   | Home      |
| n+t    | Ctrl        | ↓   | End       |
| i+e    | Shift       | ←   | Page Down |

Arrow mnemonic: left/right on the outer (pinky/index) pairs, up/down on the inner (middle/ring)
pairs.

## Three-finger chords and thumbs

| chord         | alone     | +Sp            | +Bk    |
|---------------|-----------|----------------|--------|
| s+n+i (top)   | Tab       | Delete (fwd)   | Insert |
| o+t+e (bottom)| Enter     | Escape         |        |
| Sp alone      | Space     |                |        |
| Bk alone      | Backspace |                |        |
| Sp+Bk together| release held modifiers (types nothing) | | |

## Multi-character chords

A chord can type a short sequence of characters rather than a single key. Thirteen do: the
n-grams that save the most typing, on the thirteen easiest chords that leave the pinky out.
Ordered easiest first, which is also most-valuable first.

| chord   | alone  | +Sp    |
|---------|--------|--------|
| e+i+t   | the    | The    |
| e+i+n   | in     | In     |
| e+n+t   | er     | Er     |
| i+n+t   | an     | An     |
| n+o+t   | tion   | Tion   |
| n+s+t   | re     | Re     |
| n+o+s   | or     | Or     |
| o+s+t   | es     | Es     |
| e+i+s   | en     | En     |
| e+i+o   | on     | On     |
| e+o+s   | al     | Al     |
| i+o+s   | at     | At     |
| e+n+s   | st     | St     |

Every one of these uses three keys across two or three fingers, none of them the pinky. The first
four use only the index and middle fingers, which is what makes them the easiest chords on the
board: two adjacent strong fingers, and no row disagreement between them, because one finger
presses both of its keys while the other presses one.

`th` is deliberately absent. It is the most common bigram in raw counts, but `the` has a chord, and
`the` takes most of what a `th` chord would have saved. The ranking behind these thirteen accounts
for that; see `docs/ngrams-results.md`.

Each character is sent as its own report, so a sequence types out as if you had chorded the letters
yourself. Three things follow from that:

- Held modifiers apply per character, following the usual rules (see "Modifiers" below). A one-shot
  shift lands on the first character only, so a one-shot shift with `eit` gives `The`, the same as
  `Sp+eit`. Sticky modifiers apply to all of them: sticky Ctrl plus `ent` is Ctrl-E then Ctrl-R.
- Only the **last** character is left held, so holding a chord types the sequence and then
  auto-repeats its final character — holding `eit` gives `the` and then `eeee`.
- The longer sequences take longer to send. `tion` is four reports, on a queue that drains one per
  millisecond.

`+Bk` and both-thumb variants are unassigned for all thirteen; they are held for punctuation and
programming sequences, which need their own analysis.

## Function keys (both thumbs)

Both thumbs + a digit chord gives the matching function key; v and w extend past F10:

| chord (with Sp+Bk)   | key |
|----------------------|-----|
| 1–9 chords (c u q l y f p z b) | F1–F9 |
| 0 chord (h = t+e)    | F10 |
| v chord (s+e)        | F11 |
| w chord (i+a)        | F12 |

## Modifiers: how they behave

- A modifier chord is sent to the host **immediately** on press, and stays held after you release
  the chord.
- Modifiers accumulate: press GUI then Shift, and both are down.
- The next normal key is sent with the held modifiers; when that chord is released, the key
  **and all modifiers** are released.
- Pressed a modifier by mistake (or want to press-and-release one bare, e.g. tap Cmd)? Both
  thumbs together releases all held modifiers without typing anything.
- To hold a modifier across several keys (e.g. Cmd held while tapping Tab repeatedly to cycle
  windows), **double press** it: a modifier chord whose modifiers are all already held — the same
  chord again, or its counterpart on the other hand — makes the whole held set sticky. Sticky
  modifiers survive any number of keypresses; both thumbs together is what releases them.

## The Posh variant

The same engine can interpret chords with the Posh table instead, which leaves the pinkies out.
Tap the lower-left key by itself while in taipo mode to switch between them; see POSH.md.

There are also two chords, which work in either table:

| chord     | keys                    | selects |
|-----------|-------------------------|---------|
| `r+s+n+i` | the whole top row       | Taipo   |
| `a+o+t+e` | the whole bottom row    | Posh    |

They **select** rather than toggle: the one you are already in does nothing, so a chord that was
not felt cannot leave you in the wrong table. Either hand works. The chord types nothing, and the
thumb variants (`+Sp`, `+Bk`, both) are deliberately unmapped, so a chord with a thumb
accidentally included does nothing at all.

These exist for the boards that have only the 20 Taipo keys and no spare key to put the toggle
on. On a board that has the key, both work.

## Taipo from steno mode

In steno mode, holding a taipo-shift key (proto3 scancodes 20/44 — the L-S1/R-S3 positions)
lets taipo chords type through as a layer shift, without leaving steno mode.
