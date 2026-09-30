"""Animate a clip of chorded typing: the two hands, the chords, the text.

    KEYVID_CLIP=clips/orsy.json KEYVID_SPEED=0.5 uv run manim -pqm scenes.py Replay
    KEYVID_CLIP=clips/orsy.json KEYVID_FLOW=scroll uv run manim -pqm scenes.py Stream

`Replay` is one large board with the keys going down and up.  `Stream` is a
list of small boards, one per chord or stroke, each with what it typed beside
it, in two columns that take turns (`KEYVID_FLOW=columns`) or one that
scrolls (`KEYVID_FLOW=scroll`).

The board is drawn the way TaipoTeacher's stroke hint draws it: each hand's
four finger columns in two rows, the thumbs in a row below under the index
side, the right hand the mirror of the left.  No column stagger.

The whole clip is one animation of a clock through the log's time, and every
frame is worked out from the clock, so the keys move with the real timings --
slowed by `KEYVID_SPEED` -- rather than on a beat of manim's choosing.
"""

import os
import re
import sys
from pathlib import Path

from manim import (
    DOWN,
    LEFT,
    RIGHT,
    UP,
    ORIGIN,
    Group,
    Rectangle,
    RoundedRectangle,
    Scene,
    Text,
    ValueTracker,
    VGroup,
    config,
    linear,
    smooth,
)

sys.path.insert(0, str(Path(__file__).parent))
from clip import Clip, Event  # noqa: E402

BACKGROUND = "#15171c"
KEY_IDLE = "#2a2e38"
KEY_EDGE = "#4a5060"
KEY_NAME = "#8a90a0"
TEXT = "#e8e8e8"
DIM = "#707684"
# A chord in Dosh or Taipo is one colour: it is one letter.
CHORD = "#f5b942"
DEAD = "#e5484d"
# Orsy's four Series, as the teacher colours them: onset blue, second teal,
# vowel orange, coda purple -- left to right, the order the letters come out.
ONSET = "#4c8dff"
SECOND = "#2ec4b6"
VOWEL = "#ff9f1c"
CODA = "#b388ff"

MONO = "Menlo"
SANS = "Helvetica Neue"

KEY = 0.9
GAP = 0.14
# Between the two hands.
SPLIT = 1.3

# The left hand, row by row, pinky on the outside; None is empty board.  The
# upper pinky key only Taipo used is gone from the boards, and from here.
LEFT_HAND = [
    [None, "s", "n", "i"],
    ["a", "o", "t", "e"],
    [None, None, "Sp", "Bk"],
]
BITS = {"a": 1, "o": 2, "t": 4, "e": 8, "s": 32, "n": 64, "i": 128, "Sp": 256, "Bk": 512}
ORSY_OUTER = 0x067

# The firmware names the thumbs for what they do in Taipo.  In Dosh and Orsy
# the one it calls `Bk` types the space and `Sp` the backspace, so the screen
# swaps them.  The clips keep the firmware's names, which the logs use.
SHOWN = {"Sp": "Bk", "Bk": "Sp"}

MODE_NAMES = {"dosh": "Dosh", "orsy": "Orsy", "taipo": "Taipo"}

# How long a chord's label is on screen, in video seconds.
LABEL_LIFE = 0.9
# How long the keys of a committed chord flash.
FLASH = 0.25
# The typed text's margin from the sides of the frame.
LINE_MARGIN = 0.8


def shown_spell(spell: str) -> str:
    """A chord's keys, `o+Bk` or `a+o-e+Sp`, with the thumbs named as shown."""
    return re.sub(r"[A-Za-z]+", lambda m: SHOWN.get(m.group(), m.group()), spell)


def press_colour(mode: str, side: str, name: str) -> str:
    if mode != "orsy":
        return CHORD
    outer = BITS[name] & ORSY_OUTER
    if side == "L":
        return ONSET if outer else SECOND
    return CODA if outer else VOWEL


class Key(VGroup):
    def __init__(self, side: str, name: str):
        super().__init__()
        self.side = side
        self.name_ = name
        self.id = f"{side}.{name}"
        self.cap = RoundedRectangle(
            width=KEY, height=KEY, corner_radius=0.12,
            fill_color=KEY_IDLE, fill_opacity=1, stroke_color=KEY_EDGE, stroke_width=2,
        )
        self.legend = Text(SHOWN.get(name, name), font=SANS, font_size=22, color=KEY_NAME)
        self.legend.move_to(self.cap)
        self.add(self.cap, self.legend)

    def show(self, mode: str, down: bool, flash: float, dead: bool):
        """Draw the key held or not, with `flash` (0..1) of a commit on it."""
        if down:
            colour = press_colour(mode, self.side, self.name_)
            self.cap.set_fill(colour, opacity=1)
            self.legend.set_color(BACKGROUND)
        else:
            self.cap.set_fill(KEY_IDLE, opacity=1)
            self.legend.set_color(KEY_NAME)
        if flash > 0:
            edge = DEAD if dead else "#ffffff"
            self.cap.set_stroke(edge, width=2 + 6 * flash, opacity=1)
        else:
            self.cap.set_stroke(KEY_EDGE, width=2, opacity=1)


def build_hand(side: str) -> tuple[VGroup, dict[str, Key]]:
    keys = {}
    hand = VGroup()
    for r, row in enumerate(LEFT_HAND):
        cells = row if side == "L" else list(reversed(row))
        for c, name in enumerate(cells):
            if name is None:
                continue
            key = Key(side, name)
            key.move_to(RIGHT * c * (KEY + GAP) + DOWN * r * (KEY + GAP))
            # The thumbs sit a little apart from the fingers.
            if r == 2:
                key.shift(DOWN * GAP)
            keys[key.id] = key
            hand.add(key)
    return hand, keys


class Label(VGroup):
    """What a chord typed, popping up over the hand that typed it."""

    def __init__(self, event: Event, anchor, colour: str):
        super().__init__()
        self.event = event
        text = event.label.replace("\n", "⏎") or " "
        self.word = Text(text, font=MONO, font_size=56, color=colour)
        # The keys, unless they only repeat the letter.
        spell = shown_spell(event.spell)
        spell = "" if spell == event.label else spell
        self.spell = Text(spell or " ", font=SANS, font_size=20, color=DIM)
        self.spell.next_to(self.word, DOWN, buff=0.15)
        self.add(self.word, self.spell)
        self.anchor = anchor
        self.move_to(anchor)
        self.scale_now = 1.0
        # Set once the next label in the same place is known.
        self.until: float | None = None

    def show(self, age: float):
        """Place the label for `age` video seconds after its chord."""
        life = LABEL_LIFE if self.until is None else min(LABEL_LIFE, self.until + 0.12)
        if age < 0 or age > life:
            self.set_opacity(0)
            return
        pop = min(1.0, age / 0.1)
        fade = 1.0 if age < life - 0.35 else max(0.0, (life - age) / 0.35)
        opacity = pop * fade
        scale = 1.25 - 0.25 * pop
        self.scale(scale / self.scale_now)
        self.scale_now = scale
        self.move_to(self.anchor + UP * 0.35 * (age / LABEL_LIFE))
        self.word.set_opacity(opacity)
        self.spell.set_opacity(opacity * 0.9)


class TypedLine(Group):
    """The last line of the typed text, in a fixed place, with a cursor.

    The text is drawn between two invisible bars, so its box does not move
    with its first and last characters: a leading or trailing space, or a
    line of all x-height letters, would otherwise shift it about.
    """

    def __init__(self, left_edge, width: float):
        super().__init__()
        self.left_edge = left_edge
        # As many characters as fit, measured rather than guessed: the font is
        # monospaced, and the invisible bars are part of the width too.
        bars = Text("||", font=MONO, font_size=40).width
        per_char = (Text("|" + "M" * 20 + "|", font=MONO, font_size=40).width - bars) / 20
        self.chars = int((width - bars) / per_char)
        self.cache: dict[str, Text] = {}
        self.current: Text | None = None
        self.cursor = Rectangle(width=0.05, height=0.55, fill_color=TEXT, fill_opacity=1, stroke_width=0)
        self.add(self.cursor)
        self.set_text("")

    def _render(self, s: str) -> Text:
        if s not in self.cache:
            t = Text("|" + s + "|", font=MONO, font_size=40, color=TEXT)
            t[0].set_opacity(0)
            t[-1].set_opacity(0)
            t.move_to(self.left_edge, aligned_edge=LEFT)
            self.cache[s] = t
        return self.cache[s]

    def set_text(self, text: str):
        line = text.split("\n")[-1]
        if len(line) > self.chars:
            line = line[-self.chars:]
        t = self._render(line)
        if t is not self.current:
            if self.current is not None:
                self.remove(self.current)
            self.add(t)
            self.current = t
        self.cursor.move_to(t[-1].get_center())


def load_clip() -> tuple[Clip, float]:
    """The clip named by `KEYVID_CLIP`, and the speed to play it at."""
    clip_path = os.environ.get("KEYVID_CLIP")
    if not clip_path:
        raise SystemExit("set KEYVID_CLIP to a clip written by keyvid-extract")
    return Clip(clip_path), float(os.environ.get("KEYVID_SPEED", "1"))


def corner_badges(scene: Scene, speed: float) -> dict[str, Text]:
    """Add the playback speed, if it is not 1, and return a badge per mode.

    The badges go in the top left, all at once, and the caller shows the one
    for the mode of the moment.
    """
    if speed != 1:
        rate = Text(f"×{speed:g}", font=SANS, font_size=26, color=DIM)
        rate.to_corner(UP + RIGHT, buff=0.5)
        scene.add(rate)
    return {
        m: Text(n, font=SANS, font_size=30, color=DIM).to_corner(UP + LEFT, buff=0.5)
        for m, n in MODE_NAMES.items()
    }


class Replay(Scene):
    def construct(self):
        clip, speed = load_clip()
        self.camera.background_color = BACKGROUND

        left, left_keys = build_hand("L")
        right, right_keys = build_hand("R")
        left.move_to(LEFT * (left.width / 2 + SPLIT / 2))
        right.move_to(RIGHT * (right.width / 2 + SPLIT / 2))
        board = VGroup(left, right).shift(DOWN * 0.3)
        keys = {**left_keys, **right_keys}

        badges = corner_badges(self, speed)

        # Labels go over the hand that typed them, or over the middle for an
        # Orsy stroke, which is both.
        label_y = board.get_top()[1] + 1.0
        anchors = {
            "L": left.get_center()[0] * RIGHT + label_y * UP,
            "R": right.get_center()[0] * RIGHT + label_y * UP,
            None: label_y * UP + ORIGIN,
        }
        labels = []
        last_in_place: dict[str | None, Label] = {}
        for e in clip.strokes():
            colour = DEAD if e.dead else TEXT
            label = Label(e, anchors[e.side], colour)
            label.set_opacity(0)
            prev = last_in_place.get(e.side)
            if prev is not None:
                prev.until = (e.t - prev.event.t) / speed
            last_in_place[e.side] = label
            labels.append(label)

        line_left = -config.frame_width / 2 + LINE_MARGIN
        line = TypedLine(
            line_left * RIGHT + (board.get_bottom()[1] - 1.0) * UP,
            config.frame_width - 2 * LINE_MARGIN,
        )

        clock = ValueTracker(0)
        window = speed / config.frame_rate
        strokes = clip.strokes()

        def update(_):
            t = clock.get_value()
            mode = clip.mode_at(t)
            # Which keys a chord or stroke committed recently.
            flashes: dict[str, tuple[float, bool]] = {}
            for e in strokes:
                age = (t - e.t) / speed
                if 0 <= age < FLASH:
                    for side, bits in (("L", e.left), ("R", e.right)):
                        for name, bit in BITS.items():
                            if bits & bit:
                                flashes[f"{side}.{name}"] = (1 - age / FLASH, e.dead)
            for key in keys.values():
                flash, dead = flashes.get(key.id, (0.0, False))
                key.show(mode, clip.is_down(key.id, t, window), flash, dead)
            for label in labels:
                label.show((t - label.event.t) / speed)
            line.set_text(clip.text_at(t))
            for m, badge in badges.items():
                badge.set_opacity(1 if m == mode else 0)

        # Everything that changes is in one group with the updater on it.
        # Manim draws a mobject with no updater of its own, outside the
        # animation, once into a static background -- which would freeze the
        # whole board.
        screen = Group(board, *labels, line, *badges.values())
        screen.add_updater(update)
        self.add(screen)
        update(None)
        self.wait(0.5)
        self.play(
            clock.animate.set_value(clip.duration),
            run_time=clip.duration / speed,
            rate_func=linear,
        )
        self.wait(1)


# The small boards of `Stream`, about the teacher's stroke hint beside its
# text.
MINI_KEY = 0.22
MINI_GAP = 0.06
# Between a small board and what it typed, and between one entry and the next.
MINI_LABEL_BUFF = 0.35
ENTRY_GAP = 0.3
# The entries' part of the frame, between the badges and the typed line.
STREAM_TOP = 2.9
STREAM_BOTTOM = -2.6
# How long, in video seconds, a new entry takes to scroll in, and an old
# column to clear.
SCROLL = 0.18
CLEAR = 0.15


class Glyph(VGroup):
    """A chord or stroke on a small board of both hands, as the teacher hints it.

    Only the chord's own keys are ever lit, so one pressed early for the next
    chord does not show up on this one.
    """

    def __init__(self, event: Event):
        super().__init__()
        self.caps: dict[str, RoundedRectangle] = {}
        self.colours: dict[str, str] = {}
        self.legends: dict[str, Text] = {}
        hand_width = 4 * MINI_KEY + 3 * MINI_GAP
        for side, bits in (("L", event.left), ("R", event.right)):
            x0 = 0 if side == "L" else hand_width + 5 * MINI_GAP
            for r, row in enumerate(LEFT_HAND):
                cells = row if side == "L" else list(reversed(row))
                for c, name in enumerate(cells):
                    if name is None:
                        continue
                    cap = RoundedRectangle(
                        width=MINI_KEY, height=MINI_KEY, corner_radius=0.04,
                        fill_color=KEY_IDLE, fill_opacity=1, stroke_width=0,
                    )
                    cap.move_to(RIGHT * (x0 + c * (MINI_KEY + MINI_GAP)) + DOWN * r * (MINI_KEY + MINI_GAP))
                    self.add(cap)
                    if not bits & BITS[name]:
                        continue
                    key = f"{side}.{name}"
                    self.caps[key] = cap
                    self.colours[key] = press_colour(event.mode, side, name)
                    # The thumbs change what a chord means rather than which
                    # chord it is, so the teacher names them even this small.
                    if name in SHOWN:
                        self.legends[key] = Text(
                            SHOWN[name], font=SANS, font_size=9, weight="BOLD", color=BACKGROUND,
                        ).move_to(cap)
        self.add(*self.legends.values())

    def show(self, lit, opacity: float):
        for key, cap in self.caps.items():
            cap.set_fill(self.colours[key] if key in lit else KEY_IDLE)
        self.set_opacity(opacity)
        for key, legend in self.legends.items():
            legend.set_opacity(opacity if key in lit else 0)


class Entry(VGroup):
    """One chord or stroke of the stream: its small board, and what it typed.

    The board comes up with the chord's first key and its keys light as they
    go down, staying lit until the chord commits, so it is drawn rather than
    pressed and released.  What it typed pops up beside it at the commit.
    """

    def __init__(self, event: Event, width: float):
        super().__init__()
        self.event = event
        self.glyph = Glyph(event)
        text = event.label.replace("\n", "⏎") or "·"
        # Between two invisible bars, so every label sits at the same height.
        self.word = Text("|" + text + "|", font=MONO, font_size=44, color=DEAD if event.dead else TEXT)
        room = width - self.glyph.width - MINI_LABEL_BUFF
        if self.word.width > room:
            self.word.scale(room / self.word.width)
        self.word.next_to(self.glyph, RIGHT, buff=MINI_LABEL_BUFF)
        self.add(self.glyph, self.word)
        self.scale_now = 1.0

    def show(self, clip: Clip, t: float, speed: float, opacity: float):
        e = self.event
        age = (t - e.t) / speed
        if age < 0:
            lit = {k for k in self.glyph.caps if clip.pressed_between(k, e.first_key, t)}
        else:
            lit = self.glyph.caps.keys()
        self.glyph.show(lit, opacity)
        pop = 0.0 if age < 0 else min(1.0, age / 0.1)
        scale = 1.3 - 0.3 * pop
        self.word.scale(scale / self.scale_now, about_point=self.word.get_left())
        self.scale_now = scale
        self.word.set_opacity(0)
        self.word[1:-1].set_opacity(opacity * pop)


class Stream(Scene):
    """The clip as a list of small boards, each with what it typed.

    `KEYVID_FLOW=columns` (the default) fills the left column and then the
    right, clearing each as the next chord starts in it; `scroll` is one
    column that moves up as each chord comes in.
    """

    def construct(self):
        clip, speed = load_clip()
        flow = os.environ.get("KEYVID_FLOW", "columns")
        if flow not in ("columns", "scroll"):
            raise SystemExit(f"KEYVID_FLOW is columns or scroll, not {flow!r}")
        self.camera.background_color = BACKGROUND
        badges = corner_badges(self, speed)

        column_width = config.frame_width / 2 - LINE_MARGIN
        strokes = clip.strokes()
        entries = [Entry(e, column_width - 0.4) for e in strokes]
        pitch = (entries[0].glyph.height if entries else 1) + ENTRY_GAP
        rows = int((STREAM_TOP - STREAM_BOTTOM + ENTRY_GAP) / pitch)
        # The rows are spread to fill the space, not packed at the top.
        pitch = (STREAM_TOP - STREAM_BOTTOM + ENTRY_GAP) / rows
        top = STREAM_TOP - (pitch - ENTRY_GAP) / 2
        # When each entry's board comes up, and so takes its place.
        starts = [min(e.first_key, e.t) for e in strokes]

        def place(entry: Entry, x: float, y: float):
            entry.shift((x - entry.glyph.get_left()[0]) * RIGHT + (y - entry.glyph.get_center()[1]) * UP)

        if flow == "columns":
            xs = [-config.frame_width / 2 + LINE_MARGIN, LINE_MARGIN / 2]
            for i, entry in enumerate(entries):
                place(entry, xs[(i // rows) % 2], top - (i % rows) * pitch)
        else:
            x = -column_width / 2
            for entry in entries:
                place(entry, x, top)

        def visible(t: float) -> list[tuple[Entry, float]]:
            """The entries on screen at `t`, with their opacity."""
            if flow == "columns":
                shown = []
                for i, entry in enumerate(entries):
                    if starts[i] > t:
                        break
                    # A column clears when the next block starts in it.
                    nxt = (i // rows + 2) * rows
                    fade = 1.0
                    if nxt < len(entries) and starts[nxt] <= t:
                        fade = 1 - (t - starts[nxt]) / speed / CLEAR
                        if fade <= 0:
                            continue
                    shown.append((entry, fade))
                return shown
            # How far the column has scrolled, in entries: each one eases in.
            done = sum(smooth(min(1.0, max(0.0, (t - s) / speed / SCROLL))) for s in starts if s <= t)
            bottom = top - (rows - 1) * pitch
            shown = []
            for i, entry in enumerate(entries):
                if starts[i] > t:
                    break
                y = bottom + (done - i - 1) * pitch
                if y > top + pitch:
                    continue
                # In from below, out through the top, and invisible for the
                # first and last of the way so as not to touch the typed line.
                fade = min(1.0, (y - (bottom - pitch)) / pitch, (top + pitch - y) / pitch)
                fade = max(0.0, (fade - 0.4) / 0.6)
                entry.shift((y - entry.glyph.get_center()[1]) * UP)
                shown.append((entry, fade))
            return shown

        line_left = -config.frame_width / 2 + LINE_MARGIN
        line = TypedLine(
            line_left * RIGHT + (STREAM_BOTTOM - 0.8) * UP,
            config.frame_width - 2 * LINE_MARGIN,
        )
        stream = Group()
        clock = ValueTracker(0)

        def update(_):
            t = clock.get_value()
            mode = clip.mode_at(t)
            shown = visible(t)
            stream.remove(*stream.submobjects)
            for entry, opacity in shown:
                entry.show(clip, t, speed, opacity)
                stream.add(entry)
            line.set_text(clip.text_at(t))
            for m, badge in badges.items():
                badge.set_opacity(1 if m == mode else 0)

        # As in `Replay`, everything that changes is under the one updater.
        screen = Group(stream, line, *badges.values())
        screen.add_updater(update)
        self.add(screen)
        update(None)
        self.wait(0.5)
        self.play(
            clock.animate.set_value(clip.duration),
            run_time=clip.duration / speed,
            rate_func=linear,
        )
        self.wait(1)
