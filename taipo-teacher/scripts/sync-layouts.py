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


def orsy_patterns(orsy):
    """Every Orsy pattern with what it spells, keyed by Series and Michela name.

    An outer shape is two patterns, its onset reading and its coda reading, since a
    trainer learns them separately.  The rules are keyed by name with their version,
    as a change to them is a change to every stroke that uses them.
    """
    out = {}
    for o in orsy["outer"]:
        if o["onset"] is not None:
            out[f"s1:{o['michela']}"] = o["onset"]
        out[f"s4:{o['michela']}"] = o["coda"]
    for s in orsy["second"]:
        spells = s["spells"]
        if s["mirrored_vowel"] is not None:
            spells += f"|{s['mirrored_vowel']}"
        out[f"s2:{s['michela']}"] = spells
    for v in orsy["vowel"]:
        out[f"s3:{v['michela']}"] = v["text"] + ("+" if v["ends_word"] else "")
    for c in orsy["commands"]:
        out[f"cmd:{c['name']}"] = f"{c['hand']}:{c['bits']}"
    for p in orsy.get("punctuation", []):
        out[f"punct:{p['text']}"] = (
            f"{p['bits']}:{int(p['space_after'])}{int(p['capitalises'])}")
    for r in orsy["rules"]:
        out[f"rule:{r['name']}"] = str(orsy["rules_version"])
    return out


def revision(layouts):
    out = {
        "fingerprint": layouts["fingerprint"],
        "variants": {
            name: {str(c["code"]): signature(c["action"]) for c in variant["chords"]}
            for name, variant in layouts["variants"].items()
        },
    }
    # Orsy is fingerprinted on its own, so its revisions are kept alongside
    # rather than under the variants: a change to one layout must not cost
    # the other its history.
    if "orsy" in layouts:
        out["orsy"] = {
            "fingerprint": layouts["orsy"]["fingerprint"],
            "patterns": orsy_patterns(layouts["orsy"]),
        }
    return out


def main():
    layouts = json.loads(SOURCE.read_text())
    (PACKAGE / "layouts.json").write_text(SOURCE.read_text())

    history = {"revisions": []}
    if HISTORY.exists():
        history = json.loads(HISTORY.read_text())

    fingerprint = layouts["fingerprint"]
    orsy = layouts.get("orsy", {}).get("fingerprint")
    # A revision is the pair: the same Taipo/Dosh tables with different Orsy
    # tables is a new revision, and is what an Orsy-only change looks like.
    if any(r["fingerprint"] == fingerprint and r.get("orsy", {}).get("fingerprint") == orsy
           for r in history["revisions"]):
        print(f"layout {fingerprint} (orsy {orsy}) already recorded; "
              f"{len(history['revisions'])} known")
        return 0

    history["revisions"].append(revision(layouts))
    HISTORY.write_text(json.dumps(history, indent=1, sort_keys=True) + "\n")
    print(f"recorded layout {fingerprint} (orsy {orsy}); {len(history['revisions'])} known")
    return 0


if __name__ == "__main__":
    sys.exit(main())
