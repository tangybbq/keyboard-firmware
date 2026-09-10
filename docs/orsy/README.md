# Orsy

An orthographic syllabic chord layout for the mesa3 (9 keys per hand), derived from the
Michela machine-shorthand layout by way of the Midi4Text theory. The name is short for
*orthographic-syllabic*.

One stroke spells one syllable, using both hands: onset and a second character on the
left, vowel and coda on the right. The inter-word space is folded into the last stroke of
a word rather than struck separately. There is no dictionary — the spelling follows from
the chord by rule.

- `01-mapping.md` — the chord assignment for all four Series
- `02-caps-numbers-commands.md` — capitals, and escaping to Dosh for numbers and symbols
- `03-implementation-spec.md` — how it should be built in the firmware

The analysis of the parent theory, and the measurements that justify the design, are in
`../midi4text/`.
