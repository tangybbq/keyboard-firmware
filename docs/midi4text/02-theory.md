# Midi4Text — the theory, recovered

Phases 2–4 of the analysis. This is the implementable description of Midi4Text: what
each key group spells, how the pieces compose, and how far that gets you.

**Result: ~110 table rows and a dozen composition rules reproduce 155,857 of the
158,614 shipped dictionary entries exactly — 98.26%.** The dictionary is not a word
list. It is an enumeration of the function below, and can be replaced by it.

Reference implementation: `midi4text-analysis/m4t/theory.py`. Harness:
`midi4text-analysis/validate.py`.

## Method

The tables were **derived from the dictionary, not copied from the manual**. For each
Series, hold the other three components fixed, compare the entry that uses a pattern
against the entry that leaves that Series empty, and take the majority difference over
every available context (`m4t/extract.py`). Each Series-1 and Series-4 pattern was
decided by ~5,000 independent contexts.

The derived tables agree with the manual's, and the disagreements are informative
rather than noisy: every Series-1 pattern whose vote fell below ~99% is one that
participates in a documented inter-series cluster. `FC` scored 4035/4807 precisely
because `FC`+`R` is *str*, `FC`+`RI` is *spl*, `FC`+`IU` is *spr* and `FC`+`XIU` is
*scr*. `CP` dipped because `CP`+`XIU` is *qu*; `ZN` because `ZN`+`I` is *j*. The
differencing rediscovered the inter-series table without being told it existed.

## The stroke

Twenty keys in four Series, canonical order `FSCZPN RXIU | uiea npzcsf`:

    Series 1        Series 2       Series 3      Series 4
    F S C Z P N     R X I U        u i e a       n p z c s f
    onset           second /       vowel         coda
                    mirrored vowel

A stroke is one syllable. The four Series map onto Michela's four "phonic elements",
reinterpreted as spelling rather than sound.

## Tables

**Series 1 — onset** (empty = no onset)

| | | | | | |
|---|---|---|---|---|---|
| `F` f | `S` s | `C` sh | `Z` z | `P` p | `N` n |
| `FC` h | `SC` v | `FZ` th | `SZ` k | `CZ` ck | |
| `FP` t | `SP` ch | `CP` c | `ZP` g | | |
| `FN` ind | `SN` inc | `CN` w | `ZN` y | | |
| `FCP` b | `SCP` d | `FZP` gh | `SZP` m | | |
| `FCN` r | `SCN` l | `FZN` int | `SZN` x | | |

**Series 4 — coda.** Exactly the mirror of Series 1 under `F↔f S↔s C↔c Z↔z P↔p N↔n`,
with the same spellings, except: `cf` = *st* (not *h*), `nf` = *nd*, `ns` = *ng*, `nzf`
= *nt*. Three patterns have no Series-1 counterpart and are struck "extra-ordinem",
off the normal finger assignment: `zcf` = final *h*, `zc` = final *ck*, `zcs` =
capitalise.

**Series 2 — second character**

| | | | | | |
|---|---|---|---|---|---|
| `R` r | `X` s | `I` i | `U` u | | |
| `RI` l | `XI` w/h | `RU` m | `XU` n | `IU` p | |
| `RIU` t | `XIU` c | `RX` e | `RXI` o | | |

`XI` spells *h* after `p`, `w`, `r` and *w* otherwise. (The manual also lists `g`; the
dictionary does not honour it — `ZPXIuan` is *gwan*.)

**Series 3 — vowel**

| Plain | | Ending form | |
|---|---|---|---|
| `a` a | | `ua` a | |
| `e` e | | `ue` e | |
| `i` i | | `ui` i | |
| `u` u | | `uia` u | |
| `ie` o | | `uie` o | |

Plus three "free" combinations: `ea`, `ia`, `iea` (see below).

## Composition

    word = onset + second + nucleus + coda [+ silent e]

with these rules, in order:

1. **Inter-series onset clusters.** `FC`+`R` = str, `FC`+`RI` = spl, `FC`+`IU` = spr,
   `FC`+`XIU` = scr, `C`+`XIU` = sch, `Z`+`XIU` = sk, `S`+`X` = sci, `ZN`+`I` = j,
   `CP`+`XIU` = qu. These replace the onset *and* consume Series 2.

2. **Vowel digraphs across the hands.** `U`+`u` = au, `I`+`i` = ai (and the same with
   the ending forms `uia`, `ui`).

3. **Free combinations.** Struck with Series 3 alone, `ea`, `ia` and `iea` emit the
   placeholder glyphs `°`, `*`, `_` and close the word — they are deliberately left
   free for the user. Struck alongside Series 2, they become the digraph Series 2
   cannot otherwise reach: `ea` = *ea* (non-final), `iea` = *ea* (final), `ia` = *ou*
   (final).

4. **The mirrored-vowel technique.** If Series 3 is empty and Series 2 holds a vowel
   pattern (`R`=a, `X`=e, `I`=i, `XI`=o, `U`=u, `RX`=ea), Series 2 becomes the nucleus,
   a silent **e** is appended, and the word closes. This is what buys the one-stroke
   CVCe words: *tune* = `FPUn`, *node* = `NXIpcs`, *hate* = `FCRpf`. It does not fire
   when the coda is one of the extra-ordinem patterns, and it suppresses rule 1 —
   `FCRn` is *hane*, not *strane*, because `R` is the vowel here, not the *r* of *str*.
   `FN` additionally spells *gn* rather than *ind* in this context.

5. **`RX` and `RXI` are vowels always** (*e*, *o*), never consonant material.

6. **Bare final Y.** `ui` + `nz` spells just *y*; the `ui` keys are there only to carry
   the word-final space.

## How the output binds — the rule the manual never states

This was the largest gap between the prose and the data, and it turns out to be
mechanical. Midi4Text folds the inter-word space into the last stroke of a word, so
every entry is a Plover affix, and **the shape of Series 3 alone decides which**:

| Series 3 | Output | Meaning |
|---|---|---|
| ending form (`ua ue ui uia uie ia iea`) | `word` | final syllable — space follows |
| plain vowel (`a e i u ie`) | `{word^}` | non-final syllable — binds forward |
| empty | `{^word}` | no nucleus — binds backward |

Exceptions: a Series-1-only stroke is a prefix (`F` = `{f^}`), a vowel-less fragment
closed by final Y binds forward (`FCPRInz` = `{bly^}`), and the mirrored-vowel
technique of rule 4 closes the word outright.

**This rule holds for all 68,235 ending-form entries without a single exception**, and
it is the single most useful fact for anyone implementing the system: the writer
signals "end of word" by adding the `u` key to the vowel, and everything else follows.

## Why the chord space is shaped as it is

Counting the patterns each Series actually uses:

| Series | Keys | Patterns used | of possible |
|---|---|---|---|
| 1 | `FSCZPN` | 29 | 64 |
| 2 | `RXIU` | 15 | 16 |
| 3 | `uiea` | 15 | 16 |
| 4 | `npzcsf` | 32 | 64 |

Series 2 and 3 are near-saturated and are **exact mirrors** of one another under
`R↔a X↔e I↔i U↔u` — each omits precisely the one pattern that maps to the other's
omission (`RXU` ↔ `uea`). Series 1 and 4 mirror likewise, Series 4 carrying exactly
three extra patterns, all three of them the extra-ordinem exceptions.

So the theory has **one 6-key consonant alphabet and one 4-key vowel alphabet, each
used twice** — once per hand. That is the whole design, and it is why the system is
learnable: you memorise 29 consonant chords and 15 vowel chords, not 158,614 entries.

The full four-way cross product is only 41% populated, but per hand it is 98.9%
(380 legal left-hand combinations × 422 right-hand). **The constraint lives inside each
hand, not between them** — the hands compose almost freely, which is what makes the
dictionary a near-perfect rectangle.

## The finger-exclusion constraint

The manual states that each finger owns two keys "which have not to be pressed
simultaneously (with the exception of the thumbs)", but gives the finger assignment only
as a diagram. Pairing each finger with two adjacent keys, thumb innermost, gives
`FS` pinky, `CZ` ring, `PN` middle, `RX` index, `IU` thumb — and the dictionary bears
that out almost perfectly:

| finger | pair | strokes using both keys |
|---|---|---|
| pinky | `FS` | 0 |
| middle | `PN` | 0 |
| ring | `CZ` | 1 |
| index | `RX` | 22,609 (14.3%) |
| thumb | `IU` | 34,099 (21.5%) |

Every violation is one the manual itself flags. The single ring-finger case is `CZ`, the
extra-ordinem *ck*. The index cases are `RX` and `RXI`, which the manual describes as
being played "by rotating the thumb under the index finger, similarly to the piano
thumb passage". The thumb is exempt by design.

Under strict exclusion a Michela hand offers 3x3x3x3x4 = 324 combinations; Midi4Text uses
379 on the left and 421 on the right, and the excess is exactly what the thumb passage
and the extra-ordinem chords buy.

**This is a constraint of the instrument, not of the theory.** Piano keys are wide, so one
finger genuinely cannot take two at once and the rule had to be designed around. On a
keyboard the two keys of a finger are small and adjacent, and rolling or flattening onto
both is easy -- Dosh already does it in 25 of its 132 entries. So a keyboard port is *not*
bound by finger exclusion, and the extra-ordinem chords and the thumb passage, which exist
only to work around it, need no counterpart.

## The overlay dictionaries use no special mechanism

The punctuation, word-parts and briefs dictionaries are not a separate encoding. Of their
277 entries, **268 are ordinary strokes that the main dictionary also defines** -- the
remaining 9 are simply strokes the enumeration never generated. They win because Plover
consults them first.

What makes that safe is that their main-dictionary readings are orthographic nonsense:
`CPpc` reads `{^cc}`, `CPXIen` reads `{cwen^}`, `CPRUanzs` reads `{cmax^}`, `CNRuiezs`
reads `wrok`, `CNieacs` reads `w_v`. None can occur in English, so nothing is lost by
shadowing them -- exactly the rule the manual states, that a brief "should not use common
syllables, or sequences of them, which can be present inside words".

The rule is followed, with a handful of lapses. Ten overlay entries shadow a stroke whose
main reading is a real English word, and after discarding frequency-list noise the genuine
casualties are five: **rent** (`Ruenzf`, taken by *aren't*), **tar** (`RIUuancf`, by
`{^ard}`), **teas** (`RIUieas`, by *it's*), **scion** (`SXuien`, by *session*) and
**teal** (`RIUieancs`, by *it'll*). All five remain writable under a different syllable
division, so the cost is a memorised exception rather than a hole.

## Residue: the 1.74% not reproduced

2,757 entries. None of it threatens the theory; most of it looks like inconsistency in
whatever script generated the dictionary.

| Count | Class |
|---|---|
| 1,896 | unclassified one-offs — doubled vowels (`FCPRXpcs` = *beadee*, not *beade*), dropped letters (`FCPIuinz` = *biy*, not *by*) |
| 315 | capitalisation applied to a different letter than rule 4 predicts |
| 240 | `ia` + coda: the dictionary alternates between *ou* and *ea* depending on which Series 2 pattern precedes, with no rule behind the choice |
| 181 | `cf`/`zc` after a mirrored vowel, spelt as their Series-1 mirror (*h*, *ck*) rather than as codas (*st*, *ck*) |
| 125 | the number bar — `IU` plus a coda is the separate numeric subsystem, not a syllable at all |

The number-bar entries are a genuine subsystem rather than an error; they are simply
out of scope for a syllable model. The `ia` alternation is the only place where the
shipped dictionary is internally inconsistent in a way a learner would notice.

## What this means for the remaining questions

Two consequences carry into Phases 5 and 6.

**The system is small.** A complete implementation is a few hundred lines and no
dictionary. That makes a firmware implementation entirely practical — far cheaper than
the steno dictionary machinery already in `bbq-steno`.

**The system is rigidly two-handed.** Every stroke needs an onset from the left and a
vowel from the right; the mirror symmetry is structural, not decorative. There is no
Midi4Text stroke that uses one hand. That is the fact Phase 6 has to weigh against
Taipo/Dosh, and the fact Phase 5 has to work around: with 9 keys per side there is no
way to shave a key without breaking the symmetry that makes the alphabets shareable.
