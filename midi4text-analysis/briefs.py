#!/usr/bin/env python3
"""How far briefs can take each system, and how fast they pay off.

Speed is measured in cycles on the busiest hand per word -- the number of
times a hand must form, strike and release a chord, which is what serialises.
Dosh's chords are one-handed and alternate, so a word costs half its chord
count; Midi4Text's strokes need both hands, so a word costs its stroke count.

A brief costs one cycle in Midi4Text (a two-handed stroke) and half a cycle in
Dosh (a one-handed chord) -- but Dosh has only 376 free one-handed chords,
where an 18-key Midi4Text has a quarter of a million strokes to spend.
"""

import sys

from m4t import corpus, writer, layout as L

DOSH_FREE_CHORDS = 376          # 4 thumb layers x 127 finger chords, less 132 used


def load_words(limit=20000):
    out = []
    with open("../words/count_1w.txt", encoding="utf-8", errors="replace") as fp:
        for line in fp:
            p = line.split()
            if len(p) == 2 and p[0].isalpha():
                out.append((p[0], int(p[1])))
            if len(out) >= limit:
                break
    return out


def main():
    d = corpus.load()
    chunks = writer.index(d)
    words = load_words()
    total = sum(f for _, f in words)

    rows = []
    for w, f in words:
        strokes = writer.write(w, chunks)
        rows.append((f, len(strokes), len(w) / 2))

    m_base = sum(f * m for f, m, _ in rows) / total
    d_base = sum(f * c for f, _, c in rows) / total
    m_gain = sorted(((m - 1) * f for f, m, _ in rows), reverse=True)
    d_gain = sorted(((c - 0.5) * f for f, _, c in rows), reverse=True)
    m_tot = sum(f * m for f, m, _ in rows)
    d_tot = sum(f * c for f, _, c in rows)

    print("cycles per word on the busiest hand (lower is faster)")
    print(f"{'briefs':>8} {'Midi4Text':>11} {'Dosh':>9}")
    for n in (0, 50, 100, 250, 376, 1000, 2500, 5000, 20000):
        m = (m_tot - sum(m_gain[:n])) / total
        c = (d_tot - sum(d_gain[: min(n, DOSH_FREE_CHORDS)])) / total
        wall = "  (Dosh exhausted)" if n > DOSH_FREE_CHORDS else ""
        print(f"{n:>8} {m:>11.2f} {c:>9.2f}{wall}")
    print(f"\nunbriefed fallback: Midi4Text {m_base:.2f}, Dosh {d_base:.2f}")


if __name__ == "__main__":
    sys.exit(main())
