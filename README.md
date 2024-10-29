# David Brown's Keyboard Firmware

This repo contains the firmware I use for my various
[keyboards](https://github.com/tangybbq/keyboard). Support for a given keyboard is generally best
for the newer ones, as I generally don't use the older ones that much.  Currently, the proto3,
proto4 and jolt1 are my main interest, and the jolt1 is my main keyboard I use as of 2024-10-29.

This firmware has gone through several iterations and rewrites, with a rough timeframe of:

- 2023-10-10: First commit on top of an rtic sample.  This continued for a while, quickly becoming
  my regular firmware.
- 2024-02-28: First commit on top of Zephyr.  I started as I began to grow frustrated at how many
  types from the hals leaked into the code.  This was largely a bunch of C code to make for a bit
  easier interfaces to the Zephyr interfaces.
- 2024-09-19: First commit based on the
  [zephyr-lang-rust](https://github.com/dnaq/plover-machine-hid) work.  It turns out to be a nice
  practical test to help confirm that the interfaces I'm developing for the Zephyr Rust support are
  useful.

Over time, I've moved more and more functionality out of the main program directory (before `proto`,
now `jolt`) and into various crates, all starting with `bbq-`.

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
  - `bbq-steno-macros`: A proc macro crate that provides a `steno!("STROEBG")` macro to insert steno
    strokes directly into the code.  Unfortunately, as it uses the `bbq-steno` crate, that crate
    can't use the macros.  There is a `bbq-consts` crate that helps make the constants used in the
    Emily's symbols crate.
- `bbq-keyboard`: This implements the functionality of a keyboard. It is platform independent.  It
  supports several different modes and mappings:
  - `layout`: This manages the layouts in general.  It also processes the mode switch (lower left on
    42-key, upper left on 30-key), with taping to switch modes, an holding it and various home row
    letter keys to select specific modes.
  - `layout::qwerty`: A somewhat traditional qwerty layout, but designed for a 42-key keyboard.
    Because my keyboards are designed for steno, it is very easy to press adjacent keys with a
    single keyboard, and this qwerty layer first detects numerous of these, effectively adding 24
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
    out to be a pretty nice layout, especially with it's identical layout per hand (mirrored) and
    encouragement of alternating hand.  If I get good at this, I might switch primarily to two-row
    keyboards.
  - `serialize`: Implements a CRC'd packet protocol used over UART between the two halves of the
    keyboard.  It sends the state of the keys from the passive side, and there is an LED value sent
    to the passive side, which isn't quite implemented yet.
  - `boardinfo`: A small block of cbor used to identify the specific keyboard.  This saves a gpio on
    split keyboards, where I prefer running the same firmware on both halves.
- `bbq-tool`: The tool used to build the binary dictionaries, as well as the boardinto file.
- `dict-test`: Uses the bbq-steno library, and reads my Phoenix exercise files.  As I am unable to
  distribute these, this isn't likely to be useful for others.
- `typey`: A host-based translation tool. It expecte the keyboard to be in raw mode (where it sends
  the text of the stroke followed by a space). It supports a `write` command which will show some of
  the details of the translation, and an `exbuild` command that let's me enter exercises for steno
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

The current version of this code is in the `jolt` directory.  It expects a working Zephyr install,
and will almost certainly depend on extra changes in [this
pr](https://github.com/zephyrproject-rtos/zephyr-lang-rust/pull/22).

There is a justfile that describes how to build for the various keyboards.  Note that, until I
manage to fix it, the bbq-keyboard crate needs a feature to be set as to whether the keyboard is a 2
or 3 row keyboard.  Otherwise, the configuration comes from the Zephyr device trees.

The images can be flashed with the UF2 files.  I have not been able to flash large dictionaries with
the UF2 file (it just seems to hang forever, it might just be _very_ slow, but I have given it over
an hour). I use jtag for this. For debugging the firmware, I recommend a JTAG interface anyway.

## Future direction

I am currently using these keyboards exclusively, both at home, and when traveling.  The qwerty
layout is my main use, especially while programming, although as I'm nearing the last of the lessons
in the Phoenix theory, I hope to start transitioning to more and more steno, even with code.  My
goal is for the keyboard to be self-contained.

Some other ideas I have:

- Implement a local flash-based user dictionary and sequences on the keyboard that can be used to
  program entries.
- Develop a protocol, probably HID, to allow for management of a user dictionary.
- Add log messages and other debugging utilities to the HID interface.
- Big picture: implement an app that can optionally be run on the host that detects changes in the
  focused window, and informs the keyboard.  The keyboard could maintain separate state (caps-next,
  auto-space or not, possibly even undo history).
- A host tool that could monitor the raw steno during normal use, and offer something similar to the
  suggestions window in Plover. Because the Phoenix dictionary is built around numerous prefix and
  suffix entries, this suggestion window doesn't doesn't actually work all that well, and I may put
  some thought into how to possibly do this better.
