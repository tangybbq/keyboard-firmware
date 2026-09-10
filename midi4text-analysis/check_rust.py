#!/usr/bin/env python3
"""Check the Rust port of the theory against the Python model.

Two independent paths from key bits to text: the Rust crate looks the bits
up in its own transcription of the mapping and applies its own port of the
rules, while this script maps the bits to a Michela stroke through
orsy-mapping.json and runs the original theory.py on it.  The whole chord
space is only 262,144 strokes, so the comparison is exhaustive.

With --corpus, the stroke counts from the mapping work are re-run against
the Rust output, by building a dictionary from it and writing the 20,000
commonest words with writer.py.
"""

import argparse
import collections
import json
import os
import subprocess
import sys

from m4t import layout as L, theory, writer

HERE = os.path.dirname(os.path.abspath(__file__))
MAPPING = os.path.join(HERE, "..", "docs", "orsy", "orsy-mapping.json")
CRATE = os.path.join(HERE, "..", "bbq-orsy", "Cargo.toml")
WORDS = os.path.join(HERE, "..", "words", "count_1w.txt")

OUTER_MASK = 0x067
INNER_MASK = 0x388
HAND_MASK = OUTER_MASK | INNER_MASK


def load_mapping():
    """Per-Series maps from key bits to Michela pattern."""
    with open(MAPPING) as fp:
        m = json.load(fp)
    assert m["outer_mask"] == OUTER_MASK and m["inner_mask"] == INNER_MASK
    s1 = {e["bits"]: e["michela"] for e in m["outer"]}
    s4 = {e["bits"]: L.mirror(e["michela"]) for e in m["outer"]}
    s2 = {e["bits"]: e["michela"] for e in m["inner_left"]}
    s3 = {e["bits"]: e["michela"] for e in m["inner_right"]}
    for s in (s1, s2, s3, s4):
        s[0] = ""
    return s1, s2, s3, s4


def plover_to_flags(value):
    """A Plover translation as (text, space_before, space_after)."""
    if value == "{^}":
        # Ambiguous: both fragment forms collapse to this when the word is
        # empty.  An empty word has no onset, and the only way a stroke with
        # no onset leans forward is a bare final y, which is not empty; so
        # an empty fragment always leans back.
        return "", False, True
    before = not value.startswith("{^")
    after = not value.endswith("^}")
    text = value
    if text.startswith("{"):
        text = text[1:-1]
    text = text.removeprefix("^").removesuffix("^")
    return text, before, after


def expected(maps):
    """What theory.py says for every chord: {(left, right): (text, before, after)}."""
    s1, s2, s3, s4 = maps
    out = {}
    for left in range(0x400):
        for right in range(0x400):
            if left == 0 and right == 0:
                continue
            if (left | right) & ~HAND_MASK:
                continue
            try:
                stroke = (s1[left & OUTER_MASK] + s2[left & INNER_MASK]
                          + s3[right & INNER_MASK] + s4[right & OUTER_MASK])
            except KeyError:
                continue
            try:
                value = theory.translate(stroke)
            except theory.Untranslatable:
                continue
            out[(left, right)] = plover_to_flags(value)
    return out


def rust_dump(path):
    if path is None:
        proc = subprocess.run(
            ["cargo", "run", "--quiet", "--release", "--manifest-path", CRATE,
             "--example", "dump"],
            check=True, capture_output=True, text=True)
        lines = proc.stdout.splitlines()
    else:
        with open(path, encoding="utf-8") as fp:
            lines = fp.read().splitlines()
    out = {}
    for line in lines:
        left, right, before, after, text = line.split(" ", 4)
        out[(int(left, 16), int(right, 16))] = (text, before == "1", after == "1")
    return out


def compare(want, got, show):
    missing = sorted(set(want) - set(got))
    extra = sorted(set(got) - set(want))
    differ = sorted(k for k in want if k in got and want[k] != got[k])
    print(f"python translates {len(want)} chords, rust {len(got)}")
    print(f"  missing from rust {len(missing)}, extra in rust {len(extra)}, "
          f"differing {len(differ)}")
    for name, keys in (("missing", missing), ("extra", extra), ("differ", differ)):
        for left, right in keys[:show]:
            print(f"    {name} {left:03x} {right:03x}: "
                  f"python {want.get((left, right))!r} rust {got.get((left, right))!r}")
    return not (missing or extra or differ)


def corpus(got):
    """Strokes and keys per word over the commonest 20,000 words."""
    # writer.index keeps the first of equally long stroke names for a chunk,
    # so feed it the chords fewest keys first, the way the original preferred
    # the shortest Michela stroke.
    d = {}
    by_keys = sorted(got.items(),
                     key=lambda kv: bin(kv[0][0]).count("1") + bin(kv[0][1]).count("1"))
    for (left, right), (text, before, after) in by_keys:
        key = f"{left:03x}-{right:03x}"
        if before and after:
            d[key] = text
        elif before:
            d[key] = "{" + text + "^}"
        else:
            d[key] = "{^" + text + "}"
    chunks = writer.index(d)
    words = []
    for line in open(WORDS, encoding="utf-8", errors="replace"):
        p = line.split()
        if len(p) == 2 and p[0].isalpha():
            words.append((p[0], int(p[1])))
    words = words[:20000]
    total = strokes = keys = 0
    unwritable = 0
    big = collections.Counter()
    for w, f in words:
        path = writer.write(w, chunks)
        if path is None:
            unwritable += f
            continue
        total += f
        strokes += f * len(path)
        for st in path:
            left, right = (int(x, 16) for x in st.split("-"))
            n = bin(left).count("1") + bin(right).count("1")
            keys += f * n
            big[n >= 7] += f
    print(f"words written {len(words)} ({unwritable / (total + unwritable):.2%} of "
          f"occurrences unwritable)")
    print(f"  strokes/word {strokes / total:.2f}")
    print(f"  keys/word    {keys / total:.2f}")
    print(f"  keys/stroke  {keys / strokes:.2f}")
    print(f"  strokes of 7+ keys {big[True] / (big[True] + big[False]):.1%}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dump", help="a saved dump, instead of running cargo")
    ap.add_argument("--show", type=int, default=10, help="examples per class")
    ap.add_argument("--corpus", action="store_true", help="re-run the stroke counts")
    args = ap.parse_args()

    got = rust_dump(args.dump)
    want = expected(load_mapping())
    ok = compare(want, got, args.show)
    if args.corpus:
        corpus(got)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
