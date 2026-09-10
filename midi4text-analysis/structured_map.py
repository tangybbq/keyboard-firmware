#!/usr/bin/env python3
"""A structured 9-key mapping, and what its structure costs.

propose_mesa3.py assigns chords greedily by frequency, which is optimal for
keypresses and awful to learn -- the result has no relation between a chord's
shape and what it spells.  This builds a mapping with two organising ideas and
measures the price:

  1. Dosh transfer.  The consonant group contains Dosh's `s`, `t` and `n`
     keys, and those are also the three commonest Midi4Text consonants, so
     they keep their Dosh single-key chords and transfer for free.

  2. The pinky as a voicing modifier.  With `a` reserved, the other four keys
     give 15 base consonants, and adding the pinky gives 15 more.  Where a
     voiced/unvoiced or related pair exists (t/d, s/z, f/v, p/b, c/g, sh/ch),
     the partner is the base plus the pinky.
"""

import collections
import itertools

from m4t import corpus, layout as L, theory, writer

BASE_KEYS = ["o", "s", "t", "n"]        # ring bottom/top, middle bottom/top
PINKY = "a"

# Keyed by what the shape spells; resolved to Michela patterns at run time.
ANCHORS_BY_SPELLING = {"n": "n", "s": "s", "t": "t"}   # free transfer from Dosh
PAIRS_BY_SPELLING = {"d": "t", "z": "s", "v": "f", "b": "p", "g": "c", "ch": "sh"}


def by_spelling():
    """Map an onset spelling back to its Michela Series 1 pattern."""
    return {v: k for k, v in theory.ONSET.items() if k}


def base_chords():
    out = []
    for n in range(1, len(BASE_KEYS) + 1):
        for combo in itertools.combinations(BASE_KEYS, n):
            out.append("".join(combo))
    return out                                       # 15 of them


def build(freq):
    pat = by_spelling()
    ANCHORS = {pat[k]: v for k, v in ANCHORS_BY_SPELLING.items() if k in pat}
    PAIRS = {}
    for k, v in PAIRS_BY_SPELLING.items():
        if k not in pat or v not in pat:
            continue
        a, b = pat[k], pat[v]
        # The commoner member is the base; the rarer one is base + pinky.
        if freq[a] > freq[b]:
            a, b = b, a
        PAIRS[a] = b
    order = [s for s, _ in freq.most_common()]
    chords = base_chords()
    assigned, used = {}, set()
    for shape, chord in ANCHORS.items():
        assigned[shape] = chord
        used.add(chord)
    # Bases first, commonest to rarest, skipping shapes that are a pair's partner.
    partners = set(PAIRS)
    for shape in order:
        if shape in assigned or shape in partners:
            continue
        free = [c for c in chords if c not in used]
        if not free:
            break
        c = min(free, key=len)
        assigned[shape] = c
        used.add(c)
    # Then each partner as its base plus the pinky.
    for shape, base in PAIRS.items():
        if base in assigned:
            assigned[shape] = PINKY + assigned[base]
    # Anything left over takes the cheapest remaining pinky chord.
    taken = set(assigned.values())
    for shape in order:
        if shape in assigned:
            continue
        free = [PINKY + c for c in chords if PINKY + c not in taken]
        if not free:
            break
        c = min(free, key=len)
        assigned[shape] = c
        taken.add(c)
    return assigned, ANCHORS, PAIRS


def main():
    import propose_mesa3 as P

    cons, vow = P.frequencies()
    structured, ANCHORS, PAIRS = build(cons)
    greedy = {s: c for s, c, _, _ in P.assign(cons.most_common(), P.CONS_FINGERS)}
    ctot = sum(cons.values())

    def mean(m):
        return sum(len(m[s]) * f for s, f in cons.items() if s in m) / ctot

    print(f"mean keys per consonant slot: structured {mean(structured):.2f}, "
          f"frequency-greedy {mean(greedy):.2f}")
    pinky = sum(f for s, f in cons.items() if s in structured and PINKY in structured[s])
    print(f"consonant slots using the pinky: {pinky / ctot:.1%}")
    print(f"shapes placed: {len(structured)} of {len(cons)}\n")

    print(f"{'spells':>8}  {'chord':<6} {'share':>7}  note")
    for shape, f in cons.most_common():
        if shape not in structured:
            continue
        on = theory.ONSET.get(shape, "-")
        co = theory.CODA.get(L.mirror(shape), "-")
        note = ""
        if shape in ANCHORS:
            note = "Dosh key"
        elif shape in PAIRS:
            note = f"= {theory.ONSET[PAIRS[shape]]} + pinky"
        print(f"{on:>4}/{co:<4}  {structured[shape]:<6} {f/ctot:6.2%}  {note}")


if __name__ == "__main__":
    main()
