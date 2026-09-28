"""Animate a clip of chorded typing: the two hands, the chords, the text.

    KEYVID_CLIP=clips/orsy.json KEYVID_SPEED=0.5 uv run manim -pqm scenes.py Replay

The board is drawn the way TaipoTeacher's stroke hint draws it: each hand's
four finger columns in two rows, the thumbs in a row below under the index
side, the right hand the mirror of the left.  No column stagger.

The whole clip is one animation of a clock through the log's time, and every
frame is worked out from the clock, so the keys move with the real timings --
slowed by `KEYVID_SPEED` -- rather than on a beat of manim's choosing.
"""

import os
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

# The left hand, row by row, pinky on the outside; None is empty board.
LEFT_HAND = [
    ["r", "s", "n", "i"],
    ["a", "o", "t", "e"],
    [None, None, "Sp", "Bk"],
]
BITS = {"a": 1, "o": 2, "t": 4, "e": 8, "r": 16, "s": 32, "n": 64, "i": 128, "Sp": 256, "Bk": 512}
ORSY_OUTER = 0x067
# The key no Dosh or Orsy chord uses.
UNUSED = {"dosh": {"r"}, "orsy": {"r"}, "taipo": set()}

MODE_NAMES = {"dosh": "Dosh", "orsy": "Orsy", "taipo": "Taipo"}

# How long a chord's label is on screen, in video seconds.
LABEL_LIFE = 0.9
# How long the keys of a committed chord flash.
FLASH = 0.25
# How many characters of typed text fit on the line.
LINE_CHARS = 38


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
        self.legend = Text(name, font=SANS, font_size=22, color=KEY_NAME)
        self.legend.move_to(self.cap)
        self.add(self.cap, self.legend)

    def show(self, mode: str, down: bool, flash: float, dead: bool):
        """Draw the key held or not, with `flash` (0..1) of a commit on it."""
        unused = self.name_ in UNUSED.get(mode, set())
        if down:
            colour = press_colour(mode, self.side, self.name_)
            self.cap.set_fill(colour, opacity=1)
            self.legend.set_color(BACKGROUND)
        else:
            self.cap.set_fill(KEY_IDLE, opacity=0.35 if unused else 1)
            self.legend.set_color(KEY_NAME).set_opacity(0.3 if unused else 1)
        if flash > 0:
            edge = DEAD if dead else "#ffffff"
            self.cap.set_stroke(edge, width=2 + 6 * flash, opacity=1)
        else:
            self.cap.set_stroke(KEY_EDGE, width=2, opacity=0.35 if unused else 1)


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
        spell = "" if event.spell == event.label else event.spell
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

    def __init__(self, left_edge):
        super().__init__()
        self.left_edge = left_edge
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
        if len(line) > LINE_CHARS:
            line = line[-LINE_CHARS:]
        t = self._render(line)
        if t is not self.current:
            if self.current is not None:
                self.remove(self.current)
            self.add(t)
            self.current = t
        self.cursor.move_to(t[-1].get_center())


class Replay(Scene):
    def construct(self):
        clip_path = os.environ.get("KEYVID_CLIP")
        if not clip_path:
            raise SystemExit("set KEYVID_CLIP to a clip written by keyvid-extract")
        speed = float(os.environ.get("KEYVID_SPEED", "1"))
        clip = Clip(clip_path)
        self.camera.background_color = BACKGROUND

        left, left_keys = build_hand("L")
        right, right_keys = build_hand("R")
        left.move_to(LEFT * (left.width / 2 + SPLIT / 2))
        right.move_to(RIGHT * (right.width / 2 + SPLIT / 2))
        board = VGroup(left, right).shift(DOWN * 0.3)
        keys = {**left_keys, **right_keys}

        badges = {
            m: Text(n, font=SANS, font_size=30, color=DIM).to_corner(UP + LEFT, buff=0.5)
            for m, n in MODE_NAMES.items()
        }
        if speed != 1:
            rate = Text(f"×{speed:g}", font=SANS, font_size=26, color=DIM)
            rate.to_corner(UP + RIGHT, buff=0.5)
            self.add(rate)

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

        line = TypedLine(left.get_left()[0] * RIGHT + (board.get_bottom()[1] - 1.0) * UP)

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
