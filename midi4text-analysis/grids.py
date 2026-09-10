#!/usr/bin/env python3
"""Render the mapping as key grids rather than as concatenated key names.

A chord is a shape on the board, not a string spelling out key names, and
writing it as `aost` invites reading it as four letters.  These tables put one
column per key and mark the ones held.
"""

import collections

from m4t import corpus, layout as L, theory, writer

OUTER = ["a", "o", "s", "t", "n"]
INNER = ["e", "i", "Sp", "Bk"]


def row(chord, keys):
    held = set()
    i = 0
    while i < len(chord):
        for k in ("Sp", "Bk"):
            if chord.startswith(k, i):
                held.add(k)
                i += 2
                break
        else:
            held.add(chord[i])
            i += 1
    return ["#" if k in held else "." for k in keys]


def table(title, keys, rows, headers):
    print(title)
    width = max(len(h) for h, _ in rows) if rows else 4
    head = "  " + " ".join(f"{k:>3}" for k in keys)
    print(f"  {'':<{width}}  {head}   {headers}")
    for label, chord in rows:
        cells = " ".join(f"{c:>3}" for c in row(chord, keys))
        print(f"  {label:<{width}}   {cells}")


def main():
    import first_pass as F
    import propose_mesa3 as P
    import structured_map as S

    cons, _ = P.frequencies()
    cmap, _, _ = S.build(cons)
    d = corpus.load()
    chunks = writer.index(d)
    words = []
    for line in open("../words/count_1w.txt", encoding="utf-8", errors="replace"):
        p = line.split()
        if len(p) == 2 and p[0].isalpha():
            words.append((p[0], int(p[1])))
    words = words[:20000]
    f2 = collections.Counter()
    for w, fr in words:
        for st in writer.write(w, chunks):
            s2 = L.split(st)[1]
            if s2:
                f2[s2] += fr
    s2map = F.series2_map(f2)

    rows = []
    for shape, _ in cons.most_common():
        if shape in cmap:
            on = theory.ONSET.get(shape, "-")
            co = theory.CODA.get(L.mirror(shape), "-")
            rows.append((f"{on}/{co}" if on != co else on, cmap[shape]))
    table("OUTER FIVE - consonants, both hands (onset left / coda right)",
          OUTER, rows, "")

    print()
    rows = []
    for ident, chord in F.VOWEL_IDENTITY.items():
        rows.append((ident, chord))
        rows.append((ident + " +space", chord + F.END_MARKER))
    table("INNER FOUR, right hand - the vowel", INNER, rows, "")

    print()
    rows = [(theory.SECOND[p], s2map[p]) for p, _ in f2.most_common()]
    table("INNER FOUR, left hand - the second character", INNER, rows, "")


if __name__ == "__main__":
    main()
