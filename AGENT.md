# Overview

This repository is a body of code implementing the firmware for a family of combination
steno/qwerty/taipo keyboards. The current daily driver is the "Jolt 3", a 42-key split keyboard
organized to make steno usage ergonomic, as well as regular qwerty use.

The project is primarily written in Rust. There are two main firmware variants:

- `jolt-embassy-rp`: the **currently running firmware**, built on Embassy for the RP2040. This is
  what runs on the Jolt 3 today.
- `jolt`: an in-progress port of the firmware to Zephyr, using the zephyr-lang-rust support.
  Implementing this will likely require numerous improvements to the zephyr-lang-rust project.

There is also an `archive/jolt` directory, a much older version of the firmware built around Zephyr
before official Rust support came to Zephyr. It doesn't build but is a useful reference (including
shield definitions for older keyboards such as the proto2/proto3/proto4).

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
  translation. Keyboard geometry is selected with the `proto2` (2-row) / `proto3` (3-row) cargo
  features; `jolt-embassy-rp` builds with `proto3` scancodes and translates per-board at runtime.
- minder: A simple protocol, used over a USB bulk channel, to update dictionaries, and get basic
  status.

## Firmware implementations
- jolt-embassy-rp: The current running firmware. Board-specific initialization lives in
  `src/board.rs` (with modules for jolt3, jolt2, and jolt2dir); the board is identified at runtime
  from a `BoardInfo` CBOR block stored in flash (see `bbq-keyboard/src/boardinfo.rs`), so one
  binary serves all supported boards. Scancode translation per board is in `src/translate.rs`.
- jolt: The start of a new project to make the current firmware run on Zephyr.
- zbbq: A different branch of earlier versions of the Zephyr version.
- proto: The older pre-Zephyr (rtic-based) firmware; superseded, but a reference for boards like
  the proto2.

## Utilities
- bbq-tool: A tool for converting dictionaries from a few formats to the binary format used by the
  bbq-steno::Dict code. See `bbq-tool/CLAUDE.md`.
- keyminder: A command line tool implementing the Host PC side of the 'minder' protocol.
- typey: A simple command line tool to test the dictionary. See `typey/CLAUDE.md`.
- dict-test: The start of a more automated test of the dictionary translation

## Sibling checkouts (untracked)

These directories are separate git repositories checked out inside this one, and are ignored by
the parent repo:

- `embassy/`: a checkout of the tangybbq fork of Embassy. `jolt-embassy-rp` currently builds
  against crates.io releases, but its Cargo.toml retains commented-out path dependencies pointing
  here for when local patches are needed.
- `steno-flow/`: a separate ML project for steno, with its own CLAUDE.md and TASKS.md.

Other directories can be ignored at this time.

# Agent guidelines

- Changes should be made incrementally, and grouped into logical commits.
- A given change, in general, should either change functionality, or refactor/improve the code. Try
  not to combine refactoring and functional changes into the same commit.
- The developer prefers gradual and incremental review of changes to the code rather than
  large-scale changes that are difficult to understand.
- Commit the work as part of doing it.  Do not leave finished changes sitting unstaged waiting
  for permission to commit; make the commit yourself as the natural end of each logical change.
  The developer reviews with git (`git log`, `git show`, `git diff`) before anything is pushed, so
  a commit is a proposal, not a publication.  Nothing is ever pushed without the developer asking.
- Amending, reordering, or otherwise rewriting commits that have not been pushed is fine when the
  developer's review asks for it.
- The code should be committed to git with these guidelines:
  - Commit text should follow git conventions:
    - A short summary, followed by a blank line
    - A textual description of the change.  The body of the commit should almost always be present,
      giving a bit more detail than the short summary.
    - The body should be wrapped to 72 column max lines, when reasonable (long symbols may make it
      overflow)
  - The commit text should be worded in the simple present tense, not past. "Add ..." instead of
    "Added ...".

# Testing

- Due to the complexity, each change requires manual testing of multiple systems.  That testing
  happens during the developer's review of the commits, not before they are made; commit the work
  and say plainly in the response what has and has not been tested.

# Building

## Building `jolt-embassy-rp`

- Build from the `jolt-embassy-rp` directory with `just build` (or `cargo build --bin
  jolt-embassy-rp`; the target is `thumbv6m-none-eabi`).
- `just uf2` converts the ELF to `main.uf2` and copies it to a keyboard in UF2 bootloader mode.
- `just serve` / `just gdb` / `just rtt` support JTAG debugging via a Segger J-Link.

## Building `jolt`

- Jolt can be built from the repository root with `./jolt/b-proto4.sh`.
  The script changes into the `jolt` directory and, when the environment isn't already set up
  (e.g. in the Agent shell), sources the repository-root `.envrc` for the Zephyr environment.
  Once the `build` directory is present, the symlink in `.cargo/config.toml` will allow `cargo
  check` and other cargo commands to work normally.

# Current work tracking

The active task list lives in `TASKS.md` (a local, untracked planning file).

- Treat `TASKS.md` as the source of truth for current and pending work.
- If `TASKS.md` conflicts with older notes elsewhere, follow `TASKS.md`.
