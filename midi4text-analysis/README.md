# midi4text-analysis

Tools for reverse-engineering the Midi4Text orthographic steno theory from its
shipped Plover dictionaries.  See `docs/midi4text/` for the findings.

    python3 validate.py --show 3

Regenerates the 158,614-entry dictionary from the recovered theory and reports
what it fails to reproduce.  Reads the dictionaries from `../midi4text/ENG/`,
an untracked checkout of the upstream Sillabix repository.

- `m4t/layout.py`  — the four Series, stroke ordering, the hand mirror
- `m4t/corpus.py`  — dictionary loading
- `m4t/extract.py` — recovers per-Series alphabets by differencing
- `m4t/theory.py`  — the generative model: stroke -> Plover translation
