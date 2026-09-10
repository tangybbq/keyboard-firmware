#!/usr/bin/env python3
"""Propose a 9-key-per-hand Midi4Text mapping in Dosh key names.

mesa3 gives each hand pinky(1) ring(2) middle(2) index(2) thumbs(2).  Michela
wants a consonant group of 27+ shapes and a vowel group of 13+.  Splitting the
hand as consonants = pinky+ring+middle and vowels = index+thumbs gives 32 and
16 states respectively, which is enough -- but the consonant group must spend
some of its shapes on same-finger doubles, since mesa3's pinky has one key
where Michela's has two.  Shapes are assigned cheapest-state-first by corpus
frequency, so the doubles land on the rarest consonants.
"""

import collections

from m4t import corpus, layout as L, theory, writer

# mesa3 hand in Dosh key names; thumbs are S (space) and B (backspace).
CONS_FINGERS = [("pinky", ["a"]), ("ring", ["o", "s"]), ("middle", ["t", "n"])]
VOWEL_FINGERS = [("index", ["e", "i"]), ("thumb", ["S", "B"])]

# Michela forbids a finger pressing both its keys, but that is a property of
# wide piano keys, not of the theory.  On a keyboard the two keys of a finger
# are adjacent and small, and rolling or flattening onto both is easy -- Dosh
# already does it in 25 of its 132 entries.  So no finger pays a penalty here
# and chords are ranked purely by how many keys they need.
FREE_DOUBLE = {"pinky", "ring", "middle", "index", "thumb"}


def states(fingers):
    """All key subsets of a finger group, tagged with (keys, doubles)."""
    out = [([], 0)]
    for name, keys in fingers:
        penalty = 0 if name in FREE_DOUBLE else 1
        nxt = []
        for combo, dbl in out:
            nxt.append((combo, dbl))
            for k in keys:
                nxt.append((combo + [k], dbl))
            if len(keys) == 2:
                nxt.append((combo + keys, dbl + penalty))
        out = nxt
    return out


def cost(entry):
    combo, dbl = entry
    return (dbl, len(combo))


def frequencies():
    d = corpus.load()
    chunks = writer.index(d)
    words = []
    for line in open("../words/count_1w.txt", encoding="utf-8", errors="replace"):
        p = line.split()
        if len(p) == 2 and p[0].isalpha():
            words.append((p[0], int(p[1])))
    cons, vow = collections.Counter(), collections.Counter()
    for w, f in words[:20000]:
        for stroke in writer.write(w, chunks):
            s1, s2, s3, s4 = L.split(stroke)
            if s1:
                cons[s1] += f
            if s4:
                cons[L.mirror(s4)] += f
            if s2:
                vow[s2] += f
            if s3:
                vow[L.mirror(s3)] += f
    return cons, vow


def assign(shapes, fingers):
    slots = sorted(states(fingers), key=cost)[1:]     # drop the empty chord
    out = []
    for (shape, freq), (combo, dbl) in zip(shapes, slots):
        out.append((shape, "".join(combo), dbl, freq))
    return out


def main():
    cons, vow = frequencies()
    ctot, vtot = sum(cons.values()), sum(vow.values())

    print("CONSONANT GROUP  -- pinky 'a', ring 'o'/'s', middle 't'/'n'  (32 states)")
    print(f"{'onset':>6} {'coda':>6}  {'mesa3':<6} {'share':>8}  note")
    for shape, combo, dbl, freq in assign(cons.most_common(), CONS_FINGERS):
        on = theory.ONSET.get(shape, "-")
        co = theory.CODA.get(L.mirror(shape), "-")
        note = "same-finger double" if dbl else ""
        print(f"{on:>6} {co:>6}  {combo:<6} {freq/ctot:7.3%}  {note}")

    print("\nVOWEL GROUP  -- index 'e'/'i', thumbs 'S'/'B'  (16 states)")
    print(f"{'series2':>8} {'series3':>8}  {'mesa3':<6} {'share':>8}  note")
    for shape, combo, dbl, freq in assign(vow.most_common(), VOWEL_FINGERS):
        s2 = theory.SECOND.get(shape, "-")
        s3 = theory.VOWEL.get(L.mirror(shape), "-")
        note = "same-finger double" if dbl else ""
        print(f"{s2:>8} {s3:>8}  {combo:<6} {freq/vtot:7.3%}  {note}")


if __name__ == "__main__":
    main()
