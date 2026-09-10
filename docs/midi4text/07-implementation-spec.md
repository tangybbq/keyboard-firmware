# Implementation spec — an orthographic syllabic layout for mesa3

Working name **SYLO** (placeholder; "Midi4Text" names a MIDI keyboard this has nothing to
do with). Everything below refers to the mesa3 mapping in `05-first-pass-mapping.md` and
`06-caps-numbers-commands.md`, not to the original 20-key theory.

## Is enough settled? Yes, with three exceptions

Settled and validated: the four Series and their composition rules (reproduce 98.26% of the
shipped dictionary, `midi4text-analysis/validate.py`); the chord assignment for all four
Series; the word-boundary rule; capitals; the Dosh escape and toggle.

Not settled, and deliberately out of scope for v1:

1. **Briefs.** None. The syllabic rules alone give 1.83 strokes/word.
2. **Command arguments.** "Cap previous N words" needs its digit shapes chosen.
3. **The residue.** 1.74% of the shipped dictionary is not reproduced by the rules, mostly
   generator inconsistencies. **v1 implements the rules, not the dictionary** — there is
   nothing to be bug-compatible with.

## Architecture: steno's shape, none of its machinery

The decisive facts are that a stroke translates on its own and nothing later revises it,
and that undo is a count of characters. Together these remove almost everything the steno
path exists to provide.

| stage | steno | SYLO |
|---|---|---|
| chord accumulation | whole board, last-up | **same** |
| stroke type | `Stroke`, 23 steno keys | 18-bit mask, own order |
| translation | NFA over a binary dictionary | **pure function over ~110 table rows** |
| multi-stroke matching | longest match, retranslates | **none** |
| output | `Typer` with replace/undo protocol | **ring buffer, backspace N** |

So SYLO borrows steno's *front* (accumulate the whole board, fire on last release — not
Taipo's per-side rollover, which is wrong here because both hands form one chord) and
replaces everything behind it. It needs neither `bbq_steno::dict` nor `bbq_steno::typer`.

## Crate layout

    bbq-sylo/                    no_std, no dependencies, host-testable
      src/chord.rs               18-bit chord, the four Series, bit order
      src/tables.rs              the four alphabets (~110 rows)
      src/compose.rs             chord -> output, the composition rules
      src/output.rs              spacing, capitals, undo

    bbq-keyboard/src/layout/sylo.rs
      accumulation and the mode handler; converts scancodes to chord bits

A separate crate for the same reason `bbq-steno` is one: it is pure logic that wants host
tests, and the host tools can then exercise it. It does **not** depend on `bbq-steno`.

## Core algorithm

    fn translate(chord: Chord) -> Output

1. Split the 18 bits into four Series by mask.
2. Look up each in its table (onset, second, vowel, coda).
3. Apply the composition rules, in order: inter-series onset clusters; cross-hand vowel
   digraphs; the free combinations; the mirrored-vowel rule (Series 2 supplies the nucleus,
   append silent `e`, close the word); bare final Y.
4. Emit `text` plus two flags: `space_before`, `space_after`.

`midi4text-analysis/m4t/theory.py` is the reference; it is ~150 lines and the port is
close to mechanical.

## Output stage

Much smaller than the steno typer, because nothing is ever retranslated.

    struct Output {
        recent: [u8; 64],        // ring of characters typed, for retro-cap
        strokes: [u8; 8],        // characters emitted per recent stroke, for undo
        pending_space: bool,
        pending_cap: bool,
    }

- **Spacing.** A space is emitted before a word unless the previous stroke suppressed it.
  The word-boundary rule maps directly: ending-form vowel → space after; plain vowel → no
  space after; empty Series 3 → no space before.
- **Capitals.** `pending_cap` set by the cap-prefix stroke, consumed by the next letter.
- **Retro-cap.** Walk back N word boundaries in `recent`, backspace, retype.
- **Undo.** Backspace `strokes[last]` characters and pop. That is the whole of it.

## Mode integration

Add `LayoutMode::Sylo` behind a `sylo` feature, dispatched in `LayoutManager::handle_event`
beside `Taipo` and `Steno`.

The Dosh escape needs the layout manager, not the crate:

- **One-shot** — left inner `i`+`Sp`+`Bk` held with any right-hand chord: route the right
  hand's 9 bits through the Dosh table for that stroke only.
- **Toggle** — all four inner keys of one hand: switch to Dosh and back. Free in both
  layouts (`0x388` is unmapped in Dosh), so one shape does both, intercepted the way
  `dosh_event` already intercepts `DOSH_TOGGLE_KEY`. Raise a `MinorMode` so it is visible.

## Testing

1. **Differential against the Python model.** Enumerate all valid chords, compare the Rust
   output against `theory.py` through the mesa3 mapping. This is the real test and it is
   exhaustive — the whole chord space is only 262,144.
2. **Corpus.** Re-run `writer.py`'s stroke counts against the Rust implementation; strokes
   per word must come out at 1.83.
3. **Output stage.** Unit tests for spacing across stroke boundaries, capitals, retro-cap
   and undo.

## Not in v1

Briefs; punctuation beyond the Dosh escape; numbers beyond the Dosh escape; the three rare
coda-only shapes; a trainer mode.
