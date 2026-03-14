# Overview

This repository is a body of code implementing the firmware for a combination steno/qwerty/taipo
keyboard called the "Jolt". This is a 42-key keyboard organized to make Steno usage ergonomic, as
well as regular qwerty use. It is the daily driver for the author.

The project is primarily written in Rust.  There are two main variants: "jolt-embassy-rp" a version
that uses Embassy on the raspberry pi 2040, and 'jolt' which will eventually be a version that runs
on Zephyr, using the zephyr-lang-rust support.  There is an `archive/jolt` directory, which is a
much older version of the firmware built around Zephyr, before official Rust support came to Zephyr.
It doesn't build but is a useful reference.

# Organization

There are the following crates:

## Core implementation
- bbq-steno: Implements the "Stroke" type to represent steno strokes, and implements an NFA-based
  dictionary lookup for translation to English.
- bbq-steno-macros: A proc macro `stroke!()` that allows steno strokes to be referenced in code, and
  compiled directly to the representational integer.
- bbq-consts: Because there can't be circularity between the above two, this manually extracts
  various steno constants into a checked in file `bbq-steno/src/dict/emily/consts.rs`.
- bbq-keyboard: The main body of keyboard firmware. Implements the Layout engine, with three
  primary implementations: Qwerty, Taipo, and Steno, and manages state changes between them. Qwerty
  and Taipo fully resolve to key press and release events, which are returned through callbacks. The
  Steno mode simply returns strokes, and the main firmware uses the dictionary support for
  translation.
- minder: A simple protocol, used over a USB bulk channel, to update dictionaries, and get basic
  status.

## Firmware implementations
- jolt-embassy-rs: The current running firmware.
- jolt: The start of a new project to make the current firmware run on Zephyr. Implementing this
  will likely require numerous improvements to the zephyr-lang-rust project.
- zbbq: A different branch of earlier versions of the Zephyr version.

## Utilities
- bbq-tool: A tool for converting dictionaries from a few formats to the binary format used by the
  bbq-steno::Dict code.
- keyminder: A command line tool implementing the Host PC side of the 'minder' protocol.
- typey: A simple command line tool to test the dictionary.
- dict-test: The start of a more automated test of the dictionary translation

Other directories can be ignored at this time.

# Agent guidelines

- Changes should be made incrementally, and grouped into logical commits.
- A given change, in general, should either change functionality, or refactor/improve the code. Try
  not to combine refactoring and functional changes into the same commit.
- The develop prefers gradual and incremental review of changes to the code rather than large-scale
  changes that are difficult to understand.
- The code should be committed to git with these guidelines:
  - Commit text should follow git conventions:
    - A short summary, followed by a blank line
    - A textual description of the change
    - A `Co-Authored-By` tag giving credit to the Agent model.
  - The commit text should be worded in the simple present tense, not past. "Add ..." instead of
    "Added ...".

# Testing

- Due to the complexity, each change will require manual testing of multiple systems before commits
  are made.

## Building `jolt`

- Jolt can be built from the repository root with `./jolt/b-proto4.sh`.
  The script changes into the `jolt` directory and loads `jolt/.envrc` so it works from within the
  Agent shell as well.
  Once the `build` directory is present, the symlink in `.cargo/config.toml` will allow `cargo
  check` and other cargo commands to work normally.

# Current work tracking

The active task list lives in `TASKS.md`.

- Treat `TASKS.md` as the source of truth for current and pending work.
- If `TASKS.md` conflicts with older notes elsewhere, follow `TASKS.md`.
