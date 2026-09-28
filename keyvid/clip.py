"""A clip written by `keyvid-extract`, and what it shows at any moment.

Everything here is in the log's own time, in seconds from the start of the
clip.  The scene turns that into video time by the playback speed, so a clip
can be shown slowed down without anything here knowing.
"""

import json
from bisect import bisect_right
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Event:
    """A chord or stroke the engine committed, or a change of mode."""

    t: float
    kind: str
    mode: str
    side: str | None
    left: int
    right: int
    first_key: float
    last_key: float
    spell: str
    label: str
    dead: bool
    text: str


class Clip:
    def __init__(self, path: str | Path):
        data = json.loads(Path(path).read_text())
        self.start_mode: str = data["start_mode"] or "dosh"
        self.text_before: str = data["text_before"]
        self.duration = (data["to_ms"] - data["from_ms"]) / 1000

        # Each key's presses, as (down, up) intervals.  A key still held at
        # the end of the clip is held to the end; one released at the start
        # was pressed before it and is ignored.
        self.presses: dict[str, list[tuple[float, float]]] = {}
        held: dict[str, float] = {}
        for k in data["keys"]:
            t = k["t"] / 1000
            if k["down"]:
                held[k["key"]] = t
            elif k["key"] in held:
                self.presses.setdefault(k["key"], []).append((held.pop(k["key"]), t))
        for key, t in held.items():
            self.presses.setdefault(key, []).append((t, self.duration))
        for intervals in self.presses.values():
            intervals.sort()

        self.events = [
            Event(
                t=e["t"] / 1000,
                kind=e["kind"],
                mode=e["mode"],
                side=e.get("side"),
                left=e["left"],
                right=e["right"],
                first_key=e["first_key"] / 1000,
                last_key=e["last_key"] / 1000,
                spell=e["spell"],
                label=e["label"],
                dead=e["dead"],
                text=e["text"],
            )
            for e in data["events"]
        ]
        self._times = [e.t for e in self.events]

    def is_down(self, key: str, t: float, window: float = 0.0) -> bool:
        """Whether `key` is held at any point in `(t - window, t]`.

        The window is one frame: a press shorter than a frame would otherwise
        fall between two of them and never be drawn, and a quick tap is
        exactly what a video about typing should not lose.
        """
        for down, up in self.presses.get(key, ()):
            if down > t:
                break
            if up > t - window:
                return True
        return False

    def last_event(self, t: float) -> Event | None:
        i = bisect_right(self._times, t)
        return self.events[i - 1] if i else None

    def mode_at(self, t: float) -> str:
        e = self.last_event(t)
        return e.mode if e else self.start_mode

    def text_at(self, t: float) -> str:
        """Everything typed since the clip started, as of `t`."""
        e = self.last_event(t)
        return e.text if e else ""

    def strokes(self) -> list[Event]:
        """The chords and strokes, without the mode changes."""
        return [e for e in self.events if e.kind != "mode"]
