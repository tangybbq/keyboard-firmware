#!/usr/bin/env python3
"""A complete first-pass Midi4Text mapping for a 9-key-per-hand board.

Each hand: a (pinky), o/s (ring), t/n (middle), e/i (index), Sp/Bk (thumbs),
named as in BIT_NAMES.  Dosh swaps the thumbs relative to Taipo, so Bk is the
key that types Space and Sp the one that types Backspace.  Chords are written
left-hand, hyphen, right-hand; thumb names are two letters so a chord like
`eiSp` reads unambiguously as e + i + Sp.

    outer five  a o s t n     Series 1 (onset) on the left, Series 4 (coda) right
    inner four  e i Sp Bk     Series 2 (second character) left, Series 3 (vowel) right

The groups are positional, not phonetic: the inner four spell vowels only on the
right hand.  On the left the same keys are Series 2, which is mostly consonants.
Read across the board and the four Series come out in spelling order.

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
    Bk is Dosh's Space thumb, so the mnemonic is exact.
"""

import collections

from m4t import corpus, layout as L, theory, writer

# ---------------------------------------------------------------- vowel group

# Seven vowel identities on e/i/B; S added marks the end of a word.
VOWEL_IDENTITY = {
    "e": "e",            # Dosh key
    "i": "i",            # Dosh key
    "a": "Sp",
    "o": "ei",
    "u": "iSp",
    "ea": "eSp",         # = e + a
    "ou": "eiSp",        # = o + u
}
END_MARKER = "Bk"        # the thumb that types Space in Dosh

# Series 3 patterns, as (identity, closes the word).
SERIES3 = {
    "a": ("a", False), "e": ("e", False), "i": ("i", False),
    "ie": ("o", False), "u": ("u", False), "ea": ("ea", False),
    "ua": ("a", True), "ue": ("e", True), "ui": ("i", True),
    "uie": ("o", True), "uia": ("u", True), "iea": ("ea", True),
    "ia": ("ou", True),
}

# Series 2 keeps the vowel chords wherever it means the same vowel, so a chord
# reads the same on either hand.  That covers each pattern's mirrored vowel as
# well as its plain reading: R is r, but also the mirrored a, so it takes a's
# chord.  Two patterns read as o, the plain RXI and the mirrored XI; RXI is
# four times as common and takes o's chord, and XI takes the ending o, which
# fits, since the mirrored form ends the word.
SERIES2_VOWELS = {"R": "Sp", "X": "e", "I": "i", "RXI": "ei", "U": "iSp",
                  "RX": "eSp", "XI": "eiBk"}

# The consonants the vowels leave in place keep their chords, and RI, the
# commonest pure consonant, takes the one single key left.  The rest fill in
# by frequency.
SERIES2_FIXED = {**SERIES2_VOWELS,
                 "RI": "Bk", "RU": "eBk", "XIU": "eiSp", "XU": "eSpBk"}


def series2_map(freq):
    import itertools

    fixed = dict(SERIES2_FIXED)
    used = set(fixed.values())
    chords = []
    for n in range(1, 5):
        for c in itertools.combinations(["e", "i", "Sp", "Bk"], n):
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

    print("OUTER FIVE   a (pinky), o/s (ring), t/n (middle)")
    print("  Consonants on both hands: Series 1 is the onset, Series 4 the coda.\n")
    print(f"  {'onset':>6} {'coda':>6}  {'chord':<7} note")
    for shape, _ in cons.most_common():
        if shape not in cmap:
            continue
        note = "Dosh key" if shape in anchors else ""
        if shape in pairs:
            note = f"= {theory.ONSET[pairs[shape]]} + pinky"
        print(f"  {theory.ONSET.get(shape,'-'):>6} "
              f"{theory.CODA.get(L.mirror(shape),'-'):>6}  {cmap[shape]:<7} {note}")

    print("\nINNER FOUR, right hand -- Series 3, the vowel")
    print("  Seven identities; add Bk, the Space thumb, to end the word.\n")
    print(f"  {'vowel':>6}  {'plain':<7} {'+ space':<8} note")
    seen = set()
    for ident, chord in VOWEL_IDENTITY.items():
        if ident in seen:
            continue
        seen.add(ident)
        note = {"e": "Dosh key", "i": "Dosh key",
                "ea": "= e + a", "ou": "= o + u"}.get(ident, "")
        print(f"  {ident:>6}  {chord:<7} {chord + END_MARKER:<8} {note}")

    print("\nINNER FOUR, left hand -- Series 2, the second character")
    print("  Mostly consonants, not vowels.  Where Series 2 does mean a vowel,")
    print("  plainly or mirrored, it keeps the Series 3 chord, so the vowels")
    print("  read alike on both hands.\n")
    print(f"  {'spells':>6}  {'chord':<7} note")
    for pat, _ in f2.most_common():
        vowel = theory.MIRRORED_VOWEL.get(pat)
        if pat == "RXI":
            note = "same chord as vowel o"
        elif pat == "XI":
            note = "mirrored o: same chord as ending o"
        elif vowel:
            note = f"mirrored {vowel}: same chord as vowel {vowel}"
        else:
            note = ""
        print(f"  {theory.SECOND[pat]:>6}  {s2map[pat]:<7} {note}")


if __name__ == "__main__":
    main()
