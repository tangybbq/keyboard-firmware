"""Midi4Text keyboard layout: the four Series and the canonical stroke order."""

SERIES1 = "FSCZPN"   # left, white keys  -- initial characters
SERIES2 = "RXIU"     # left, black keys  -- subsequent characters / mirrored vowels
SERIES3 = "uiea"     # right, black keys -- vowels
SERIES4 = "npzcsf"   # right, white keys -- final characters

SERIES = (SERIES1, SERIES2, SERIES3, SERIES4)
ORDER = SERIES1 + SERIES2 + SERIES3 + SERIES4

_ORDER_INDEX = {k: i for i, k in enumerate(ORDER)}

# Positional mirror between the hands: series 1<->4 and series 2<->3.  The
# reflection reverses key order within a Series, since the two hands face
# outwards from the middle of the keyboard.
MIRROR = dict(zip(SERIES1, reversed(SERIES4)))
MIRROR.update(zip(SERIES2, reversed(SERIES3)))
MIRROR.update({v: k for k, v in MIRROR.items()})


def split(stroke):
    """Split a stroke into its four Series components, in order."""
    return tuple("".join(c for c in stroke if c in s) for s in SERIES)


def mirror(pattern):
    """Reflect a single-Series pattern onto the opposite hand."""
    return normalize("".join(MIRROR[c] for c in pattern))


def normalize(keys):
    """Sort an iterable of keys into canonical stroke order."""
    return "".join(sorted(set(keys), key=_ORDER_INDEX.__getitem__))


def is_valid(stroke):
    return all(c in _ORDER_INDEX for c in stroke) and normalize(stroke) == stroke
