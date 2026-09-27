# Orsy — how much there is to learn

A count of what a writer has to learn in Orsy, for comparison with other layouts. The
figures are counted from the tables in `bbq-orsy` (`Outer::ALL`, `Second::ALL`,
`Vowel::ALL`, `commands`, `punctuation`, and the rule flags in `compose::rules`) as of
September 2026, and have to be recounted after a table or rule change.

Orsy has no dictionary. Every stroke is spelled by rule from four small pattern tables, so
what is learned is the patterns and the rules, not the strokes. About 170,000 strokes
follow from them: 170,120 chords translate, not counting the commands.

## Chord shapes: 74

| group | shapes | |
|---|---|---|
| outer five: onset on the left, coda on the right | 30 | one set of shapes for both hands |
| second character (left inner four) | 13 | `r l i t m o s e w c u p n` |
| vowel (right inner four) | 13 | seven vowels, plain or ending |
| commands | 10 | |
| punctuation marks | 8 | `. , ' ? - ! : ;` |
| **total** | **74** | |

- **Outer five.** 23 shapes spell the same consonant as an onset and a coda. Four read
  differently as a coda (`h`/`st`, `ind`/`nd`, `inc`/`ng`, `int`/`nt`), and three are
  coda-only (a final `h`, a tail `e`, and the capitalising coda). Voiced pairs add the
  pinky: d = t + pinky, b = p + pinky, g = c + pinky, v = f + pinky, z = s + pinky,
  sh = ch + pinky.
- **Second character.** Seven of the thirteen reuse a vowel's shape from the right hand:
  `r` (`a`), `i`, `s` (`e`), `e` (`ea`), `o`, `u`, and `w` (the ending `o`).
- **Vowel.** Seven vowels, `a e i o u ea ou`, each plain or with `Bk` added to end the
  word; `ou` has only the ending form. In practice that is seven shapes and one habit.
- **Commands.** Seven left inner shapes struck alone: undo, join, cap next, all caps, cap
  previous, uncap next and uncap previous. Then the space (right `Bk` alone), the Dosh
  one-shot and the Dosh toggle.
- **Punctuation.** The marks that affect spacing, as right `Bk` plus outer keys.
  Everything else, digits included, comes from Dosh through the one-shot or the toggle.

The 74 overstates the new material. The 30 outer shapes are one set used by both hands,
and five shapes mean the same as in Dosh: the outer `s`, `t` and `n` and the vowels `e`
and `i`.

## Rules: 7, on a fixed structure

The structure applies to every stroke:

- The four Series always read in order: onset, second character, vowel, coda. *dream* is
  d + r | ea + m.
- One shape per Series. Adding a key changes the consonant rather than adding one.
- Spacing follows from the vowel. The ending form (`+ Bk`) closes the word and adds the
  space, a plain vowel joins onto the next stroke, and a stroke with no vowel joins onto
  the one before.

On top of that are the seven composition rules that `compose::rules` flags, and that the
trainer tracks as separate items:

1. **Mirrored vowel and silent `e`.** With no vowel on the right, a second character that
   is also a vowel becomes the vowel and a silent `e` is added: `time`, `name` and `life`
   are one stroke each.
2. **Onset clusters.** Nine onset and second-character pairs spell a cluster outright:
   `str spl spr scr sch sk sci j qu`.
3. **`w` reads as `h`** after `p`, `w` and `r`: `when`, `phase`.
4. **Diphthongs.** A second-character vowel fuses with the vowel into `au` or `ai`.
5. **The free vowel pairs.** `ea` and `ou` on the right spell the digraph only after a
   second character (`great`, `cloud`). Otherwise they are written across the hands
   (`eat`, `out`).
6. **Bare final `y`.** A word-final `y` on its own is struck with the ending `i`
   (`i` + `Bk`), which then spells nothing and only supplies the space: `eas·y`.
7. **Capitalising coda.** A coda shape that capitalises the syllable, inherited from
   Midi4Text. The capital commands are the usual way.

Beyond these is the skill with no Dosh equivalent: dividing a word into syllables the way
the layout spells them.

## Compared with Dosh

A word here is the standard five characters, space included, as in words per minute.

| | shapes | rules | strokes per word | on the busiest hand |
|---|---|---|---|---|
| Dosh | 133 chords, each a letter, symbol or key | none | 5.00 | 2.50 |
| Orsy | 74 | 7 | 1.55 | 1.42 |

Orsy takes about a third as many strokes as Dosh. Dosh's hands alternate, though, so one
hand forms its next chord while the other is still striking; counted per hand, Orsy's
advantage is about 1.76×. Every Orsy stroke uses both hands, and each is a larger chord.
Whether a larger chord takes longer to form is the open question that decides how much of
that advantage is real.

Dosh types a chord per character, so its figures are exact. Orsy's depend on real word
lengths, because a stroke is a syllable, not a fixed number of characters. They are
converted from `../midi4text/03-mesa3-and-verdict.md`, which counts per dictionary word:
1.83 strokes per word against Dosh's 5.90, frequency-weighted over the top 20,000 words
of `words/count_1w.txt`. Those words average 4.90 letters, so 5.90 characters with the
space, and every figure is scaled by 5 / 5.90. They are corpus measurements, not typing
speeds.
