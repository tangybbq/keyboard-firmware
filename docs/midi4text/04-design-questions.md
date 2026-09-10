# Midi4Text — follow-up design questions

Four questions arising from the Phase 5/6 verdict, answered with measurements.

## 1. Did the Dosh figure include the space?

Yes. Dosh's 5.90 chords per word is 4.90 letters plus one space chord — one chord per
letter, plus the thumb. That is the honest comparison, since Midi4Text folds its space
into the last stroke and pays nothing extra for it.

## 2. Folding the space into Dosh without four-key chords

**Dosh's chord space is numerically empty and ergonomically full.** It uses 132 of the
508 available finger-chord slots — 26% — but the cheap shapes are gone:

| thumb layer | used | free |
|---|---|---|
| alone | 37 | 90 |
| +Sp | 33 | 94 |
| +Bk | 33 | 94 |
| +both | 29 | 98 |

In the *alone* layer only **2 free two-key chords and 26 free three-key chords** remain;
everything else free is four keys or more. So adding 26 new "letter+space" chords is not
affordable as new chord shapes.

**The way in is to repurpose rather than add.** `Bk` alone already types Space. Make
`letter + Bk` mean *that letter, then a space*. This:

- adds **no new chord shapes** — it reuses the 26 slots capitals currently hold;
- adds **no keys** — one thumb key, exactly the one the separate space chord would have
  cost;
- is mnemonically exact: the space thumb adds a space, alone or in company.

Capitals have to move. The cheapest home is a one-shot shift chord: at roughly 3% of
letters that costs about 0.15 chords per word against a saving of 1.00. Net **5.90 →
~5.05 chords per word**, with keys per word unchanged at 8.13. The same trick applies to
sentence-final punctuation, which is nearly always followed by a space.

**One caveat, and it matters.** Dosh's space is already the cheapest possible chord: a
one-key thumb tap, and because the hands alternate it can be made by the hand that is
*not* mid-word. In wall-clock terms it may already overlap almost completely with the
next letter. So 5.90 overstates what space actually costs, and folding it will improve
the chord count more than it improves real typing speed. It is still worth doing — but it
is not a free 17%.

## 3. What Midi4Text needs to run on 9 keys per side

The earlier "512 chords per hand, fits comfortably" was too glib. The binding constraint
is not the key count but **finger exclusion**, and once that is accounted for the real
structure of a Michela hand appears:

| group | fingers | keys | states | Midi4Text needs |
|---|---|---|---|---|
| Series 1 / 4 | pinky, ring, middle | 2 each | 3×3×3 = **27** | 30 shapes |
| Series 2 / 3 | index, thumb | 2 each | 4×4 = **16** | 13 shapes |

Series 1 never uses more than three keys at once, one per finger — `FSCZPN` appears only
in the licence chord. That is the whole design: three fingers spell the consonant, index
and thumb spell the vowel.

**mesa3's only structural difference is that its pinky has one key where Michela's has
two**, so the consonant group is 5 keys rather than 6. That is not a problem: 5 keys give
31 usable chords and the shared consonant alphabet needs 30. Finger exclusion does not
apply on a keyboard (see `02-theory.md`), so every one of those chords is available at its
face cost.

### Proposed mapping, in Dosh key names

Keys per hand: `a` (pinky), `o`/`s` (ring), `t`/`n` (middle), `e`/`i` (index), and the two
thumbs, written `S` and `B`. Hands separated by `-`, steno-style, since both hands now
share one set of names. Shapes are assigned smallest-chord-first by corpus frequency, so
the commonest consonants get single keys.

**Consonant group** — `a`, `o`/`s`, `t`/`n`. One alphabet, both hands: onset on the left,
coda on the right. 30 shapes into 31 chords, one spare.

| spells | chord | | spells | chord | | spells | chord |
|---|---|---|---|---|---|---|---|
| n | `t` | | th | `sn` | | ind / nd | `ost` |
| s | `n` | | l | `os` | | inc / ng | `osn` |
| t | `o` | | b | `at` | | k | `atn` |
| r | `s` | | w | `an` | | ch | `aot` |
| c | `a` | | g | `ao` | | int / nt | `aon` |
| d | `tn` | | h / st | `as` | | x | `ast` |
| p | `ot` | | v | `otn` | | sh | `asn` |
| f | `on` | | m | `stn` | | gh | `aos` |
| y | `st` | | | | | z | `ostn` |
| | | | | | | ck | `aotn` |

**Vowel group** — `e`/`i` (index), `S`/`B` (thumbs). Series 2 left, Series 3 right. 13
shapes into 15 chords, two spare.

| Series 2 | Series 3 | chord | | Series 2 | Series 3 | chord |
|---|---|---|---|---|---|---|
| r | a | `S` | | c | o | `iB` |
| s | e | `B` | | t | u | `ei` |
| i | i | `e` | | u | u | `eSB` |
| n | e | `i` | | o | _ | `iSB` |
| m | a | `SB` | | e | ° | `eiS` |
| w | o | `eS` | | | | |
| p | i | `eB` | | | | |
| l | * | `iS` | | | | |

### The re-encoding is better than Michela's

Michela's chord assignment is historic and mnemonic, not optimised; assigning by frequency
instead makes the 9-key version *cheaper than the 10-key original* on every measure:

| | Michela encoding | mesa3 re-encoding | Dosh (space folded) |
|---|---|---|---|
| keys per stroke | 4.56 | **4.11** | 1.66 per chord |
| keys per word | 8.36 | **7.52** | 8.13 |
| keys per hand per word | 4.2 | **3.78** | 4.06 |
| cycles, busiest hand | 1.68 | 1.68 | 2.45 |
| strokes of 9+ keys | 4.3% | **1.4%** | — |

So the 9-key port is not a compromise. It uses fewer keys per word than Dosh *and* fewer
hand cycles, and it cuts the very large chords by two thirds.

Generated by `midi4text-analysis/propose_mesa3.py`.

### A mapping that fits, rather than one that merely fits *well*

The table above is frequency-greedy, which is optimal for keypresses and unlearnable: no
relation holds between a chord's shape and what it spells. A usable mapping needs
structure, and two ideas pay for themselves (`midi4text-analysis/structured_map.py`):

**1. Free transfer from Dosh.** The consonant group contains Dosh's `s`, `t` and `n` keys,
and *those are also the three commonest Midi4Text consonants* — so they want single-key
chords anyway. Giving them their Dosh chords costs nothing and transfers directly.

**2. The pinky as a voicing modifier.** Reserving `a` leaves four keys giving 15 base
consonants, and adding the pinky gives 15 more — exactly the 30 needed. Where a
voiced/unvoiced pair exists, the rarer member is the commoner one plus the pinky.

| | chord | | | chord |
|---|---|---|---|---|
| n | `n` *(Dosh)* | | d | `at` = t + pinky |
| s | `s` *(Dosh)* | | b | `aot` = p + pinky |
| t | `t` *(Dosh)* | | g | `aos` = c + pinky |
| r | `o` | | v | `aon` = f + pinky |
| c | `os` | | z | `as` = s + pinky |
| p | `ot` | | sh | `a` + ch |

**The structure costs 5%**: 7.91 keys per word against the greedy 7.52 — still below
Michela's 8.36 and Dosh's 8.13. The pinky is engaged on 20.9% of consonant slots.

This is a sketch, not a finished layout. What it has not addressed: the vowel group has had
no equivalent design pass; the three rare coda-only shapes are parked on five-key chords;
nothing has been done about which chords are comfortable *in sequence* rather than
individually; and the mnemonic value of the pinky rule needs checking against how the
pairs actually feel. That work is the substance of any real attempt.

## 4. Learning effort

Because the mirror lets one alphabet serve both hands, the inventory is small:

| system | chords to memorise | dictionary |
|---|---|---|
| Dosh | 26 letters | none |
| Midi4Text | **43** (30 consonant + 13 vowel) | none |
| phonetic steno | ~30 key-to-sound mappings | 100k+ entries, briefs mandatory |

The acquisition curves are closer than the totals suggest, because both are dominated by
a few frequent shapes:

| coverage of running English | Midi4Text | Dosh |
|---|---|---|
| ~40% | 20 shapes | 10 chords |
| ~90% | 33 shapes | 20 chords |
| 100% | 43 shapes | 26 chords |

So the rote memorisation is about **1.65× Dosh**, and your suspicion is right that it is
far below a phonetic theory — Midi4Text's whole point is that it needs *no dictionary at
all*, where a phonetic system's dictionary is the bulk of the learning and never really
finishes.

**But the chord count is not the real cost.** Midi4Text adds two things Dosh has no
analogue for: about a dozen composition rules (the ending-vowel space marker, the
mirrored-vowel silent E, the inter-series clusters), and **syllable division as a live
decision**. The manual is explicit that English hyphenation does not apply, that words
have several legal divisions, and that awkward clusters force unnatural ones. That skill
is the part that takes months, and it is not bounded by a chord count. Realistically:
chords in a week or two, fluency gated by division practice.

## 5. On the 3× margin

With the space folded into Dosh, the chord ratio falls from 3.22× to **2.68×** — and
Dosh's space chord was already the cheapest and most pipelinable one it has. That is the
right way to read the margin: it is thin, and it is thin *before* accounting for the fact
that Midi4Text's strokes are serialised while Dosh's are not.

You are right that briefs are where Midi4Text would pull ahead — 1.83 strokes/word is a
brief-free figure, and a brief dictionary would cut it substantially. But that reopens
the objection the orthographic design exists to close: the moment briefs carry the speed,
the system is a dictionary system again, and its advantage over phonetic steno
evaporates. Briefs can equally be added to Dosh.

The margin is now measured rather than asserted — but see "Speed potential, with
alternation credited" in `03-mesa3-and-verdict.md`, which revises the Phase 6 verdict.
Crediting Dosh's alternation and Midi4Text's one-handed strokes, the comparison is 2.95
against 1.68 hand cycles per word, or 2.45 against 1.68 once the space is folded — a
ceiling of 1.46-1.76x rather than no gain at all.
