# keyvid

Videos of Dosh and Orsy typing, animated with [manim](https://www.manim.community/) from the
key logs TaipoTeacher writes.

A clip shows both hands the way the teacher's stroke hint draws them, the keys going down and
up with their real timings, what each chord or stroke typed popping up over the hand that
typed it, and the text building up underneath.  In Orsy the keys take the colours of their
Series: onset blue, second teal, vowel orange, coda purple.

## Pieces

- `extract/`: a Rust tool that replays a log through `bbq_keyboard::replay` (the real layout
  engine, not a model of it) and writes a stretch of it as JSON: every key event, every chord
  or stroke with its keys and label, and the text on the screen after each.
- `clip.py`: reads a clip and answers what is held, what mode it is in, and what has been
  typed, at any moment.
- `scenes.py`: the manim scene.  One animation runs a clock through the clip and every frame
  is drawn from it.

## Making a clip

```sh
just list 2026-09-28                                  # what was typed when, per session
just clip 2026-09-28 0 1842000 1852000 orsy-directed  # day, session, from, to (log ms), name
just render orsy-directed 0.5 m                       # half speed, 720p
```

The video lands in `media/videos/scenes/<quality>/orsy-directed.mp4`.  `just preview NAME`
renders at low quality and opens it.

`uv sync` sets up the Python side the first time.

## Caveats

- The replay uses today's chord tables.  A session typed under different Taipo or Dosh
  tables gets a warning; a change to the Orsy tables cannot be detected, since the session
  header does not fingerprint them.  The d/g swap (2026-09-27, `a2b10b4`) is one: Orsy
  typed before it comes out with d and g exchanged.  Cut Orsy clips from later logs.
- A clip's text starts empty.  A backspace into text typed before the clip began takes
  nothing off the line.
- `clips/` and `media/` are ignored by git: the logs hold whatever was typed.
