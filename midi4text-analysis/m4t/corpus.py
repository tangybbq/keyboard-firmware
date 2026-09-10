"""Loading of the four shipped Midi4Text dictionaries."""

import json
import os

DICT_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "..", "midi4text", "ENG"
)

FILES = {
    "main": "Midi4Text main (eng) (1.3.9).json",
    "briefs": "Midi4Text briefs sample (eng) (1.3.7).json",
    "word_parts": "Midi4Text word_parts (eng) (1.3.7).json",
    "punctuation": "Midi4Text punctuation&commands (eng) (1.3.7).json",
}

# The all-keys chord carries the licence string in every dictionary.
LICENCE_STROKE = "FSCZPNRXIUuieanpzcsf"


def load(which="main", drop_licence=True):
    path = os.path.join(DICT_DIR, FILES[which])
    with open(path, encoding="utf-8") as fp:
        d = json.load(fp)
    if drop_licence:
        d.pop(LICENCE_STROKE, None)
    return d
