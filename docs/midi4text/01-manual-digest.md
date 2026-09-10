# Midi4Text — Phase 1: digest of the official manual

Source: `midi4text/ENG/Midi4Text manual (ver. 1.3.8).pdf` (Sillabix group, CC BY-SA 4.0).
Dictionaries: `Midi4Text main (eng) (1.3.9).json` and three small overlay dictionaries.

Everything below is **the authors' own claim**, transcribed from the manual. Statements
derived from the dictionary data rather than the manual are marked **[data]**. Phase 2
reconciles the two.

## 1. Provenance

Midi4Text is an adaptation of the **Michela** system — a 19th-century Italian machine
shorthand played on a piano keyboard, still used by the Italian Senate. Michela proper is
*phonetic*. Midi4Text keeps Michela's layout and finger assignments but reinterprets the
keys as **letters rather than sounds**, making it *orthographic* ("orthosyllabic" in the
authors' term). This is the crux of the theory: a stroke spells a syllable, it does not
sound one.

Where Michela distinguishes hard/soft/voiced consonants phonetically, Midi4Text reuses
those combinations for the corresponding English *spellings*: soft C → `ch`, hard G →
`gh`, voiced S → `z`, and the freed Michela Z-combination → `k` / final `ck`.

## 2. Physical layout

20 keys, 10 per hand, taken from two regions of a MIDI keyboard (D♯–C and E–C♯). Each
hand is split into two **Series**. The split is *positional*, not by key colour: each
hand covers ten contiguous semitones, and the Series are the first six and the last four
of them. (Series 1 is `F` D#2 black, `S` E2 white, `C` F2 white, `Z` F#2 black, `P` G2
white, `N` G#2 black — a mix.)

| Series | Keys | Role |
|---|---|---|
| 1st | `F S C Z P N` (6) | initial characters (syllable onset) |
| 2nd | `R X I U` (4) | "subsequent" characters — second element of onset, and mirrored vowels |
| 3rd | `u i e a` (4) | vowels (syllable nucleus) |
| 4th | `n p z c s f` (6) | final characters (syllable coda) |

Canonical steno order, used as the dictionary key: **`FSCZPN` `RXIU` | `uiea` `npzcsf`**.
Left-hand keys are written uppercase, right-hand lowercase.

Reference MIDI keymap (37-key keyboard):

| Left | | Right | |
|---|---|---|---|
| D♯2 | `F-` | E3 | `-u` |
| E2 | `S-` | F3 | `-i` |
| F2 | `C-` | F♯3 | `-e` |
| F♯2 | `Z-` | G3 | `-a` |
| G2 | `P-` | G♯3 | `-n` |
| G♯2 | `N-` | A3 | `-p` |
| A2 | `R-` | A♯3 | `-z` |
| A♯2 | `X-` | B3 | `-c` |
| B2 | `I-` | C4 | `-s` |
| C3 | `U-` | C♯4 | `-f` |

Each finger owns exactly two keys, which are never pressed together (thumbs excepted).
Three combinations deliberately violate this and are flagged **"extra-ordinem"**: `zcf`
(final `h`, middle finger shifted onto `z`), `zc` (final `ck`, right pinky shifted onto
`c`), and `zcs` (capitalize).

The layout is **specular**: the manual claims "75 percent of the sounds in the left
keyboard are repeated and mirrored in the right keyboard with the same combinations."
**[data]** the mirror is in fact exact for Series 1↔4 under `F↔f S↔s C↔c Z↔z P↔p N↔n`,
and exact for Series 2↔3 under `R↔a X↔e I↔i U↔u`.

## 3. Series 1 and 4 — initial and final characters

Mirror pairs. Left column is the onset spelling, right the coda spelling.

| S1 | S4 | Character(s) |
|---|---|---|
| `F` | `f` | f |
| `S` | `s` | s |
| `C` | `c` | sh |
| `FC` | `cf` | **initial** h / **final** st |
| — | `zcf` | final h *(extra-ordinem)* |
| `SC` | `cs` | v |
| `Z` | `z` | z |
| `FZ` | `zf` | th |
| `SZ` | `zs` | initial k / final k |
| `CZ` | `zc` | final ck *(extra-ordinem)* — **[data]** `CZ` = `{ck^}`, undocumented |
| `P` | `p` | p |
| `FP` | `pf` | t |
| `SP` | `ps` | ch |
| `CP` | `pc` | c |
| `FCP` | `pcf` | b |
| `SCP` | `pcs` | d |
| `SZN` | `nzs` | x |
| `ZP` | `pz` | g |
| `FZP` | `pzf` | gh |
| `SZP` | `pzs` | m |
| `N` | `n` | n |
| `FN` | `nf` | **initial** ind, und, gn / **final** nd, gn |
| `SN` | `ns` | **initial** inc, ing / **final** ng |
| `CN` | `nc` | w |
| `FCN` | `ncf` | r |
| `SCN` | `ncs` | l |
| `ZN` | `nz` | y |
| `FZN` | `nzf` | **initial** int / **final** nt, n't |
| — | `pzc` | `AE` digraph (with 3rd Series) |
| — | `zcs` | capitalize *(command, extra-ordinem)* |

**[data]** Every pattern in this table is attested in the dictionary, and the dictionary
introduces no others beyond `CZ` and the all-keys license chord. Series 4 uses all 32
subsets of its 6 keys; Series 1 uses 29.

## 4. Series 2 — subsequent characters

| Keys | Character(s) |
|---|---|
| `R` | r |
| `X` | s |
| `I` | i |
| `U` | u — *pressed alone: delete last stroke* |
| `RI` | l |
| `XI` | w / h ¹ (f, v ²) |
| `RU` | m |
| `XU` | n |
| `IU` | p / b ² — *also the number bar* |
| `RIU` | t / d ² |
| `XIU` | c / k ² / g ² |
| `RX` | **e** (mirrored vowel) |
| `RXI` | **o** (mirrored vowel) |

¹ after p, w, r, g. ² only in briefs and abbreviations.

Series 2 does double duty: consonant cluster material *and* the "mirrored vowels" of §6.

## 5. Series 3 — vowels

| Keys | Character | Ending-syllable form |
|---|---|---|
| `a` | a | `ua` |
| `e` | e | `ue` |
| `i` | i | `ui` |
| `ie` | **o** | `uie` |
| `u` | u | `uia` |
| `ea` | ° special use / blank space | `iea` (also apostrophe) |
| `ia` | \* special use | — |

Note the asymmetry: `o` is spelled `ie` and `u`'s ending form is `uia`, not `uu`.

### The blank space
Midi4Text uses the "space" convention (like Melani, unlike Velotype): **the space is
folded into the final stroke of a word**, never struck separately. It is signalled by
using the *ending-syllable* vowel, which is generally the standard vowel plus the `u`
key. Exceptions: `u` itself (→ `uia`) and the `ea` diphthong (→ `iea`).

## 6. The mirrored-vowel technique (silent final E)

For syllables/words ending in vowel `E` on a CVCV pattern, one stroke can be saved by
putting the vowel in **Series 2** and leaving Series 3 empty:

    A=R    e=X    i=I    o=XI    u=U    (ea=RX)

Doing so **automatically appends `e` and the final space**. Examples: tune = `FPUn`,
insane = `in/SRn`, hate = `FCRpf`, node = `NXIpcs`, update = `up/SCPRpf`, ease = `RXs`,
cease = `CPRXs`. Composable with the trailing-consonant technique: tunes = `FPUn/s`,
nodes = `NXIpcs/s`, hated = `FCRpf/pcs`.

## 7. Inter-series combinations

**1st + 2nd (onset clusters)**

| Keys | Initial |
|---|---|
| `FC`+`R` | str |
| `FC`+`RI` | spl |
| `FC`+`IU` | spr |
| `FC`+`XIU` | scr |
| `C`+`XIU` | sch |
| `Z`+`XIU` | sk |
| `S`+`X` | sci |
| `NZ`+`I` | j |
| `CP`+`XIU` | qu |

**2nd + 3rd (vowel digraphs)**

| Keys | Characters |
|---|---|
| `U`+`u` | au |
| `I`+`i` | ai |
| (any 2nd)+`ia` | ou (ending syllable) |
| (any 2nd)+`ea` | ea |
| (any 2nd)+`iea` | ea (ending syllable) |

The `ea`/`ia`/`iea` combinations exist so that `ea` and `ou` can be written when Series 2
is already occupied by a consonant — e.g. dread = `SCPRieapcs`, ground = `FZPRianf`,
tedious = `FPe/SCPIias`. The manual notes these are user-repurposable, since the same
words can always be split across two strokes.

**3rd + 4th**

| Keys | Characters |
|---|---|
| `a`+`pzc` | AE |
| `ua`+`pzc` | AE (ending syllable) |

## 8. Commands

| Keys | Command |
|---|---|
| `zcs` | capitalize — next word if alone, first letter of the syllable if combined |
| `ea` (alone) | add blank space |
| `RXea` | delete blank space |
| `iea` | apostrophe |
| `U` (alone) | delete last stroke |
| `IU` | number bar |
| `nzf` | carriage return |
| `FZNX` | indent |

## 9. Syllable division

Strict English hyphenation is **not required**. Any division the layout can represent is
legal, and the manual actively recommends violating hyphenation when a cluster won't fit:
fantastic = `fan-tas-tic` *or* `fant-as-tic` *or* `fant-a-stic`; attempts = `at-tem-pts`;
rhythm = `rhy-thm`.

### Trailing-consonant technique
If a word ends in a consonant the 4th Series cannot express, add an extra stroke using the
4th Series alone (the space is added automatically): add = `apcs/pcs`, nand = `Nan/pcs`.
Extends to clusters, drawing on Series 1, 2 and 4 together: crisps = `CPRis/ps`, fifths =
`Fif/FZs`, contexts = `CPien/FPenzs/FPs`, epitaph = `e/Pi/FPa/PXI`, attempts =
`apf/FPepzs/PRIUs`.

## 10. Numbers

Struck with Series 1 + Series 4 plus the `IU` number bar. Series 1 gives tens, Series 4
gives units; the digit shapes are chosen to recall a letter of the number's name.

| Tens | Keys | Units | Keys |
|---|---|---|---|
| 10 | `CN` | 1 | `nc` |
| 20 | `FP` | 2 | `pf` |
| 30 | `FZ` | 3 | `zf` |
| 40 | `F` | 4 | `f` |
| 50 | `ZN` | 5 | `nz` |
| 60 | `Z` | 6 | `z` |
| 70 | `S` | 7 | `s` |
| 80 | `FC` | 8 | `cf` |
| 90 | `N` | 9 | `n` |
| 00 | `SZ` | 0 | `zs` |

So 1 = `IUnc`, 10 = `CNIU`, 12 = `CNIUpf`, 98 = `NIUcf`. Three digits and up split across
strokes: 112 = `IUnc/CNIUpf`, 1226 = `CNIUpf/FPIUz`. Zero runs have dedicated chords
(`FCIUpcs` = 00, `FZIUnf` = 000, `SZPIUn` = 000,000, …).

## 11. Overlay dictionaries

Three small hand-written dictionaries sit **above** the main one in Plover's priority
order, so they win: punctuation & commands (20 entries), word parts (~85 prefixes,
infixes and suffixes, e.g. `NXue` = `{^ness}`, `SCNXIUuinz` = `{^logy}`, `FNencf` =
`{under^}`), and briefs (~180, e.g. `FR` = for, `I` = I, `IU` = you, `FN` = and).

The authors stress that briefs are **optional** — the system writes any word syllabically
without them — and that because the system is orthographic, a brief must avoid colliding
with letter sequences that occur inside real words.

## 12. What the manual claims about efficiency

The manual's argument is purely stroke-count: "window" is 2 strokes vs 7 QWERTY
keypresses; "personification" is 6 vs 16; "fantastic" is 3 vs 10. It claims >200 wpm is
attainable with machine shorthand generally, and that Midi4Text takes far less than the
usual 1.5–2 years because there is no abbreviation system to memorise. It offers **no**
measurement of its own, and does not discuss chording load or hand alternation. Phase 6
tests this claim.

## 13. Open questions carried into Phase 2

1. Series 2 is heavily overloaded (consonant, mirrored vowel, number bar, delete). How is
   the ambiguity resolved — purely by which other Series are occupied?
2. `FC` = initial `h` but final `st`; `SZ` = `k` both ways but `zs` also final `k`. How
   many Series 1/4 pairs are genuinely *not* mirror-symmetric in meaning?
3. The manual gives no rule for when the dictionary emits a bare fragment (`{f^}`,
   `{^bo}`) versus a whole syllable (`boo`, `boon`). This is the biggest gap between the
   prose and the data.
4. Which of the 15×15 Series-2/Series-3 combinations are legal, and why is `RXU`/`uea`
   the single excluded pattern in each?
