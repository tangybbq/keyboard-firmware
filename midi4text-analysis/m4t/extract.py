"""Recover the per-Series alphabets from the shipped dictionary by differencing.

For each Series we hold the other three components fixed and compare the entry
that uses a pattern against the entry that leaves that Series empty.  The
difference, taken over thousands of contexts, is the pattern's spelling.
"""

import collections

from . import layout as L


def text(value):
    """Strip Plover's affix syntax, returning the bare letters and a form tag."""
    v = value
    if v.startswith("{") and v.endswith("}"):
        v = v[1:-1]
    lead = v.startswith("^")
    trail = v.endswith("^")
    return v.strip("^"), lead, trail


def derive(d, series_index, affix):
    """Majority-vote the spelling contributed by each pattern of one Series.

    `affix` is "prefix" when the Series contributes at the front of the
    syllable (Series 1 and 2) and "suffix" when at the back (Series 4).
    """
    baselines = {}
    for k, v in d.items():
        parts = L.split(k)
        if parts[series_index] == "":
            baselines[parts] = text(v)[0]

    votes = collections.defaultdict(collections.Counter)
    for k, v in d.items():
        parts = L.split(k)
        pat = parts[series_index]
        if pat == "":
            continue
        key = list(parts)
        key[series_index] = ""
        base = baselines.get(tuple(key))
        if base is None:
            continue
        whole = text(v)[0]
        if affix == "prefix":
            if whole.endswith(base):
                votes[pat][whole[: len(whole) - len(base)]] += 1
        else:
            if whole.startswith(base):
                votes[pat][whole[len(base):]] += 1
    return votes


def best(votes):
    """Reduce vote counters to a single spelling plus a confidence figure."""
    out = {}
    for pat, ctr in votes.items():
        spelling, n = ctr.most_common(1)[0]
        out[pat] = (spelling, n, sum(ctr.values()))
    return out
