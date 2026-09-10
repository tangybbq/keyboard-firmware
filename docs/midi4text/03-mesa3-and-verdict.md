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

**Superseded — see "Speed potential, with alternation credited" below.** The conclusion
first recorded here read the comparison off keys per word (8.36 vs 8.13) and concluded
there was no headroom. That is the wrong yardstick: keys per word says how much finger
motion a system costs, not how fast it can go. The right yardstick is *hand cycles* —
how many times a hand must form, strike and release a chord — because that is what
serialises.

## Speed potential, with alternation credited

Chords per word is the numerator for speed, but Dosh's must be divided by its
alternation. Both hands are complete layouts and either can type any letter, including
doubled ones, so **perfect alternation is always available** — unlike QWERTY, where it
depends on the text. Dosh's per-hand chord rate is therefore half its chord rate.
Midi4Text gets a small credit too: 21.6% of its strokes touch only one hand.

Per word, frequency-weighted:

| | Midi4Text | Dosh | Dosh, space folded |
|---|---|---|---|
| chords/strokes | 1.83 | 5.90 | 4.90 |
| **cycles on the busiest hand** | **1.68** | **2.95** | **2.45** |
| keys per hand | 4.15 / 4.21 | 4.06 | 4.06 |
| keys per cycle | 2.5 | 1.38 | 1.66 |
| Midi4Text advantage | — | **1.76×** | **1.46×** |

The striking line is *keys per hand*: **4.2 against 4.1.** Both systems ask each hand to
move essentially the same number of keys per word. The entire difference is how those
keys are packed — Midi4Text into 1.68 large simultaneous chords, Dosh into 2.95 small
ones.

So the ceiling is **1.46–1.76×**, not the 3.22× the raw chord count suggests, and the
lower figure applies if Dosh adopts the space-folding of `04-design-questions.md`.

**Whether even that is achievable turns on one unmeasured question: does a 2.5-key chord
take the same time to form as a 1.38-key one?** If chord time is flat in key count,
Midi4Text is genuinely ~1.5× faster. If it grows — and every chording layout's design
lore, Posh's pinky removal included, assumes it does — the advantage erodes toward
parity. That is the crux, and it is an empirical question about hands that these
measurements cannot settle.

### Correction to the cycle count

The 1.68 cycles/word first recorded above was miscounted: it took the maximum of the two
hands' *averages*, which lets a one-handed stroke overlap a two-handed neighbour when it
cannot. Averaging the per-word maximum gives **1.83** — effectively the stroke count,
since 78% of strokes need both hands. The unbriefed advantage over Dosh is therefore
**1.34×** with Dosh's space folded, or 1.61× without.

## Briefs: the argument that actually decides it

Everything above measures the systems *brief-free*, and that undersells Midi4Text badly.
The chord space of an 18-key Michela-derived layout is enormous, and briefs are learned
gradually, each paying off in proportion to its frequency.

**Brief capacity:**

| | free chords | |
|---|---|---|
| Dosh | **376** | one-handed, 4 thumb layers x 127, less the 132 the layout uses |
| mesa3 Midi4Text | **255,162** | of 2^18 strokes, real English needs only 6,982 (2.7%) |

That is a **678× difference**, and it is structural: Dosh chords are one-handed by design,
so its brief space is a few hundred slots, while a Midi4Text stroke is inherently
two-handed and spends a quarter of a million.

**What that buys** (cycles per word on the busiest hand; `midi4text-analysis/briefs.py`):

| briefs | Midi4Text | Dosh |
|---|---|---|
| 0 | 1.83 | 2.45 |
| 100 | 1.71 | 2.08 |
| 376 | 1.58 | **1.78 — exhausted** |
| 1,000 | 1.43 | 1.78 |
| 5,000 | 1.15 | 1.78 |
| 20,000 | 1.00 | 1.78 |

Dosh hits a wall at 376 briefs and stops at 1.78. Midi4Text keeps descending. At a brief
set a steno writer would accumulate over a few years — 2,000 to 5,000 — it reaches
1.15–1.26 against Dosh's floor of 1.78, a **1.4–1.5× advantage that the unbriefed
comparison does not show at all**.

**The counter, and why it does not fully answer.** Dosh could allow two-handed briefs with
an engine change, and its brief space would become vast too. But a two-handed Dosh brief
costs a full cycle, forfeiting the half-cycle advantage its one-handed chords enjoy — so
the two systems would converge on briefed words. What would *not* converge is the
**unbriefed fallback**: every word you have not briefed still costs Midi4Text 1.83 cycles
and Dosh 2.45. Midi4Text's syllabic layer is itself a brief system covering the entire
tail of the language, which is precisely what a per-letter layout cannot have.

### Revised recommendation

I have moved on this twice, so plainly: **the analysis no longer supports a confident
recommendation against.**

The case for is now: a 1.34× floor unbriefed, 678× the brief headroom, a graceful fallback
for every word never briefed, briefs learnable incrementally with immediate per-brief
payoff, and a 9-key re-encoding that is cheaper per keypress than Dosh (7.52 vs 8.13 keys
per word).

The case against is unchanged and still real: syllable division is an open-ended skill
with no analogue in Dosh, the chords are half again as large (2.25 vs 1.66 keys per
cycle), and it is a firmware project rather than a table swap.

What it is *not* is a system with no headroom, which is what the first pass concluded from
keys per word. Whether the return justifies the learning is a judgement about your time
that these numbers inform but cannot make.

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
