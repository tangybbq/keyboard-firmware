#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Rank letter n-grams by how many chords a Taipo chord for each would save.

Taipo types one chord per letter.  A chord that types a whole n-gram saves n-1
chords every time it is used, so the value of a candidate n-gram is the number
of chords it takes off the total, summed over a corpus.

The subtlety the note in n-grams.md asks for is that the value of a gram
depends on which other grams already have chords: once `the` has one, most of
what `th` would have saved is already saved.  That falls out of measuring cost
as a segmentation rather than counting occurrences:

    The cost of typing a word is the fewest chords it can be segmented into,
    where each piece is either a single letter or a gram that has a chord.

Benefit is then the drop in total cost, weighted by word frequency, that adding
a gram produces -- which already accounts for nesting (`th` inside `the`) and
for overlap (`the` and `ther` competing in `other`), with no separate
deduction rule.

Grams are counted inside word types, never across a space, because the corpus
is a word-frequency list rather than running text.

`chords` is the other half: which Taipo chords are still free to put those
grams on.

Usage:
    uv run words/ngrams.py rank
    uv run words/ngrams.py rank -n 100 --max-len 6
    uv run words/ngrams.py chords
    uv run words/ngrams.py selftest
"""

import argparse
import sys
import urllib.request
from collections import defaultdict
from pathlib import Path

CORPUS_URL = "https://norvig.com/ngrams/count_1w.txt"
DEFAULT_CORPUS = Path(__file__).with_name("count_1w.txt")

# The Taipo chord table, read as text so that this never needs the firmware to
# build and stays right as the table changes.
DEFAULT_TABLE = (Path(__file__).parent.parent
                 / "bbq-keyboard" / "src" / "layout" / "taipo.rs")

# The chord code is a bitmap.  The finger keys are named for the letter they
# type alone (see TAIPO.md); the two thumbs are above them.
TOP_ROW = [(0x010, "r"), (0x020, "s"), (0x040, "n"), (0x080, "i")]
BOTTOM_ROW = [(0x001, "a"), (0x002, "o"), (0x004, "t"), (0x008, "e")]
FINGERS = 0x0ff
SP, BK = 0x100, 0x200

# Each finger's two keys, ordered pinky to index.
HAND = [("pinky", 0x010, 0x001), ("ring", 0x020, 0x002),
        ("middle", 0x040, 0x004), ("index", 0x080, 0x008)]

# --- The ease model -------------------------------------------------------
#
# These weights are judgement, not measurement.  They are here to be edited:
# what the model is really doing is turning "which fingers, and do they have
# to do different things" into an order, and the exact numbers matter much
# less than the structure they encode.  Lower is easier.

# How willing each finger is.  The pinky is short and weak, the ring has poor
# independence, index and middle do as they are told.
FINGER_COST = {"pinky": 2.3, "ring": 1.6, "middle": 1.05, "index": 1.0}

# One finger pressing both of its keys at once.  Taipo uses this a lot and it
# is comfortable, but it scales with the finger doing it.
SQUEEZE = 1.4

# Two *adjacent* fingers held in different rows -- the one that needs real
# independence.  A finger that is squeezing both its keys covers both rows,
# so it never conflicts with its neighbour.
SPLAY = {("pinky", "ring"): 1.0, ("ring", "middle"): 0.8,
         ("middle", "index"): 0.35}

# An idle finger between two active ones, which has to be held still.
SKIP = {"ring": 0.5}
SKIP_DEFAULT = 0.3

# The four thumb variants of a finger pattern, in the order they are reported.
# Per the layout convention an n-gram chord needs the first two: the bare
# pattern types the gram in lower case, and Sp capitalises its first letter.
VARIANTS = [("bare", 0x000), ("+Sp", SP), ("+Bk", BK), ("+both", SP | BK)]

# Grams shorter than this save nothing; longer than --max-len are not counted.
MIN_LEN = 2


def load_corpus(path, url=CORPUS_URL, quiet=False):
    """Return [(word, count)], downloading the list on first use.

    Only alphabetic words of at least two letters are kept: single letters
    cannot contain a gram, and the few tokens with digits in them are noise.
    """
    if not path.exists():
        if not quiet:
            print(f"downloading {url} -> {path}", file=sys.stderr)
        urllib.request.urlretrieve(url, path)

    words = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            word, _, count = line.partition("\t")
            if len(word) >= MIN_LEN and word.isalpha():
                words.append((word, int(count)))
    return words


def gram_counts(words, max_len):
    """Weighted count of every gram, counting overlapping occurrences."""
    counts = defaultdict(int)
    for word, count in words:
        for n in range(MIN_LEN, max_len + 1):
            for i in range(len(word) - n + 1):
                counts[word[i:i + n]] += count
    return counts


class Ranker:
    """Greedy selection of grams by chords saved.

    `chosen` is the set of grams that have chords; `costs[i]` is the current
    cost of word `i` under that set, kept up to date so that measuring a
    candidate only has to re-segment the words that actually contain it.
    """

    def __init__(self, words, max_len, floor):
        self.words = words
        self.max_len = max_len
        self.chosen = set()
        self.costs = [len(w) for w, _ in words]

        # Total letters, weighted -- the baseline cost, since Taipo types one
        # chord per letter today.  Every benefit is reported against this.
        self.baseline = sum(len(w) * c for w, c in words)

        counts = gram_counts(words, max_len)

        # A gram is used at most `count` times and saves at most len-1 chords
        # each time, so this bounds its benefit for *any* chosen set.  It is
        # what makes both the candidate cut and the per-round scan exact.
        self.bound = {g: n * (len(g) - 1) for g, n in counts.items()}

        cut = floor * self.baseline / 1000.0
        self.floor = floor
        self.candidates = sorted(
            (g for g, b in self.bound.items() if b >= cut),
            key=lambda g: -self.bound[g],
        )

        # Words each candidate appears in.  Adding a gram can only change the
        # cost of these, which is what keeps re-measurement cheap.
        self.index = defaultdict(list)
        cands = set(self.candidates)
        for i, (word, _) in enumerate(words):
            seen = {
                word[j:j + n]
                for n in range(MIN_LEN, max_len + 1)
                for j in range(len(word) - n + 1)
            }
            for g in seen & cands:
                self.index[g].append(i)

    def cost(self, word):
        """Fewest chords `word` segments into under the current chosen set."""
        n = len(word)
        dp = [0] * (n + 1)
        for i in range(1, n + 1):
            best = dp[i - 1] + 1
            for length in range(MIN_LEN, min(self.max_len, i) + 1):
                if word[i - length:i] in self.chosen:
                    if dp[i - length] + 1 < best:
                        best = dp[i - length] + 1
            dp[i] = best
        return dp[n]

    def benefit(self, gram):
        """Chords saved, weighted, by giving `gram` a chord as well."""
        self.chosen.add(gram)
        total = 0
        for i in self.index[gram]:
            word, count = self.words[i]
            total += (self.costs[i] - self.cost(word)) * count
        self.chosen.discard(gram)
        return total

    def take(self, gram):
        """Give `gram` a chord, updating the costs it changes."""
        self.chosen.add(gram)
        for i in self.index[gram]:
            self.costs[i] = self.cost(self.words[i][0])

    def best(self, exhaustive=False):
        """The candidate with the highest benefit, and that benefit.

        Candidates are walked in descending order of their upper bound, so
        once the bound falls to the best benefit found so far, nothing left
        can beat it.  That makes the scan exact, not approximate.

        Note that benefits are *not* monotonically decreasing as grams are
        taken: adding one gram can slightly raise another's benefit by moving
        where a word's best segmentation falls.  That is why this rescans
        rather than keeping a heap of stale benefits -- with a lazy heap those
        stale values stop being upper bounds and the wrong gram gets picked.
        """
        top, winner = -1, None
        for gram in self.candidates:
            if gram in self.chosen:
                continue
            if not exhaustive and self.bound[gram] <= top:
                break
            value = self.benefit(gram)
            if value > top or (value == top and gram < winner):
                top, winner = value, gram
        return winner, top

    def rank(self, count, exhaustive=False):
        """Yield `count` (gram, benefit) pairs, best first."""
        for _ in range(count):
            gram, value = self.best(exhaustive)
            if gram is None:
                return
            self.take(gram)
            yield gram, value


def read_codes(path, table):
    """The chord codes a Rust chord table uses, each with its action kind."""
    import re
    source = path.read_text(encoding="utf-8")
    match = re.search(rf"static {table}[^=]*=\s*&?\[(.*?)\n\];", source, re.S)
    if not match:
        raise SystemExit(f"no table {table} found in {path}")
    return {
        int(code, 16): kind
        for code, kind in re.findall(
            r"code:\s*0x([0-9a-fA-F]+)\s*,\s*action:\s*Action::(\w+)",
            match.group(1))
    }


def finger_states(pattern):
    """Each finger's state in this chord: None, "top", "bottom" or "both"."""
    states = {}
    for name, top, bottom in HAND:
        hit = (bool(pattern & top), bool(pattern & bottom))
        states[name] = {(True, True): "both", (True, False): "top",
                        (False, True): "bottom", (False, False): None}[hit]
    return states


def ease(pattern):
    """How hard this chord is to press.  Lower is easier.

    The cost of a chord is what each finger is asked to do, plus what it is
    asked to do *differently from its neighbour*.  The second part is most of
    what separates chords that use the same fingers.
    """
    states = finger_states(pattern)
    score = 0.0
    for name, state in states.items():
        if state is None:
            continue
        score += FINGER_COST[name] * (SQUEEZE if state == "both" else 1.0)

    for (a, b), penalty in SPLAY.items():
        sa, sb = states[a], states[b]
        if sa is None or sb is None:
            continue
        # A squeezing finger spans both rows, so there is nothing to splay
        # against; otherwise a row disagreement costs.
        if sa != "both" and sb != "both" and sa != sb:
            score += penalty

    order = [name for name, _, _ in HAND]
    active = [i for i, name in enumerate(order) if states[name]]
    if active:
        for i in range(min(active) + 1, max(active)):
            if not states[order[i]]:
                score += SKIP.get(order[i], SKIP_DEFAULT)
    return score


def fingers_used(pattern):
    """Which fingers the chord uses, as initials, pinky to index."""
    states = finger_states(pattern)
    return "".join(name[0] if states[name] else "."
                   for name, _, _ in HAND)


def chord_name(pattern):
    """The keys of a finger pattern, alphabetically -- `0x04c` is `ent`."""
    return "".join(sorted(n for b, n in TOP_ROW + BOTTOM_ROW if pattern & b))


def chord_picture(pattern):
    """The pattern as it sits under the hand, top row over bottom row."""
    rows = ["".join(n if pattern & b else "." for b, n in row)
            for row in (TOP_ROW, BOTTOM_ROW)]
    return "/".join(rows)


def availability(used, code):
    """Whether `code` can take an n-gram: "free", "ngram" or None.

    A code holding an `Action::Text` is holding an n-gram that was assigned by
    this same analysis.  Those are not committed the way a letter or a
    punctuation mark is -- reassigning one is a table edit -- so they stay in
    the pool, marked rather than hidden.
    """
    kind = used.get(code)
    if kind is None:
        return "free"
    return "ngram" if kind == "Text" else None


def cmd_chords(args):
    used = read_codes(args.source, args.table)

    rows = []
    for pattern in range(1, FINGERS + 1):
        state = {name: availability(used, pattern | thumb)
                 for name, thumb in VARIANTS}
        rows.append((pattern, state))

    if args.fingers:
        rows = [r for r in rows if bin(r[0]).count("1") in args.fingers]

    if args.all:
        print(f"{args.table} in {args.source}: {len(used)} entries\n")
        print("code  chord     keys       ease   available")
        for pattern, state in sorted(rows, key=lambda r: ease(r[0])):
            avail = " ".join(f"{n}={v}" for n, v in state.items() if v)
            print(f"0x{pattern:03x} {chord_name(pattern):<9} "
                  f"{chord_picture(pattern):<10} {ease(pattern):>5.2f}  "
                  f"{avail or '-'}")
        return

    # The default view: patterns an n-gram chord could take, meaning the bare
    # code and its +Sp capital are both available.  +Bk and +both are left
    # alone; they are held for punctuation and programming grams later.
    usable = [(p, st) for p, st in rows if st["bare"] and st["+Sp"]]
    usable.sort(key=lambda r: (ease(r[0]), chord_name(r[0])))

    print(f"{args.table} in {args.source}: {len(used)} entries")
    print(f"{len(usable)} of {len(rows)} finger patterns can take an n-gram, "
          f"easiest first")
    print("(* already holds an n-gram, so it is reassignable rather than free)\n")

    print("  ease  code  chord     keys       fingers")
    for pattern, state in usable:
        mark = "*" if "ngram" in (state["bare"], state["+Sp"]) else " "
        print(f"{mark} {ease(pattern):>5.2f}  0x{pattern:03x} "
              f"{chord_name(pattern):<9} {chord_picture(pattern):<10} "
              f"{fingers_used(pattern)}")


def per_thousand(value, baseline):
    """Chords saved per 1000 letters -- the unit everything is reported in."""
    return 1000.0 * value / baseline


def cmd_rank(args):
    words = load_corpus(args.words)
    ranker = Ranker(words, args.max_len, args.floor)

    if not args.quiet:
        print(
            f"{len(words):,} words, {len(ranker.candidates):,} candidates "
            f"(grams of {MIN_LEN}-{args.max_len} letters, "
            f"bound >= {args.floor} per 1000)",
            file=sys.stderr,
        )

    results = list(ranker.rank(args.number))

    if args.json:
        import json
        print(json.dumps({
            "corpus": str(args.words),
            "maxLen": args.max_len,
            "baselineLetters": ranker.baseline,
            "unit": "chords saved per 1000 letters",
            "grams": [
                {"rank": i, "gram": g, "saved": per_thousand(v, ranker.baseline)}
                for i, (g, v) in enumerate(results, 1)
            ],
        }, indent=2))
        return

    # Baseline is one chord per letter, so exactly 1000 per 1000 letters.
    print(f"corpus letters (weighted): {ranker.baseline:,}")
    print("baseline: 1000.00 chords per 1000 letters\n")
    print("rank gram      saved/1000   cost after")
    running = 0
    lowest = None
    for i, (gram, value) in enumerate(results, 1):
        running += value
        saved = per_thousand(value, ranker.baseline)
        lowest = saved
        after = 1000.0 - per_thousand(running, ranker.baseline)
        print(f"{i:>4} {gram:<8} {saved:>10.2f} {after:>12.2f}")

    if lowest is not None and lowest < args.floor:
        print(
            f"\nwarning: the last benefit ({lowest:.2f}) is below the candidate "
            f"floor ({args.floor}), so grams that were cut could outrank it; "
            f"lower --floor to be sure of the tail",
            file=sys.stderr,
        )


def cmd_selftest(args):
    """Check the cost model and the pruned scan against things known by hand."""
    failures = []

    def check(name, got, want):
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    # The DP itself, on a word whose grams overlap.
    r = Ranker([("banana", 1)], max_len=5, floor=0.0)
    check("cost with no chords", r.cost("banana"), 6)
    r.chosen.add("ana")
    # `ana` occurs twice but they overlap, so only one can be used:
    # b + ana + n + a.
    check("cost with ana", r.cost("banana"), 4)
    r.chosen.clear()

    # Benefit counts non-overlapping uses, not occurrences.
    check("benefit of ana", r.benefit("ana"), 2)
    check("benefit with nothing chosen is a real saving", r.benefit("an"), 2)

    # The deduction the note asks for: `the` takes what `th` would have saved.
    r = Ranker([("the", 10)], max_len=5, floor=0.0)
    check("benefit of the", r.benefit("the"), 20)
    check("benefit of th", r.benefit("th"), 10)
    r.take("the")
    check("benefit of th after the", r.benefit("th"), 0)

    # An empty chosen set saves nothing and costs one chord per letter.
    r = Ranker([("hello", 3), ("world", 2)], max_len=5, floor=0.0)
    check("baseline", r.baseline, 5 * 3 + 5 * 2)
    check("costs start at one per letter", r.costs, [5, 5])

    # The pruned scan must pick what an exhaustive scan picks.  Run it on a
    # slice of the real corpus, which has the awkward overlap structure that
    # synthetic words do not.
    words = load_corpus(args.words, quiet=True)[:args.slice]
    pruned = Ranker(words, args.max_len, args.floor)
    full = Ranker(words, args.max_len, args.floor)
    for step in range(args.steps):
        a, va = pruned.best()
        b, vb = full.best(exhaustive=True)
        check(f"pruned scan agrees with exhaustive at step {step + 1}",
              (a, va), (b, vb))
        pruned.take(a)
        full.take(b)

    if failures:
        for line in failures:
            print(f"FAIL {line}")
        sys.exit(1)
    print(f"ok ({args.steps} pruned/exhaustive steps on {len(words):,} words)")


def main():
    p = argparse.ArgumentParser(
        description="Rank letter n-grams by the chords a Taipo chord would save.",
    )
    p.add_argument("-w", "--words", type=Path, default=DEFAULT_CORPUS,
                   help=f"word-frequency list (default: {DEFAULT_CORPUS.name}, "
                        "downloaded on first use)")
    p.add_argument("--max-len", type=int, default=5, metavar="N",
                   help="longest gram to consider (default: 5)")
    p.add_argument("--floor", type=float, default=0.5, metavar="X",
                   help="drop candidates that could not save X chords per 1000 "
                        "letters even at best (default: 0.5)")
    sub = p.add_subparsers(dest="command", required=True)

    rank = sub.add_parser("rank", help="rank grams by chords saved")
    rank.add_argument("-n", "--number", type=int, default=60, metavar="N",
                      help="how many grams to report (default: 60)")
    rank.add_argument("--json", action="store_true", help="emit JSON")
    rank.add_argument("-q", "--quiet", action="store_true",
                      help="no progress on stderr")
    rank.set_defaults(func=cmd_rank)

    chords = sub.add_parser("chords", help="which chords are still free")
    chords.add_argument("-s", "--source", type=Path, default=DEFAULT_TABLE,
                        help=f"Rust file holding the table "
                             f"(default: {DEFAULT_TABLE.name})")
    chords.add_argument("-t", "--table", default="TAIPO_ACTIONS",
                        help="table to read (default: TAIPO_ACTIONS)")
    chords.add_argument("-f", "--fingers", type=int, nargs="*", metavar="N",
                        help="only patterns using this many finger keys")
    chords.add_argument("-a", "--all", action="store_true",
                        help="every pattern, with its free thumb variants")
    chords.set_defaults(func=cmd_chords)

    test = sub.add_parser("selftest", help="check the cost model and the scan")
    test.add_argument("--slice", type=int, default=20000, metavar="N",
                      help="words of the corpus to cross-check on (default: 20000)")
    test.add_argument("--steps", type=int, default=8, metavar="N",
                      help="selection steps to cross-check (default: 8)")
    test.set_defaults(func=cmd_selftest)

    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
