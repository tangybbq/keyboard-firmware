#!/usr/bin/env python3
"""Regenerate the Midi4Text dictionary from the theory and report coverage.

The theory in m4t/theory.py is roughly 110 table rows and a dozen composition
rules.  If it reproduces the shipped 158,614-entry dictionary, the theory is
right and the dictionary is redundant.  Whatever it fails to reproduce is the
errata list, printed here by class.
"""

import argparse
import collections
import sys

from m4t import corpus, layout as L, theory


def classify(stroke, want, got):
    s1, s2, s3, s4 = L.split(stroke)
    if s2 == "IU" and s4 and not s3:
        return "number bar"
    if s3 == "ia" and s4:
        return "'ia' + coda: dictionary alternates ou/ea"
    if s4 in ("cf", "zc") and not s3:
        return "coda cf/zc spelt as its Series 1 mirror"
    if s4 == "zcs":
        return "capitalisation placement"
    if got is None:
        return "untranslatable"
    return "unclassified"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--show", type=int, default=0,
                    help="print this many example failures per class")
    args = ap.parse_args()

    d = corpus.load()
    ok = 0
    classes = collections.Counter()
    examples = collections.defaultdict(list)
    for stroke, want in d.items():
        try:
            got = theory.translate(stroke)
        except theory.Untranslatable:
            got = None
        if got == want:
            ok += 1
            continue
        cls = classify(stroke, want, got)
        classes[cls] += 1
        if len(examples[cls]) < args.show:
            examples[cls].append((stroke, want, got))

    total = len(d)
    print(f"entries      {total}")
    print(f"reproduced   {ok}  ({ok / total:.2%})")
    print(f"residue      {total - ok}")
    print()
    for cls, n in classes.most_common():
        print(f"  {n:6}  {cls}")
        for stroke, want, got in examples[cls]:
            print(f"            {stroke:16} want {want!r:20} got {got!r}")
    return 0 if ok / total > 0.98 else 1


if __name__ == "__main__":
    sys.exit(main())
