#!/usr/bin/env python3
"""Copy the generated chord tables into the Swift package, and remember this revision.

Run this instead of copying `layouts.json` by hand, and run it *before* changing the
tables as well as after.  The history it keeps is what lets logs written against an older
layout still be read: a revision that was never recorded cannot be interpreted later, and
its sessions are skipped whole rather than half-understood.

Nothing here is clever.  The signature of a chord is its action written out, so a diff
between two revisions is exactly the set of chords whose meaning changed, and the file
stays something a person can read.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "bbq-keyboard" / "layouts.json"
PACKAGE = ROOT / "taipo-teacher" / "Sources" / "TaipoKit"
HISTORY = PACKAGE / "layout-history.json"


def signature(action):
    """What a chord does, as one comparable string."""
    kind = action["kind"]
    detail = (
        action.get("key")
        or action.get("text")
        or "+".join(action.get("mods") or [])
        or ""
    )
    return f"{kind}:{detail}" if detail else kind


def revision(layouts):
    return {
        "fingerprint": layouts["fingerprint"],
        "variants": {
            name: {str(c["code"]): signature(c["action"]) for c in variant["chords"]}
            for name, variant in layouts["variants"].items()
        },
    }


def main():
    layouts = json.loads(SOURCE.read_text())
    (PACKAGE / "layouts.json").write_text(SOURCE.read_text())

    history = {"revisions": []}
    if HISTORY.exists():
        history = json.loads(HISTORY.read_text())

    fingerprint = layouts["fingerprint"]
    if any(r["fingerprint"] == fingerprint for r in history["revisions"]):
        print(f"layout {fingerprint} already recorded; {len(history['revisions'])} known")
        return 0

    history["revisions"].append(revision(layouts))
    HISTORY.write_text(json.dumps(history, indent=1, sort_keys=True) + "\n")
    print(f"recorded layout {fingerprint}; {len(history['revisions'])} known")
    return 0


if __name__ == "__main__":
    sys.exit(main())
