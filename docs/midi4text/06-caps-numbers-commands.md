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

**Cap next word** — left outer `a`+`s` with inner `e`, nothing on the right:

      LEFT                        RIGHT
      a  o  s  t  n  e  i Sp Bk   a  o  s  t  n  e  i Sp Bk
      #  .  #  .  .  #  .  .  .   .  .  .  .  .  .  .  .  .

A shape the syllabic system never produces. (An earlier draft put this on all four inner
keys; that shape is now the Dosh toggle, which needs to be free in both layouts.)

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

## One-hand strokes, and where an escape can live

**Both one-hand forms are valid and common**, so neither can be reserved wholesale:

| stroke | share | example |
|---|---|---|
| both hands | 78.4% | most syllables |
| right hand only | 13.2% | vowel-initial words — *a*, *and*, *of* |
| left hand only | 8.4% | trailing consonants, prefixes |

An escape therefore has to be one *specific* unused shape rather than a positional rule.
There is no shortage: a hand has 511 non-empty shapes, and real English uses only 184 of
them as left-only strokes and 182 as right-only, leaving **327 and 329 free**.

## Numbers, punctuation and symbols: escape to Dosh

Rather than design any of this, switch to Dosh and use what is already there — digits,
symbols, navigation and function keys, all on thumb layers that are already learned. This
removes the whole problem *and* its learning cost.

### One-shot: left hand marks, right hand is Dosh

The escape does not need to be a stroke of its own. **Hold the escape shape on the left
while the right hand plays a literal Dosh chord**, and the whole thing is a single stroke:

      LEFT                        RIGHT
      a  o  s  t  n  e  i Sp Bk   a  o  s  t  n  e  i Sp Bk
      .  .  .  .  .  .  #  #  #   <------ any Dosh chord ------>

That is the important consequence: **a punctuation mark costs one stroke — exactly what a
purpose-designed native stroke would cost.** So there is no reason to design native
punctuation at all. Every symbol Dosh can reach becomes available at the same price, with
nothing new to learn.

The right hand has all nine keys, so every Dosh chord is reachable, thumb layers included,
and the hands are identical so nothing is lost by the Dosh chord always being right-handed.

The shape is the left inner `i`+`Sp`+`Bk`, three keys on index and thumbs. It leaves the
left outer five — the strong fingers — completely free, and the syllabic system never
produces it: of the 511 left-hand shapes, English uses 343, and this is among the 169 that
never occur in any stroke.

### Toggle: for runs

For numbers, code, or symbol-heavy text, switch wholesale. In Dosh both hands are
independent layouts with rollover, so a run goes at full Dosh speed rather than one
right-handed chord at a time.

**Toggle: all four inner keys of one hand**, `e`+`i`+`Sp`+`Bk`. This shape is free in
*both* layouts — it is a spare Series 2 chord in Midi4Text, and Dosh leaves `0x388`
unmapped — so the same chord enters and leaves, with no special-casing needed beyond
intercepting it the way `dosh_event` already intercepts `DOSH_TOGGLE_KEY`.

**The firmware already has all of this.** `LayoutManager::dosh_event` toggles
`TaipoVariant` on a lone press-and-release and raises `MinorMode::Dosh` so the mode is
visible; the mechanism, the indicator and the tests exist. The only change is that on
mesa3 all 18 keys are spoken for, so the trigger is a chord rather than a lone key.

**Numbers should use the toggle, not a number bar.** Michela's number bar plus ten digit
shapes is ten things to learn for something used rarely and almost always in runs, where a
toggle amortises to nothing and Dosh's digits are already known. This supersedes the
number-bar sketch this document previously carried.

### Revised chord assignment

Frequency decides which gets the cheapest shape, and the one-shot escape is the most used
of the three — punctuation is commoner than capitals:

| | shape | keys | roughly |
|---|---|---|---|
| one-shot Dosh escape | left inner `i`+`Sp`+`Bk` | 3 | 0.15 / word |
| capitalise | left outer `a`+`s`, inner `e` | 3 | 0.03 / word |
| Dosh toggle | all four inner | 4 | rare |

All three are among the 169 left-hand shapes the syllabic system never produces.

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
