# Orsy — training plan

Follows `03-implementation-spec.md`. The firmware side of Orsy exists and has not been
typed on. This is the plan for learning it, written for a writer who already knows Dosh
well and has the TaipoTeacher trainer (`taipo-teacher/`) and its key logs.

The short version: **the unit of skill is the pattern, not the stroke**; the first
deliverable is static drill sheets that need no app work; the app then learns to read Orsy
strokes from the logs it already collects, and grows a per-pattern ladder and a drill that
judges by the text a stroke produces rather than by the stroke itself.

## What is actually being learned

Dosh is one skill: 133 chords, each a letter. Orsy is three:

1. **The patterns.** 30 outer shapes, each read as an onset on the left and a coda on the
   right; 13 vowel patterns (seven identities, plain and ending form); 13 second-character
   patterns. About 70 things, from which 170,127 strokes follow.
2. **The composition rules.** Mirrored vowels and the silent `e`, the onset clusters
   (`str`, `qu`, `j`), `XI` as `h` or `w`, the diphthongs, `ea` and `ou`, bare final `y`,
   and when a fragment binds forward or back. Each is a habit to acquire, not a chord.
3. **Syllable division.** Deciding where a word splits into strokes, and which division
   is cheapest. Nothing in Dosh resembles this; it is the skill the corpus figures assume
   and that the spec says only hands can settle.

Plus five commands, of which the Dosh one-shot is the one used every sentence.

### What transfers from Dosh, and what fights it

Same keys, same bit numbering, so every Orsy chord already means something to Dosh
hands. Computed from `layouts.json` against `orsy-mapping.json`:

| outer shape | Dosh | Orsy | | outer shape | Dosh | Orsy |
|---|---|---|---|---|---|---|
| `n` `s` `t` | n s t | **n s t** | | `a+o+t` `a+o+n` `a+t+n` | — | v m k |
| `a` | a | r | | `a+s+t` `a+s+n` | — | ind/nd inc/ng |
| `o` | o | c | | `o+s+t` `o+s+n` | — | ch int/nt |
| `a+t` | q | d | | `a+o+s+t` `s+t+n` | — | sh gh |
| `o+s` | alt | p | | `a+s` `a+o+s+n` | — | z ck |
| `o+t` | u | f | | `o+t+n` | PrintScreen | x |
| `a+n` | j | y | | | | |
| `o+n` | g | th | | | | |
| `s+t` | ? | l | | | | |
| `a+o+s` | gui | b | | | | |
| `s+n` | p | w | | | | |
| `a+o` | l | g | | | | |
| `t+n` | control | h/st | | | | |

Three shapes transfer, fourteen are free in Dosh, and **thirteen actively conflict**: the
hand already has a habit for them and it is wrong. The vowel group is the same story:
`e` and `i` transfer, `e+i` is Dosh's shift and `i+Sp` its full stop. Series 2 on the left
transfers `e`, `i`, `o`, `u` as vowels but reads `e` alone as `s`.

So the training has to do what `ConfusionDrill` does for Taipo, deliberately: the
conflicting shapes are drilled as pairs against their Dosh meaning, and the hint has to
name the Dosh letter the hand will want to type. The free shapes are the easy ones; the
transfers are free.

## The unit of skill

The existing model keys everything on one chord code: `ChordSamples` per `UInt16`,
`LadderItem.codes`, the `SkillStore` keys, `Confusion.fingerStates`. That cannot carry
Orsy, and not because of the width: a stroke is not the thing being learned. Nobody
learns 170k strokes; they learn ~70 patterns and a dozen rules.

So an Orsy stroke is **split into its Series and each pattern gets the sample**:

- **Timing.** The gap from the previous stroke, as now, attributed to every pattern in the
  stroke. A stroke with one unlearned pattern is slow because of that one, and the
  attribution to the learned ones is noise the 64-use window absorbs. Per-pattern medians
  are what the ladder's confidence and hint fade read.
- **Errors.** An undone stroke (the undo command, or Dosh-escaped backspaces that erase a
  whole stroke) is compared with the stroke that replaced it, and **the Series that differ
  take the blame**. If every Series differs, the whole stroke was wrong and all of them do.
  In a drill the target is known, so blame is exact.
- **Rules** are items too. A rule item is reached when enough strokes that exercise it have
  been reached: mirrored-vowel strokes for the silent `e`, `FC+R` for `str`, and so on.
  The rule set is enumerable from `compose.rs`.
- **Division** is measured per word, not per pattern: strokes used against the fewest
  possible, from the same DP the drill sheets use. It is a running rate shown as a
  technique figure, like the same-hand rate is for Taipo, and gates nothing.

## Step 0: drill sheets, before any app work

A host-only example in `bbq-orsy`, `lessons`, that prints lesson word lists as markdown into
`docs/orsy/drills/`, the way `words/drills.py` does for the n-gram chords. Each lesson is
a set of patterns; a word belongs to the lesson that completes its cheapest division.
Every word is printed with its strokes in key names:

    dream    d+r | ea+m      at+Bk  -  eSp+aon

This needs one new piece of logic, **`bbq_orsy::write(word)`**: the fewest-stroke division
of a word, ties broken by fewest keys, a port of `m4t/writer.py`'s DP over
`translate`. It is behind a `std` feature and is reused by everything below.

The keyboard already prints text, so the sheets work in MonkeyType or an editor with the
keyboard in Orsy mode, and **the Live tab already logs every key event regardless of
mode**, so practice done this way is not lost: once the replay learns to read Orsy strokes
(Phase A) the logs replay retroactively, as the Taipo ones did.

Lesson order, for a Dosh hand:

1. **Transfer.** `s` `t` `n`, vowels `e` `i` plain and ending, the space and undo commands.
   Words: ten, sit, net, tin, its, tent, nest. This is where syllable division is met
   first, on words whose patterns are all known.
2. **The conflicts, one pair at a time**, commonest first: `a` (r, not a), `o` (c),
   `a+t` (d), `o+s` (p), `o+t` (f), and the rest of the thirteen.
3. **Vowels** `a` `o` `u`, then `ea` and `ou`; **Series 2** by frequency, `r` `l` first.
4. **The free shapes** by frequency.
5. **The rules**, each with words that need it: mirrored vowels (tame, hide, tone), onset
   clusters (strap, quit, jam), `h`/`w` (when, phase), diphthongs (paul, pail), `y`.
6. **The escapes**: the one-shot for punctuation with the Dosh chords already known; the
   toggle for a run of digits.

A day's work, and it gives something to type tomorrow.

## Phase A: replay, synth and export (Rust)

The host analysis is the firmware itself, run over logs, and it does not yet see Orsy
strokes: `OrsyManager` reports nothing through `LayoutActions`, so `replay` derives only
the key presses it types.

- **`LayoutActions::orsy_stroke(chord, outcome)`**, reported by `OrsyManager::stroke`
  for every committed stroke, command and untranslatable chord included, before anything
  is typed — the counterpart of `taipo_chord`. `replay` turns it into
  `Derived::Stroke { time_ms, chord, first_key_ms, last_key_ms, text, .. }`, and
  `derived_to_text` prints it, so golden files cover it.
- **`synth`** grows an Orsy style: a target text divided by `write`, strokes with a spread
  and a hold, periodic errors that are undone by the undo command and by Dosh-escaped
  backspaces. This is what produces the golden logs the Swift side is checked against.
- **`layouts.json`** gains an `orsy` section: the masks, the three tables (bits, Michela
  name, spelling, ending form, mirrored vowel), the commands, and **its own fingerprint**
  over the tables. It is *not* a third variant: the variant model is one-hand codes, and
  every consumer of `variants` would misread it. `sync-layouts.py` records it in the
  layout history like the others, so a table change later costs only the patterns that
  moved.
- The `check_rust.py` comparison already guards the tables and rules; this adds a check
  that the export agrees with them.

Small: a few hundred lines, mostly tests.

## Phase B: the Swift model

- **Reading the log.** `ChordEngine.marker` ignores the `mode` marker, so today the app
  would assemble Orsy key events into per-hand Taipo chords and fold them into Dosh's
  skill — silently, in exactly the way the fingerprint machinery exists to prevent. The
  engine honours `mode`; in Orsy it hands events to a **`StrokeEngine`** mirroring
  `OrsyManager` (whole board, first release commits, a press while releasing starts a new
  stroke from what is held), checked line-for-line against the Rust goldens as
  `Chording.swift` is now.
- **`OrsyTheory.swift`**: a third port of the composition rules, from the exported tables.
  ~150 lines. Tested against a sampled golden dump from `cargo run --example dump` (every
  chord with the left hand in a fixed thousand-chord sample, plus the hand-worked cases
  from `compose.rs`), not the full 170k lines. The alternative of bundling the full dump as
  a lookup table was considered and rejected: the drill has to say *which Series* was
  wrong, and that needs the tables in hand anyway.
- **Per-pattern skill.** A `StrokeSkillCollector` beside `SkillCollector`: samples keyed by
  Series and pattern (`"s1:FC"`, `"s3:uia"`, `"rule:silent-e"`), timing and blame as
  above. The same `ChordSamples`, `Options` and `everReached` gate, so the ladder code
  needs no new notion of "reached". `SkillStore` caches it under the Orsy fingerprint,
  separately from the Taipo/Dosh model, so the two invalidate independently — which also
  answers the open per-variant-fingerprint item in `TASKS.md` for Orsy, if not yet for
  Dosh.
- **The word table.** `orsy-words.json`, generated by the `lessons` example: every word of
  `english_10k` with its cheapest division and the patterns it needs. Bundled, so the
  Swift side never needs the inverse of the theory (text to stroke); it filters this table
  by the unlocked pattern set, and judges typing with the forward port.

With this much the **Live tab shows Orsy strokes and their text**, and the skill model
fills from every log, before there is a drill.

## Phase C: the drill

- **`OrsyLadder`.** Items are patterns and rules, staged as in Step 0: transfer, conflicts,
  vowels and seconds, free shapes, rules, escapes. Unlocking is the current rule — reach,
  not speed — with focus on the least confident unreached items. Words for a line come
  from the word table restricted to unlocked patterns, one per focus item, plus one older
  item, as `LadderMaker` does now.
- **Judging by text, not by stroke.** `DrillSession` compares chord codes to a
  segmentation. Orsy has more than one valid division of most words, and a writer who
  chooses a different one is right. So the Orsy session runs the typed strokes through the
  theory and a small output stage (the spacing flags and pending capital), and compares
  **the text produced** with the target prefix. The fewest-stroke division is what the hint
  shows, and using more strokes than it is reported the way `spelled` grams are, not as an
  error. A wrong stroke is diagnosed by Series against the hinted stroke, and against the
  Dosh meaning of the shape when that is what came out.
- **The hint.** Both hands, eighteen keys, the four Series in four colours, fading by the
  least confident pattern in the stroke; the division drawn under the word (`dre·am`).
  `ChordDiagram` assumes one right hand of ten keys and is replaced for this view, not
  bent.
- **Stats.** Strokes per minute and keys per stroke; **stroke spread against key count**,
  which is the spec's unanswered question — whether a five-key chord forms as fast as a
  two-key one — measured for free on every stroke; divisions per word against the
  optimum. The alternation strip is meaningless for two-hand strokes and is hidden in Orsy.
- Auto-capitalisation after a one-shot `.`, the undo command and the space command all
  go through the drill's output stage so the target stays in step with the keyboard.

## Phase D: on the hands

Nothing here has been typed, so the thresholds are guesses and two decisions wait on
Phase A's measurements: whether first-up is the right commit rule for a two-hand stroke
(a ragged release that splits one stroke into two will show up as untranslatable strokes
in the log), and whether `SkillModel.Options` — 12 samples to reach, 700 ms target —
suit chords of twice the size.

## Order and size

| step | what it gives | size |
|---|---|---|
| 0 | drill sheets; `write`; practice starts, logs accumulate | a day |
| A | strokes visible to the replay; goldens; export | small |
| B | the app reads Orsy logs; Live tab; per-pattern skill | medium |
| C | the ladder drill | large |
| D | tuning, and the spread measurement | ongoing |

Step 0 first, then A, because everything after depends on the goldens and the export, and
because logs typed before A are not lost. B is worth shipping without C: seeing strokes
and their text live, with the per-pattern skill filling in, is most of the feedback.
