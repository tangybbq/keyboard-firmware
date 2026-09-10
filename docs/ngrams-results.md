# N-gram results

Output of `words/ngrams.py`, recorded so the ranking can be read without re-running it.
Regenerate with:

```
uv run words/ngrams.py rank -n 60
uv run words/ngrams.py chords
```

See [n-grams.md](../n-grams.md) for the design and the reasoning behind the cost model.

## What was measured

Norvig's `count_1w.txt` — 333,307 usable word types with occurrence counts from the Google Web
Trillion Word Corpus.  Grams of 2–5 letters, candidates cut at a bound of 0.5 chords per 1000
letters, leaving 1,727 of them.  Weighted letter total 2,951,951,224,614.

Benefit is **chords saved per 1000 letters**, against that one fixed denominator.  The unit does
not vary with gram length: a bigram saves one chord per use and a five-gram four, both counted in
chords.  The baseline is what Taipo does today, one chord per letter, so exactly 1000 chords per
1000 letters.  The `cost after` column is what typing costs with everything down to that row
adopted.

Because a gram can never span a space in this corpus, no space elimination is involved.

## The ranking

| rank | gram | saved /1000 | cost after | | rank | gram | saved /1000 | cost after |
|-----:|------|------------:|-----------:|-|-----:|------|------------:|-----------:|
| 1 | the | 20.53 | 979.47 | | 31 | pro | 4.12 | 739.87 |
| 2 | in | 18.62 | 960.85 | | 32 | il | 3.79 | 736.07 |
| 3 | er | 14.77 | 946.07 | | 33 | ac | 3.62 | 732.45 |
| 4 | an | 14.08 | 931.99 | | 34 | de | 3.48 | 728.97 |
| 5 | tion | 13.89 | 918.10 | | 35 | for | 3.43 | 725.55 |
| 6 | re | 13.43 | 904.67 | | 36 | el | 3.10 | 722.44 |
| 7 | or | 10.34 | 894.34 | | 37 | ol | 2.96 | 719.48 |
| 8 | es | 9.73 | 884.60 | | 38 | am | 2.94 | 716.54 |
| 9 | en | 9.57 | 875.03 | | 39 | age | 2.80 | 713.74 |
| 10 | on | 9.18 | 865.85 | | 40 | ad | 2.80 | 710.94 |
| 11 | al | 8.79 | 857.06 | | 41 | ri | 2.64 | 708.30 |
| 12 | at | 8.26 | 848.80 | | 42 | ation | 2.60 | 705.70 |
| 13 | st | 7.87 | 840.93 | | 43 | with | 2.51 | 703.18 |
| 14 | ar | 7.82 | 833.12 | | 44 | ow | 2.47 | 700.71 |
| 15 | it | 7.70 | 825.41 | | 45 | ot | 2.44 | 698.27 |
| 16 | ou | 6.93 | 818.48 | | 46 | ur | 2.39 | 695.88 |
| 17 | ed | 6.78 | 811.70 | | 47 | ne | 2.37 | 693.51 |
| 18 | ic | 6.33 | 805.38 | | 48 | li | 2.49 | 691.02 |
| 19 | ing | 6.27 | 799.11 | | 49 | be | 2.37 | 688.66 |
| 20 | is | 6.13 | 792.98 | | 50 | us | 2.33 | 686.32 |
| 21 | to | 5.91 | 787.07 | | 51 | ter | 2.32 | 684.00 |
| 22 | and | 5.57 | 781.50 | | 52 | ve | 2.27 | 681.73 |
| 23 | of | 5.41 | 776.09 | | 53 | ay | 2.23 | 679.50 |
| 24 | le | 5.07 | 771.01 | | 54 | ect | 2.16 | 677.34 |
| 25 | th | 4.89 | 766.12 | | 55 | si | 2.13 | 675.21 |
| 26 | as | 4.74 | 761.38 | | 56 | te | 2.12 | 673.09 |
| 27 | om | 4.66 | 756.72 | | 57 | lo | 2.07 | 671.02 |
| 28 | ch | 4.34 | 752.38 | | 58 | un | 2.06 | 668.96 |
| 29 | se | 4.22 | 748.16 | | 59 | ce | 2.04 | 666.92 |
| 30 | ent | 4.17 | 743.98 | | 60 | vi | 2.16 | 664.76 |

Adopting all 60 takes typing from 1000 to 665 chords per 1000 letters — a third off.

Reading it:

- **Over half the total saving is in the first 20 rows.**  By rank 40 each further gram is worth a
  seventh of the first, and there is no cliff to stop at.  This is a menu to work down at whatever
  rate learning allows, not a set to adopt.
- **`th` is rank 25, not rank 1.**  Raw bigram frequency puts it first, which is why it was the
  string the first `Action::Text` chord got pointed at.  `the` is taken first and swallows most of
  its occurrences.  Nothing is invested in `th` yet, so repointing that chord (`ent`, code `0x04c`)
  at `the` is a one-line table edit worth four times as much.
- **Benefits are not monotonically decreasing.**  Rank 48 is worth more than rank 47.  Adding a
  gram can slightly raise another's benefit by moving where a word's best segmentation falls; the
  selection is still exact.  Do not "fix" this by sorting.
- **Whole words appear** — `the`, `in`, `and`, `of`, `to`, `for`, `with`.  They are legitimate
  adjacent-letter sequences, but they are the entries most likely to feel different to type, since
  the chord ends a word rather than sitting inside one.

## Free chords

Against the current 128 `TAIPO_ACTIONS` entries, 216 of the 255 finger patterns have both their
bare code and their `+Sp` capital available — the pair an n-gram chord needs.  No two-finger
pattern is free.  A chord already holding an `Action::Text` counts as available and is marked `*`:
those hold n-grams assigned by this same analysis, so reassigning one is a table edit, not a
commitment.

Ordered by the ease model in `ngrams.py` (lower is easier; see the weights at the top of the
file).  The three-finger patterns, which is where the easy chords are:

```
   2.45  0x0c8 ein       ..ni/...e  ..mi
   2.45  0x08c eit       ...i/..te  ..mi
*  2.47  0x04c ent       ..n./..te  ..mi
   2.47  0x0c4 int       ..ni/..t.  ..mi
   3.07  0x046 not       ..n./.ot.  .rm.
   3.07  0x064 nst       .sn./..t.  .rm.
   3.29  0x062 nos       .sn./.o..  .rm.
   3.29  0x026 ost       .s../.ot.  .rm.
   3.30  0x08a eio       ...i/.o.e  .r.i
   3.30  0x0a8 eis       .s.i/...e  .r.i
   3.54  0x02a eos       .s../.o.e  .r.i
   3.54  0x0a2 ios       .s.i/.o..  .r.i
   4.00  0x068 ens       .sn./...e  .rmi
   4.00  0x086 iot       ...i/.ot.  .rmi
   4.27  0x045 ant       ..n./a.t.  p.m.
   4.27  0x054 nrt       r.n./..t.  p.m.
   4.45  0x02c est       .s../..te  .rmi
   4.45  0x0c2 ino       ..ni/.o..  .rmi
   4.50  0x089 aei       ...i/a..e  p..i
   4.50  0x098 eir       r..i/...e  p..i
   4.54  0x023 aos       .s../ao..  pr..
   4.54  0x032 ors       rs../.o..  pr..
   4.77  0x051 anr       r.n./a...  p.m.
   4.77  0x015 art       r.../a.t.  p.m.
   4.80  0x04a eno       ..n./.o.e  .rmi
   4.80  0x0a4 ist       .s.i/..t.  .rmi
   4.82  0x013 aor       r.../ao..  pr..
   4.82  0x031 ars       rs../a...  pr..
   4.85  0x00d aet       ..../a.te  p.mi
   4.85  0x0c1 ain       ..ni/a...  p.mi
   4.85  0x01c ert       r.../..te  p.mi
   4.85  0x0d0 inr       r.ni/....  p.mi
   4.95  0x007 aot       ..../aot.  prm.
   4.95  0x070 nrs       rsn./....  prm.
   5.02  0x019 aer       r.../a..e  p..i
   5.02  0x091 air       r..i/a...  p..i
   5.20  0x049 aen       ..n./a..e  p.mi
   5.20  0x085 ait       ...i/a.t.  p.mi
   5.20  0x058 enr       r.n./...e  p.mi
   5.20  0x094 irt       r..i/..t.  p.mi
   5.20  0x00b aeo       ..../ao.e  pr.i
   5.20  0x083 aio       ...i/ao..  pr.i
   5.20  0x038 ers       rs../...e  pr.i
   5.20  0x0b0 irs       rs.i/....  pr.i
   5.75  0x043 ano       ..n./ao..  prm.
   5.75  0x034 rst       rs../..t.  prm.
   5.95  0x061 ans       .sn./a...  prm.
   5.95  0x016 ort       r.../.ot.  prm.
   6.20  0x029 aes       .s../a..e  pr.i
   6.20  0x0a1 ais       .s.i/a...  pr.i
   6.20  0x01a eor       r.../.o.e  pr.i
   6.20  0x092 ior       r..i/.o..  pr.i
   6.75  0x025 ast       .s../a.t.  prm.
   6.75  0x052 nor       r.n./.o..  prm.
```

The tiers are what to read here, not the exact numbers:

- **~2.45, index and middle only.**  Four chords, well clear of everything else, and `ent` is one
  of them.  What makes them easy is not that they are three keys but that they are two adjacent
  strong fingers with no row disagreement — one finger presses both of its keys while the other
  presses one.
- **~3.1–4.0, ring but no pinky.**  A step up, and within the group the cost is mostly whether the
  ring finger has to sit in a different row from its neighbour.
- **4.27 and up, pinky involved.**  Roughly half the three-finger patterns, and the reason the
  earlier code-ordered listing read almost backwards: low code bits are the pinky.

The model is judgement rather than measurement, and it is deliberately *not* fitted to the existing
table — Taipo's single letters are laid out mnemonically, not by ease (`a`, the third most common
letter, sits on a pinky), so fitting to letter frequency would learn the mnemonic instead.  The one
calibration point is that `ent` comes out top, which it does.

## Assigned

Implemented in `TAIPO_ACTIONS`.  The thirteen best grams on the thirteen easiest chords that leave
the pinky out, in order — easiest chord to most valuable gram.

| chord | code | ease | types | +Sp | gram rank |
|-------|------|-----:|-------|-----|----------:|
| eit | `0x08c` | 2.45 | the | The | 1 |
| ein | `0x0c8` | 2.45 | in | In | 2 |
| ent | `0x04c` | 2.47 | er | Er | 3 |
| int | `0x0c4` | 2.47 | an | An | 4 |
| not | `0x046` | 3.07 | tion | Tion | 5 |
| nst | `0x064` | 3.07 | re | Re | 6 |
| nos | `0x062` | 3.29 | or | Or | 7 |
| ost | `0x026` | 3.29 | es | Es | 8 |
| eis | `0x0a8` | 3.30 | en | En | 9 |
| eio | `0x08a` | 3.30 | on | On | 10 |
| eos | `0x02a` | 3.54 | al | Al | 11 |
| ios | `0x0a2` | 3.54 | at | At | 12 |
| ens | `0x068` | 4.00 | st | St | 13 |

Together they take typing from 1000 to about 841 chords per 1000 letters, roughly
16% of all keystrokes, if every one of them gets used.

Notes on the choices:

- **`th` is not among them.**  It is the top bigram by raw count and rank 25 here, because `the`
  has a chord and takes most of what `th` would have saved.  The chord that used to type `th`
  (`ent`) now types `er`.
- **`ist` is excluded** although it is available and no harder to reach than `ens`.  Middle finger
  down while both its neighbours go up is the splay case; the ease model puts it at 4.80, third
  worst of the no-pinky triplets, and it is awkward to execute.
- **Ties are broken by shared letters.**  Ease ties are exact, so the pairing within one is free;
  it goes to whichever gram shares letters with the chord's keys.  `ein` types `in` and `eit`
  types `the` for that reason.  It is a learning aid, nothing the engine knows about.
- **`Tion` is not useful**, but the capital variants are uniform rather than case-by-case, so it
  exists.

Five more no-pinky triplets remain unassigned: `iot` (4.00), `est` and `ino` (4.45), `eno` and
`ist` (4.80).  The next grams down the list are `ar`, `it`, `ou`, `ed`, `ic`.

## Extensions

```
uv run words/ngrams.py extend
```

The ranking scores a set once it is fully adopted.  What gets learned is a *prefix* of it, and in
the gap between the two, some of the thirteen train a habit that a gram further down the list
takes apart.  Measuring how much of each assigned chord's use another gram would claim:

| extension | saves /1000 | takes | of |
|-----------|------------:|------:|----|
| `ation` | 2.60 | 56% | `tion` |
| `ent` | 4.19 | 42% | `en` |
| `and` | 5.57 | 40% | `an` |
| `ing` | 6.27 | 34% | `in` |
| `for` | 3.43 | 31% | `or` |

A third to a half of five of the thirteen is a habit with an expiry date, and `ing` is only six
places further down the greedy list — `ar`, `it`, `ou`, `ed`, `ic` first — so the habit would be
five lessons old before anything broke it.  All five are adopted for that reason rather than for
their rank: `ar` and `it` are each worth more than any of them.

Chord supply turned out not to be the constraint.  The four-finger no-pinky patterns are free and
several are easier than three-finger ones already in use, and each extension fits on **its base's
chord plus one key**, so the pair is one shape rather than two:

| chord | code | ease | types | = base + | gram rank |
|-------|------|-----:|-------|----------|----------:|
| eint | `0x0cc` | 2.87 | ing | `ein` + t | 19 |
| nost | `0x066` | 3.71 | for | `nos` + t | 35 |
| eios | `0x0aa` | 3.94 | ent | `eis` + o | 30 |
| inst | `0x0e4` | 4.07 | and | `int` + s | 22 |
| enot | `0x04e` | 4.07 | ation | `not` + e | 42 |

Each follows its base in `TAIPO_ACTIONS`, which is what makes them one lesson in
[taipo-drills.md](taipo-drills.md).  Eighteen chords take typing from 1000 to about 819 per 1000
letters.

Running `extend` against the eighteen shows the effect does not stop, only shrinks: `ment` claims
41% of the new `ent`, `ate` 25% of `at`, `all` 23% of `al`, `con` 22% of `on`, `ter` 21% of `er`.
None of them is acted on, because the share is only worth paying for when the extension is coming
soon anyway — `ing` was six places down the list, while `ment` at 1.73 per 1000 is past the end of
it.  A habit that nothing will break for fifty chords is not a habit with an expiry date.

The one cost that is not in any of these numbers: `ing` and `in` now sit one key apart and compete
inside the same words, so a sloppy press misfires into its twin rather than into nothing.  Whether
that outweighs learning the pair as a single shape is a question for the hands.

## Caveats

These bear on how much to trust the tail of the list, not the top of it.

- The corpus is web-flavoured — `home`, `page`, `search`, `site`, and `news` are all in the top 60
  words.  Outright junk is far enough down not to dominate (`com` is rank 1112, `www` 1943), but
  the vocabulary leans toward web prose rather than books.
- It is lowercase and letters-only, so capitalisation patterns and apostrophes are invisible.
- **No source code is represented.**  Identifier grams (`str`, `ptr`, `self`, `_t`) are absent
  entirely, as is anything involving punctuation.

### How much does the corpus actually matter?

Less than it looks.  Re-running against the MonkeyType `english_10k` list in `words/` — a different
vocabulary, a different source, and Zipf (1/rank) weights instead of real counts — gives a top 25
that **shares 23 of its 25 entries** with the run above.  Only `to` and `le` drop out, replaced by
`for` and `om`.

The order shifts (Zipf badly over-weights the single most common word, so `the` scores 51.6 there
against 20.5 here), but which grams are worth a chord is close to corpus-independent.  Fine
distinctions in the tail move around; the part anyone would actually learn does not.

So the missing source code is unlikely to change the answer, and matters even less if code is not
typed at speed.  `--words` takes any list in the same `word<TAB>count` format if it is worth
re-checking later.
