"""Find the fewest Midi4Text strokes that spell a given word.

Each dictionary entry contributes a chunk of letters plus a binding: a
`{x^}` entry continues a word, a bare word or a `{^x}` entry ends one.
Finding the best way to write a word is then a shortest-path problem over
the word's letters, which is what the manual's "divide it however the
layout allows" instruction amounts to in practice.
"""

import collections

MEDIAL, FINAL = "medial", "final"


def index(d):
    """Map letter-chunk -> {binding: cheapest stroke} from a dictionary."""
    chunks = collections.defaultdict(dict)
    for stroke, value in d.items():
        if value.startswith("{") and value.endswith("^}"):
            text, binding = value[1:-2], MEDIAL
        elif value.startswith("{^") and value.endswith("}"):
            text, binding = value[2:-1], FINAL
        elif value.startswith("{"):
            continue                       # commands and punctuation
        else:
            text, binding = value, FINAL
        if not text.isalpha():
            continue
        text = text.lower()
        prev = chunks[text].get(binding)
        if prev is None or len(stroke) < len(prev):
            chunks[text][binding] = stroke
    return dict(chunks)


def write(word, chunks, max_chunk=12):
    """Return the shortest stroke list spelling `word`, or None."""
    word = word.lower()
    n = len(word)
    # best[i] = (strokes, path) for the prefix of length i
    best = [None] * (n + 1)
    best[0] = (0, [])
    for i in range(n):
        if best[i] is None:
            continue
        cost, path = best[i]
        for j in range(i + 1, min(n, i + max_chunk) + 1):
            entry = chunks.get(word[i:j])
            if not entry:
                continue
            # The last chunk must close the word; earlier ones must not.
            binding = FINAL if j == n else MEDIAL
            stroke = entry.get(binding)
            if stroke is None:
                continue
            cand = (cost + 1, path + [stroke])
            if best[j] is None or cand[0] < best[j][0]:
                best[j] = cand
    return best[n][1] if best[n] else None
