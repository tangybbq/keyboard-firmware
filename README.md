# David Brown's Keyboard Firmware

This repo contains the firmware I use for my various
[keyboards](https://github.com/tangybbq/keyboard). Support for a given keyboard is generally best
for the newer ones, as I generally don't use the older ones that much. As of mid-2026, my main
keyboard is the jolt3, running the `jolt-embassy-rp` firmware.

This firmware has gone through several iterations and rewrites, with a rough timeframe of:

- 2023-10-10: First commit on top of an rtic sample.  This continued for a while, quickly becoming
  my regular firmware (the `proto` directory).
- 2024-02-28: First commit on top of Zephyr.  I started as I began to grow frustrated at how many
  types from the hals leaked into the code.  This was largely a bunch of C code to make for a bit
  easier interfaces to the Zephyr interfaces (now under `archive/`).
- 2024-09-19: First commit based on the
  [zephyr-lang-rust](https://github.com/zephyrproject-rtos/zephyr-lang-rust) work.  It turns out to
  be a nice practical test to help confirm that the interfaces I'm developing for the Zephyr Rust
  support are useful (the `jolt` directory).
- 2025-02-07: First commit of `jolt-embassy-rp`, a port of the firmware to
  [Embassy](https://embassy.dev/) on the RP2040.  This is the firmware I currently run, while the
  Zephyr version waits on upstream Rust support maturing.

Over time, I've moved more and more functionality out of the main program directories and into
various platform-independent crates, all starting with `bbq-`.

- `bbq-steno`: This implements the bulk of the steno functionality, including:
  - `stroke::Stroke`: The primary type that represents a single steno stroke.  This is extended to
    support the `+` and `^` keys as well, which I use extensively.
  - `dict`: Steno dictionary and translation work.  The `Lookup` type manages incremental steno
    lookup, across multiple dictionaries, with undo support (the '*' key).  The `Joiner` type takes
    the output of `Lookup`, keeps track of what has been typed, and results in actions that are
    sequences of keys to be typed, deletions to be done, and raw key events (control keys and such).
  - `dict::emily`: My slightly modified version of Emily's Symbols.  This is how I enter symbols and
    do programming and such with steno.
  - `mapdict`: An implementation of the traits from `dict` to support a compact memory-mapped
    encoding of steno dictionaries.  These are placed directly in flash.
  - `bbq-steno-macros`: A proc macro crate that provides a `stroke!("STROEBG")` macro to insert
    steno strokes directly into the code.  Unfortunately, as it uses the `bbq-steno` crate, that
    crate can't use the macros.  There is a `bbq-consts` crate that helps make the constants used in
    the Emily's symbols code.
- `bbq-keyboard`: This implements the functionality of a keyboard. It is platform independent.  It
  supports several different modes and mappings:
  - `layout`: This manages the layouts in general.  It also processes the mode switch (lower left on
    42-key, upper left on 30-key), with tapping to switch modes, and holding it and various home row
    letter keys to select specific modes.
  - `layout::qwerty`: A somewhat traditional qwerty layout, but designed for a 42-key keyboard.
    Because my keyboards are designed for steno, it is very easy to press adjacent keys with a
    single finger, and this qwerty layer first detects numerous of these, effectively adding 24
    more keys.  The end result is a somewhat intuitive layout for those that have spent a lot of
    time on a traditional qwerty keyboard.
  - `layout::steno`: The steno support itself.  This also detects a special `RA*U` stroke to switch
    between translating, and just sending the untranslated raw strokes.  I use this raw mode for
    various tests that run on the host, as well as the steno-drill program I use to learn and keep
    steno knowledge.
  - `layout::artsey`: An implementation of artsey.  I don't know if this works any more, as I don't
    use it.  I use Taipo now.
  - `layout::taipo`: An implementation of the Taipo layout.  I added this primarily to use with
    2-row, 30-key keyboards as it only needs 8 finger keys and two thumb keys per hand.  It turns
    out to be a pretty nice layout, especially with its identical layout per hand (mirrored) and
    encouragement of alternating hands.  If I get good at this, I might switch primarily to two-row
    keyboards.
  - `serialize`: Implements a CRC'd packet protocol used over UART between the two halves of the
    keyboard.  It sends the state of the keys from the passive side, and there is an LED value sent
    to the passive side, which isn't quite implemented yet.  (The jolt3 uses an I2C protocol
    between halves instead, in `jolt-embassy-rp/src/inter.rs`.)
  - `boardinfo`: A small block of cbor used to identify the specific keyboard.  This saves a gpio on
    split keyboards, where I prefer running the same firmware on both halves, and lets a single
    firmware binary support several different boards.
- `minder`: A simple protocol, run over a USB bulk channel, to update dictionaries and query basic
  status.  `keyminder` is the host-side command line tool.
- `bbq-tool`: The tool used to build the binary dictionaries, as well as the boardinfo file.
- `dict-test`: Uses the bbq-steno library, and reads my Phoenix exercise files.  As I am unable to
  distribute these, this isn't likely to be useful for others.
- `typey`: A host-based translation tool. It expects the keyboard to be in raw mode (where it sends
  the text of the stroke followed by a space). It supports a `write` command which will show some of
  the details of the translation, and an `exbuild` command that lets me enter exercises for steno
  drill.

## Steno support

The most important thing to note is that I do not use the Plover theory. I've tried it, I tried
Lapwing, and ended up going back to Phoenix due to the quality and quantity of the training
material.

This has a few significant impacts:

- My translation is strictly a greedy regex match of the translations.  This means that longer
  translations will take priority over shorter ones, but it will never replace a previous longer
  translation with a shorter one to make later translations work better.  I don't know how much
  other theories depend on this.
- My formatting codes are a bit different and a bit more simple.  In addition to "delete space"
  (which is really more suppress auto-space), I have a "force space" which has higher precedence.
  This is used by the Emily's Symbols to insert spaces, giving priority to spaces when requested (if
  the first stroke says to have a space after it, that will "win" over a following stroke that tries
  to suppress spaces.
- I only interpret the formatting codes that are in my dictionaries. Even then, I have missed some,
  and will fix them as I encounter them.
- `RA*U` for raw mode is handled directly by the code and enters a special raw mode.
- Direct key strokes (Cursor movement, or Control-C for example) create an "undo" break, where the
  keyboard will not try to undo past them. I've found these tend to be fairly meaningless anyway.
  Once the cursor has moved, editing needs to be done using whatever environment the user is in.

## How to use

The firmware I currently run is in the `jolt-embassy-rp` directory.  It is a normal Rust embedded
project (no Zephyr needed): `just build` in that directory builds it for the RP2040
(`thumbv6m-none-eabi`), and `just uf2` produces a `main.uf2` and copies it to a keyboard waiting in
the UF2 bootloader.  The same binary supports my various boards; each keyboard carries a small
"boardinfo" CBOR block in flash (built with `bbq-tool`) that tells the firmware which board it is
on, and which side it is for split designs.

Dictionaries are built with `bbq-tool` (see `bbq-tool/dicts.sh`) into a memory-mapped binary format
that is flashed directly.  I have not been able to flash large dictionaries with the UF2 file (it
just seems to hang forever, it might just be _very_ slow, but I have given it over an hour).  I use
jtag for this.  For debugging the firmware, I recommend a JTAG interface anyway; the justfile has
recipes for a Segger J-Link with defmt RTT logging.

The `jolt` directory holds the in-progress Zephyr port, built on zephyr-lang-rust.  It expects a
working Zephyr install (see the `.envrc` at the repo root), and can be built with
`./jolt/b-proto4.sh`.  It is not yet functional enough to be my daily firmware.

## Future direction

I am currently using these keyboards exclusively, both at home, and when traveling.  The qwerty
layout is my main use, especially while programming, although I hope to keep transitioning to more
and more steno, even with code.  My goal is for the keyboard to be self-contained.

Some things I'm working toward:

- Support more of my older boards in `jolt-embassy-rp`, starting with the proto4 (a 30-key,
  tiny2040-based board where the single controller scans both halves directly).
- Steno stroke logging: capture every stroke written (including corrections) over the minder
  protocol, to build a personal corpus for ML experiments (the separate steno-flow project).
- Bring the Zephyr `jolt` port up to parity with `jolt-embassy-rp`, improving zephyr-lang-rust
  along the way.
- Implement a local flash-based user dictionary and sequences on the keyboard that can be used to
  program entries.
- Big picture: implement an app that can optionally be run on the host that detects changes in the
  focused window, and informs the keyboard.  The keyboard could maintain separate state (caps-next,
  auto-space or not, possibly even undo history).
- A host tool that could monitor the raw steno during normal use, and offer something similar to the
  suggestions window in Plover. Because the Phoenix dictionary is built around numerous prefix and
  suffix entries, this suggestion window doesn't actually work all that well, and I may put some
  thought into how to possibly do this better.
