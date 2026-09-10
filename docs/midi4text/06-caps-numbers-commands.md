# Midi4Text on mesa3 — capitals, numbers and commands

Follows `05-first-pass-mapping.md`. Keys keep their **Dosh** names throughout — `a` `o` `s`
`t` `n` `e` `i` `Sp` `Bk` per hand — and Taipo is not referred to at all.

Chords are drawn as grids rather than written as strings of key names. A chord is a shape
on the board, and writing it `aost` invites reading it as four letters, which it is not.

## What is actually spare

The syllabic mapping leaves very little inside each group:

| group | chords | used | spare |
|---|---|---|---|
| outer five | 31 | 30 | 1 |
| inner four, left (Series 2) | 15 | 13 | 2 |
| inner four, right (Series 3) | 15 | 13 | 2 |

But a *whole stroke* is 9 keys against 9, and real English needs only 6,982 of those
262,144 combinations. So commands should be built as **whole strokes that are not valid
syllables**, not carved out of the alphabets — which is what Midi4Text itself does, and why
its punctuation dictionary is nothing but ordinary strokes whose orthographic reading is
impossible.

## Capitals

### Sentence-ending punctuation should cap the next word

Yes — adopt it. Plover's `{.}` already does exactly this, and Midi4Text's punctuation
dictionary already emits `{.}`, so it costs nothing to keep.

**But it buys less than it seems.** Measured over the manual's own prose — the only cased
English to hand, and unrepresentative in absolute terms at 27.6% capitalised words —
sentence-initial capitals were only **13% of all capitals**; proper nouns were the other
87%. Ordinary prose has shorter sentences and fewer proper nouns, so the sentence-initial
share would be higher, but proper nouns stay the bulk of the problem. Auto-capping is
worth having and does not remove the need for an explicit marker.

### A prefix stroke, not an in-stroke marker

Michela puts capitalisation in the coda slot (`zcs`), which capitalises the syllable's
first letter but costs the coda — so a capitalised syllable cannot also end in a
consonant. That is a poor trade here.

**Use a prefix stroke instead.** Priced into the assignment, an in-stroke capital at 6% of
words costs 1.72 → 1.76 keys per consonant slot and eats the one spare outer chord. A
prefix stroke costs one extra stroke on a capitalised word — about 0.03 strokes per word
in ordinary prose — and leaves both alphabets untouched.

**Cap next word** — left inner four, nothing on the right:

      LEFT              RIGHT
      e  i Sp Bk        e  i Sp Bk
      #  #  #  #        .  .  .  .

This is the spare Series 2 chord `eiSpBk`, so it collides with no syllable.

## A command family, Phoenix-style

The same left-hand shape with a right-hand argument gives the retro-commands. Phoenix's
set is the right model: *cap next*, *don't cap next*, *cap previous N words*.

| command | left inner | right hand |
|---|---|---|
| cap next word | all four | — |
| don't cap next | all four | `e` |
| cap previous N words | all four | digit N on the outer five |
| join previous (kill space) | all four | `i` |

*Join previous* is much less useful here than in Phoenix, since Midi4Text controls spacing
through the vowel's ending form rather than emitting a space per stroke — it is worth
having only for compounds and repairs.

## Numbers

Michela's number bar, kept: a modifier held while the outer fives spell digits.

**Number bar** — left inner `i`+`Sp`+`Bk`, the other spare Series 2 chord.

With it held, each outer five spells a digit — tens on the left, units on the right,
counting outward from the pinky:

| digit | keys | | digit | keys |
|---|---|---|---|---|
| 1 | `a` | | 6 | `a`+`o` |
| 2 | `o` | | 7 | `o`+`s` |
| 3 | `s` | | 8 | `s`+`t` |
| 4 | `t` | | 9 | `t`+`n` |
| 5 | `n` | | 0 | `a`+`n` |

So 47 is the number bar with `t` on the left and `n`+`t` on the right. Phoenix's
right-hand 1–9 shapes are the same idea; using both hands gets two digits per stroke.

**This is the least designed part of the mapping.** The digit shapes above are merely
positional, multi-digit handling beyond two digits is unaddressed, and Michela's dedicated
chords for runs of zeros have no counterpart yet.

## The mapping as grids

```
OUTER FIVE - consonants, both hands (onset left / coda right)
              a   o   s   t   n   
  n          .   .   .   .   #
  s          .   .   #   .   .
  t          .   .   .   #   .
  r          #   .   .   .   .
  c          .   #   .   .   .
  d          #   .   .   #   .
  p          .   #   #   .   .
  f          .   #   .   #   .
  y          #   .   .   .   #
  th         .   #   .   .   #
  l          .   .   #   #   .
  b          #   #   #   .   .
  w          .   .   #   .   #
  g          #   #   .   .   .
  h/st       .   .   .   #   #
  v          #   #   .   #   .
  m          #   #   .   .   #
  ind/nd     #   .   #   #   .
  inc/ng     #   .   #   .   #
  k          #   .   .   #   #
  ch         .   #   #   #   .
  int/nt     .   #   #   .   #
  x          .   #   .   #   #
  sh         #   #   #   #   .
  gh         .   .   #   #   #
  z          #   .   #   .   .
  ck         #   #   #   .   #
  -/         #   #   .   #   #
  -/h        #   .   #   #   #
  -/e        .   #   #   #   #

INNER FOUR, right hand - the vowel
                 e   i  Sp  Bk   
  e             #   .   .   .
  e +space      #   .   .   #
  i             .   #   .   .
  i +space      .   #   .   #
  a             .   .   #   .
  a +space      .   .   #   #
  o             #   #   .   .
  o +space      #   #   .   #
  u             .   #   #   .
  u +space      .   #   #   #
  ea            #   .   #   .
  ea +space     #   .   #   #
  ou            #   #   #   .
  ou +space     #   #   #   #

INNER FOUR, left hand - the second character
         e   i  Sp  Bk   
  r     .   .   .   #
  l     .   .   #   .
  i     .   #   .   .
  t     #   .   #   .
  m     #   .   .   #
  o     #   #   .   .
  s     #   .   .   .
  e     .   #   .   #
  w     .   .   #   #
  c     #   #   #   .
  u     .   #   #   .
  p     #   #   .   #
  n     #   .   #   #
```

## Still open

- Punctuation beyond the sentence-enders has no assignment.
- The three rare coda-only shapes still sit on four- and five-key chords.
- Digit shapes and multi-digit numbers need the treatment the consonants got.
