#!/usr/bin/env python3
"""A complete first-pass Midi4Text mapping for a 9-key-per-hand board.

Each hand: a (pinky), o/s (ring), t/n (middle), e/i (index), S/B (thumbs),
where S is the thumb that types Space in Dosh and B the one that types
Backspace.  Chords are written left-hand, hyphen, right-hand.

    consonant group  a o s t n     Series 1 (onset) left, Series 4 (coda) right
    vowel group      e i S B       Series 2 left, Series 3 (vowel) right

Three design rules, each costing a little against a pure frequency fit:

  * Dosh transfer.  s, t and n are the three commonest consonants and also
    Dosh keys, so they keep their Dosh chords.  Likewise e and i in the vowel
    group.
  * Pinky as voicing modifier.  Reserving `a` leaves 15 base consonants; the
    pinky doubles them to the 30 needed, and voiced/unvoiced partners pair up.
  * S is the word-end marker.  Midi4Text's central rule -- its own, not
    Michela's, which is phonetic and marks no spaces at all -- is that the
    ending form of a vowel is the plain one plus a key, which folds the
    inter-word space into the last stroke instead of spending a stroke on it.
    S is Dosh's Space thumb, so the mnemonic is exact.
"""

import collections

from m4t import corpus, layout as L, theory, writer

# ---------------------------------------------------------------- vowel group

# Seven vowel identities on e/i/B; S added marks the end of a word.
VOWEL_IDENTITY = {
    "e": "e",           # Dosh key
    "i": "i",           # Dosh key
    "a": "B",
    "o": "ei",
    "u": "iB",
    "ea": "eB",         # = e + a
    "ou": "eiB",        # = o + u
}
END_MARKER = "S"

# Series 3 patterns, as (identity, closes the word).
SERIES3 = {
    "a": ("a", False), "e": ("e", False), "i": ("i", False),
    "ie": ("o", False), "u": ("u", False), "ea": ("ea", False),
    "ua": ("a", True), "ue": ("e", True), "ui": ("i", True),
    "uie": ("o", True), "uia": ("u", True), "iea": ("ea", True),
    "ia": ("ou", True),
}

# Series 2 keeps the vowel chords wherever it means the same vowel, so a chord
# reads the same on either hand; the consonants fill in by frequency.  R and RI
# are the two commonest and take the remaining single keys.
SERIES2_FIXED = {"X": "e", "I": "i", "RXI": "ei", "U": "iB", "R": "S", "RI": "B"}


def series2_map(freq):
    import itertools

    fixed = dict(SERIES2_FIXED)
    used = set(fixed.values())
    chords = []
    for n in range(1, 5):
        for c in itertools.combinations("eiSB", n):
            chords.append("".join(c))
    out = dict(fixed)
    for pat, _ in freq.most_common():
        if pat in out:
            continue
        free = [c for c in chords if c not in used]
        if not free:
            break
        c = min(free, key=len)
        out[pat] = c
        used.add(c)
    return out


def main():
    import propose_mesa3 as P
    import structured_map as S

    cons, _ = P.frequencies()
    cmap, anchors, pairs = S.build(cons)
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
    s2map = series2_map(f2)

    print("CONSONANT GROUP   a (pinky), o/s (ring), t/n (middle)")
    print("  Series 1 on the left is the onset, Series 4 on the right the coda.\n")
    print(f"  {'onset':>6} {'coda':>6}  {'chord':<7} note")
    for shape, _ in cons.most_common():
        if shape not in cmap:
            continue
        note = "Dosh key" if shape in anchors else ""
        if shape in pairs:
            note = f"= {theory.ONSET[pairs[shape]]} + pinky"
        print(f"  {theory.ONSET.get(shape,'-'):>6} "
              f"{theory.CODA.get(L.mirror(shape),'-'):>6}  {cmap[shape]:<7} {note}")

    print("\nVOWEL GROUP, right hand -- Series 3")
    print("  Seven identities; add S to end the word.\n")
    print(f"  {'vowel':>6}  {'plain':<7} {'+ space':<8} note")
    seen = set()
    for ident, chord in VOWEL_IDENTITY.items():
        if ident in seen:
            continue
        seen.add(ident)
        note = {"e": "Dosh key", "i": "Dosh key",
                "ea": "= e + a", "ou": "= o + u"}.get(ident, "")
        print(f"  {ident:>6}  {chord:<7} {chord + END_MARKER:<8} {note}")

    print("\nVOWEL GROUP, left hand -- Series 2")
    print("  Same chord shapes; e, i, o and u read as the same vowel on both hands.\n")
    print(f"  {'spells':>6}  {'chord':<7} note")
    for pat, _ in f2.most_common():
        same = {"X": "e", "I": "i", "RXI": "o", "U": "u"}.get(pat)
        note = f"same chord as vowel {same}" if same else ""
        print(f"  {theory.SECOND[pat]:>6}  {s2map[pat]:<7} {note}")


if __name__ == "__main__":
    main()
