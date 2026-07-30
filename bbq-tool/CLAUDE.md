# bbq-tool

This crate is a command-line tool for converting steno dictionaries from various source formats
into the packed binary format consumed by the bbq-steno runtime.

See the top-level `CLAUDE.md` for project-wide conventions (commit style, incremental changes,
testing policy, etc.).

## Purpose

Builds combined, memory-mapped binary dictionary files that are flashed onto the keyboard.
The tool also produces a `full.bin` used by host-side tools such as `dict-test` and
`keyminder`.

## Source formats supported

- **RTF/CRE** (`.rtf`) — the Phoenix theory dictionary and similar court-reporting dictionaries.
  Parsed in `src/rtfcre.rs`. RTF control words are translated into the internal encoding on load.
- **JSON** (`.json`) — Plover's native dictionary format, with Plover meta-code translation.
  Parsed in `src/jsondict.rs`.
- **YAML** (`.yaml` / `.yml`) — same meta-code semantics as JSON; used for small overrides
  (e.g. `rust.yaml`).

## Internal encoding

Both importers translate their source-format meta-codes into a compact byte-string encoding
defined by `bbq-steno::Replacement`. The sentinel bytes used are:

| Byte | Meaning |
|------|---------|
| `\x01` | Delete preceding space (suppress space / attach-left) |
| `\x02` | Capitalize next word |
| `\x03` | Number literal follows |

Unrecognized or unhandled codes currently fall through to literal `{…}` text in the output,
which causes mismatches when `dict-test` compares dictionary output to exercise translations.

## Building dictionaries

Run `./dicts.sh` from this directory. It invokes `cargo run` three times to produce:

- `dicts.bin` — main flash dictionary (Phoenix + fixes + built-in emily-symbols)
- `user-dict.bin` — user-layer dictionary (taipo, personal overrides)
- `full.bin` — combined file used by host tools (`dict-test`, etc.)

It then converts the `.bin` files to UF2 and ELF formats for flashing and GDB loading.
The script requires `$ZEPHYR_BASE` to be set (for `uf2conv.py`) and `arm-zephyr-eabi-objcopy`
to be on the path.

## Dictionary load order

Later dictionaries in each `build` invocation take priority over earlier ones. The order in
`dicts.sh` reflects this: `phoenix.rtf` is the base, `phoenix_fix.json` overrides specific
entries, and user dictionaries sit on top.

## Key source files

- `src/rtfcre.rs` — RTF/CRE tokenizer and encoder
- `src/jsondict.rs` — JSON/YAML importer and Plover meta-code translator
- `src/encode.rs` — binary packing into the memory-mapped format
- `src/main.rs` — CLI (`build`, `show`, `boardinfo` subcommands)

## Testing

There is no automated test suite in this crate. Correctness is verified by running `dicts.sh`
and then running `dict-test` (in `../dict-test`) against the resulting `full.bin`. Any
divergences reported by `dict-test` indicate encoding bugs or missing meta-code handling.
