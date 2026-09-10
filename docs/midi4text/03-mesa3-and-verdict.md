# Midi4Text — mesa3 feasibility, and whether it is worth learning

Phases 5 and 6. Both answers rest on measurements over the top 20,000 English words by
frequency (Norvig's `count_1w.txt`), using the minimum-stroke writer in
`midi4text-analysis/m4t/writer.py`.

The writer searches for the fewest strokes that spell a word, which is what the manual's
"divide the word however the layout allows" instruction amounts to. It reproduces the
manual's own worked examples exactly — *window* 2, *fantastic* 3, *syllabic* 3,
*personification* 6 — and writes **every one of the 20,000 words**, so the numbers below
describe the system as its authors intended it, with no brief dictionary involved.

## Phase 5 — does Midi4Text fit on mesa3?

mesa3 has 9 keys per hand (7 finger + 2 thumb, the Dosh geometry). Midi4Text wants 10.

**It fits, with room to spare, and the arithmetic is not close.**

The key finding is that Midi4Text's consonant encoding is extravagantly wasteful. It
spends 6 keys — 64 combinations — on groups that real English barely fills:

| Series | Keys | Patterns available | Used by the whole dictionary | Used by real English |
|---|---|---|---|---|
| 1 (onset) | `FSCZPN` | 64 | 29 | **27** |
| 2 (second) | `RXIU` | 16 | 15 | **14** |
| 3 (vowel) | `uiea` | 16 | 15 | **14** |
| 4 (coda) | `npzcsf` | 64 | 32 | **31** |

Series 1 uses 42% of its space and Series 4 exactly half. Drop one key from each
consonant group and a **5 + 4 split** gives 32 and 16 patterns respectively — enough for
all four, with five spare onsets and one spare coda:

    Series 1  27 needed <= 32 available
    Series 2  14 needed <= 16 available
    Series 3  14 needed <= 16 available
    Series 4  31 needed <= 32 available

The whole-hand check agrees and is much looser: real English uses 343 distinct left-hand
combinations and 335 right-hand ones, against the 512 a 9-key hand offers.

**The mirror survives.** This matters more than the raw count. Midi4Text's real design is
*one consonant alphabet and one vowel alphabet, each used twice* — that is what makes 86
chords cover the language. Under a 5+4 split the coda needs 31 patterns and the onset 27,
so a single 5-key consonant alphabet of 31 chords still serves both hands, with the onset
using a subset. The property that makes the system learnable is preserved exactly.

**What it costs.** The 4-key vowel groups must not be touched — they are 14 of 16 full,
and dropping to 3 keys leaves 8 patterns for 14 vowels, which fails outright. So the key
has to come off the consonant side, and that forces a **complete re-encoding of the
consonant chords**. Every mnemonic in the current table (`FC` = h, `SCP` = d, `FZP` = gh)
is built on 6 keys and would change. The Michela heritage — and with it the ability to
read the existing manual, dictionaries and tutorials — is gone. What you would have is a
Midi4Text-*derived* theory, not Midi4Text.

The other cost is headroom: one spare coda pattern. The three "extra-ordinem" chords
(`zcf` = final h, `zc` = final ck, `zcs` = capitalise) currently exist *because* Series 4
has 32 unused patterns to hide them in. On 5 keys they consume the entire margin.

**Verdict: technically feasible, and the fit is comfortable rather than marginal — but it
is a fork of the theory, not a port of it.** That only makes sense if the theory is worth
having in the first place, which is Phase 6.

## Phase 6 — is it worth learning?

Frequency-weighted over the top 20,000 words:

| | Midi4Text | Dosh |
|---|---|---|
| chords per word | **1.83** | 5.90 |
| keys per chord | 4.56 | 1.38 |
| **keys per word** | **8.36** | **8.13** |
| hands per chord | 2, always | 1, always |

Midi4Text needs **3.22× fewer chords — and almost exactly the same number of
keypresses.** 8.36 against 8.13.

That single line is the answer. The work does not disappear; it moves from sequential
into simultaneous. Midi4Text's advantage over a per-letter layout is entirely a bet that
pressing 4.56 keys at once is cheaper than pressing 1.38 keys four and a half times.

### Why that bet is worse here than on a steno machine

For court stenography the bet pays, because the alternative is QWERTY — one finger, one
letter, one key at a time, and no chording at all. Against Dosh the comparison is quite
different, and three things go against Midi4Text:

**No hand alternation.** Every Midi4Text stroke needs both hands: an onset on the left, a
vowel on the right. There is no such thing as a one-handed stroke. Dosh is the opposite —
every chord is one hand, and the hands alternate freely, so one hand releases while the
other is already forming its next chord. That overlap is most of where Dosh's speed comes
from, and Midi4Text cannot use it at all. Its 1.83 strokes per word are 1.83 *fully
serialised* events; Dosh's 5.90 are heavily pipelined.

This is the concern in the project brief, and the measurement confirms it: with the same
keypress count and no alternation, the 3.22× chord advantage has to overcome a pipelining
disadvantage that applies to every single stroke.

**Chord size.** Nearly a third of Midi4Text strokes press 6 or more keys simultaneously,
and 4.3% press 9 or more, across both hands:

| keys in stroke | 1–3 | 4–5 | 6–8 | 9–12 |
|---|---|---|---|---|
| share of strokes | 34% | 34% | 27% | 4.3% |

Dosh's chords are 1, 2 or 3 keys on one hand. Large simultaneous chords are slower to
form, harder to hit accurately, and are exactly what Posh was designed to avoid — the
layout Dosh derives from dropped the pinkies specifically "to make combos more accurate
and long periods of work more comfortable". Midi4Text goes hard in the other direction.

**Syllable division is a live cognitive task.** Dosh asks you to spell. Midi4Text asks you
to decide, in real time, where to break each word — and the manual is explicit that
English hyphenation rules do *not* apply, that a word has several legal divisions, and
that awkward clusters force unnatural ones (*attempts* → `at-tem-pts`, *rhythm* →
`rhy-thm`). That is a per-word decision at typing speed, and it has no analogue in Dosh.

### Where Midi4Text genuinely wins

**47.6% of word tokens are a single stroke.** Nearly half of running English comes out in
one hit, and 77.5% in two or fewer. For a full-hand syllabic system with no brief
dictionary, that is a real achievement, and it is why 1.83 strokes/word is achievable at
all.

The learning curve is also modest by steno standards: roughly 86 chords (27 onsets, 31
codas, 14 seconds, 14 vowels) plus the composition rules, with no word list to memorise.
The manual's claim of far-below-the-usual-1.5-years is credible.

### Verdict

**Not worth learning as a replacement for Dosh, and not worth building for mesa3.**

The decisive number is 8.36 versus 8.13 keys per word. Midi4Text does not reduce the
physical work; it repackages it into fewer, much larger, strictly two-handed chords, and
then gives up hand alternation to do so. Against QWERTY that is a large win. Against a
well-tuned one-hand-at-a-time chording layout that already achieves the same keypress
count *with* alternation and 1–3 key chords, there is no headroom left for it to win.

The honest summary is that Dosh has already collected the gain Midi4Text is offering, by
a route that keeps the alternation. Adopting Midi4Text would trade a pipelined 5.9-chord
word for a serialised 1.8-chord word at equal key cost — and the pipelining is worth more
than the chord count.

**What is worth keeping.** Two ideas from this theory are worth stealing independently of
the rest:

1. **Folding the inter-word space into the last stroke**, signalled by adding one key to
   the vowel. It costs one key instead of one chord, and it is why Midi4Text's chord
   count is as low as it is. Dosh spends a full thumb chord on every space — a sixth of
   its 5.90 chords per word. This is directly portable and is the single cheapest
   improvement visible in these numbers.
2. **The one-alphabet-used-twice mirror**, which is what keeps the chord inventory at 86
   rather than several hundred. Taipo and Dosh already do this — both hands are identical
   — so the lesson is confirmation rather than news, but it is worth noting that the two
   systems independently arrived at it.

## Reproducing these numbers

    cd midi4text-analysis
    python3 validate.py --show 3        # theory vs. the shipped dictionary
