"""A generative model of Midi4Text: stroke -> Plover translation.

The tables here were recovered from the shipped dictionary by differencing
(see extract.py) and agree with the manual's own tables.  The point of the
module is that it is small: roughly a hundred table rows and a handful of
composition rules stand in for 158,614 dictionary entries.
"""

from . import layout as L

# Series 1 -- initial characters (syllable onset).
ONSET = {
    "": "",
    "F": "f", "S": "s", "C": "sh", "Z": "z", "P": "p", "N": "n",
    "FC": "h", "SC": "v", "FZ": "th", "SZ": "k", "CZ": "ck",
    "FP": "t", "SP": "ch", "CP": "c", "ZP": "g",
    "FN": "ind", "SN": "inc", "CN": "w", "ZN": "y",
    "FCP": "b", "SCP": "d", "FZP": "gh", "SZP": "m",
    "FCN": "r", "SCN": "l", "FZN": "int", "SZN": "x",
}

# Series 4 -- final characters (syllable coda).
CODA = {
    "": "",
    "f": "f", "s": "s", "c": "sh", "z": "z", "p": "p", "n": "n",
    "cf": "st", "cs": "v", "zf": "th", "zs": "k", "zc": "ck",
    "pf": "t", "ps": "ch", "pc": "c", "pz": "g",
    "nf": "nd", "ns": "ng", "nc": "w", "nz": "y",
    "pcf": "b", "pcs": "d", "pzf": "gh", "pzs": "m",
    "ncf": "r", "ncs": "l", "nzf": "nt", "nzs": "x",
    "zcf": "h", "pzc": "e", "zcs": "",
}

# Series 2 -- subsequent characters, read as consonants.
SECOND = {
    "": "",
    "R": "r", "X": "s", "I": "i", "U": "u",
    "RI": "l", "XI": "w", "RU": "m", "XU": "n", "IU": "p",
    "RIU": "t", "XIU": "c",
    "RX": "e", "RXI": "o",
}

# Series 2 -- the same patterns read as "mirrored" vowels, which fire only
# when Series 3 is empty and a real coda is present.  Using one appends a
# silent E and the word-final space.
MIRRORED_VOWEL = {"R": "a", "X": "e", "I": "i", "XI": "o", "U": "u", "RX": "ea"}

# Series 3 -- vowels.  The "ending" forms additionally close the word.
VOWEL = {
    "": "",
    "a": "a", "e": "e", "i": "i", "u": "u", "ie": "o",
    "ua": "a", "ue": "e", "ui": "i", "uia": "u", "uie": "o",
    "ea": "°", "ia": "*", "iea": "_",
}
ENDING_VOWELS = {"ua", "ue", "ui", "uia", "uie", "iea", "ia"}

# Inter-series overrides: an onset that a Series 2 pattern rewrites wholesale.
ONSET_OVERRIDE = {
    ("FC", "R"): "str", ("FC", "RI"): "spl", ("FC", "IU"): "spr",
    ("FC", "XIU"): "scr", ("C", "XIU"): "sch", ("Z", "XIU"): "sk",
    ("S", "X"): "sci", ("ZN", "I"): "j", ("CP", "XIU"): "qu",
}

# Series 4 patterns that are commands or digraph tails rather than consonants,
# and so do not license the mirrored-vowel reading.
EXTRA_ORDINEM = {"zc", "zcs"}

CAPITALIZE = "zcs"


class Untranslatable(Exception):
    pass


# Series 2 patterns that are always vowels, never consonant material.
VOWEL_ONLY_SECOND = {"RX", "RXI"}

# Series 3 free combinations.  Struck alone they emit a placeholder glyph and
# close the word; struck with Series 2 they spell the digraph that Series 2
# cannot reach on its own.  The flags are (alone, with-S2, with-S2 closes).
FREE = {
    "ea": ("\u00b0", "ea", False),
    "iea": ("_", "ea", True),
    "ia": ("*", "ou", True),
}

# Inter-series 2nd+3rd: a Series 2 vowel fused with the Series 3 vowel.
DIPHTHONG = {("U", "u"): "au", ("U", "uia"): "au",
             ("I", "i"): "ai", ("I", "ui"): "ai"}

# Series 2 'XI' spells H rather than W after these onset letters.
XI_IS_H_AFTER = ("p", "w", "r")


def _nucleus(s1, s2, s3, s4):
    """Return (nucleus, silent_e, closes_word, series2_consumed)."""
    if s3 in FREE:
        alone, joined, closes = FREE[s3]
        if s2 == "":
            return alone, False, True, False
        return joined, False, closes, False
    if (s2, s3) in DIPHTHONG:
        return DIPHTHONG[(s2, s3)], False, s3 in ENDING_VOWELS, True
    if s3 != "":
        return VOWEL[s3], False, s3 in ENDING_VOWELS, False
    # No Series 3.  Series 2 may supply the nucleus instead.
    if s2 == "RX":
        return MIRRORED_VOWEL["RX"], True, True, True
    if s2 == "RXI":
        return SECOND["RXI"], False, False, True
    if s2 in MIRRORED_VOWEL and s4 and s4 not in EXTRA_ORDINEM:
        return MIRRORED_VOWEL[s2], True, True, True
    return "", False, False, False


def translate(stroke):
    """Render a stroke the way the dictionary does, as a Plover value."""
    s1, s2, s3, s4 = L.split(stroke)
    for part, table in ((s1, ONSET), (s2, SECOND), (s3, VOWEL), (s4, CODA)):
        if part not in table:
            raise Untranslatable(f"unknown pattern {part!r} in {stroke!r}")

    nucleus, silent_e, closes, consumed = _nucleus(s1, s2, s3, s4)
    # An onset cluster like FC+R = "str" needs Series 2 as a consonant; when
    # Series 2 has been taken for the nucleus instead, the cluster is off.
    override = None if consumed else ONSET_OVERRIDE.get((s1, s2))
    if override is not None:
        onset, middle = override, ""
    else:
        onset = ONSET[s1]
        if s1 == "FN" and consumed:
            onset = "gn"
        middle = "" if consumed else SECOND[s2]
        if s2 == "XI" and not consumed:
            middle = "h" if onset.endswith(XI_IS_H_AFTER) else "w"

    # A bare final Y takes its word-final space from the 'ui' keys, which then
    # spell nothing themselves (manual, lesson X).
    if s3 == "ui" and s4 == "nz":
        nucleus = ""

    coda = "" if s4 == CAPITALIZE else CODA[s4]
    body = onset + middle + nucleus
    if s4 == CAPITALIZE:
        body = body.capitalize()
    word = body + coda + ("e" if silent_e else "")

    if closes:
        return word
    if s3 == "":
        # No Series 3 vowel: a fragment.  A bare onset leans forward, anything
        # carrying Series 2 or a coda leans back onto the preceding syllable.
        leans_forward = (s1 and not s2 and not s4) or s4 == "nz"
        return "{" + word + "^}" if leans_forward else "{^" + word + "}"
    return "{" + word + "^}"
