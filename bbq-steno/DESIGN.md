# bbq-steno Design

## Purpose

`bbq-steno` is the steno translation engine used by the keyboard firmware. It has three core jobs:

- Represent steno strokes in a compact, sortable form.
- Look up incremental stroke sequences against one or more dictionaries.
- Convert dictionary translations into concrete edit operations that can be typed on the host.

The crate is designed to run in firmware environments, so the active code path avoids heap-heavy data structures where possible, supports `no_std`, and can read dictionaries directly from flash.

## Scope

The crate contains a few distinct layers:

- `stroke.rs`: the canonical representation of a single steno stroke.
- `dict.rs`: dictionary abstraction and incremental prefix search.
- `memdict.rs`: compact flash-backed dictionary format and loader.
- `replacements.rs`: structured control language encoded inside dictionary values.
- `dict/lookup.rs`: stroke-by-stroke translation lookup with undo tracking.
- `dict/joiner.rs`: turns translation results into text edits and state transitions.
- `dict/emily.rs`: built-in programmatic dictionary for Emily-style symbols.

There is also an older `dict/translate.rs` + `dict/typer.rs` pipeline still in tree. It is useful as reference, but it is not the live firmware path.

## Runtime Architecture

The active runtime pipeline is:

1. Firmware collects a `Stroke`.
2. `Lookup` advances all currently viable dictionary prefixes plus a fresh root for each dictionary.
3. `Lookup` picks the best translation by longest match, with later dictionaries winning ties.
4. The winning dictionary string is decoded into `Vec<Replacement>`.
5. `Joiner` applies those replacements against its current typed-text model and emits `Joined::Type { remove, append }`.
6. Firmware converts `Joined` into host key events.

In the current firmware integration, `bbq-keyboard/src/dict.rs` owns one `Lookup` and one `Joiner`, loads flash dictionaries with `MemDict::from_raw_ptr`, and feeds results to the host-side typing layer.

## Stroke Model

`Stroke` is a thin wrapper around `u32`.

- Bits encode the full US steno layout.
- Ordering is meaningful: dictionary keys are sorted lexicographically by stroke sequence, and individual strokes must therefore be comparable.
- `succ()` provides the next sortable stroke value, which is used to compute an upper bound in prefix scans.
- `is_star()` treats `*`, `^`, and `+` as undo strokes.

Important consequence: the text representation is only a UI/debug format. Internally, all algorithms operate on the bit-encoded `Stroke`, not on strings.

## Dictionary Abstraction

`DictImpl` exposes a dictionary as:

- `len()`
- `key(index) -> &[Stroke]`
- `value(index) -> &str`
- `selector() -> Box<dyn Selector>`

The abstraction assumes dictionary entries are sorted by stroke sequence. Incremental lookup is implemented by `BinarySelector`, which maintains:

- `left` and `right`: a half-open range of candidate entries.
- `count`: how many strokes of the key have already been consumed.

Each new stroke narrows the range with two binary searches:

- lower bound for `needle`
- lower bound for `needle.succ()`

If the narrowed range is non-empty, lookup continues. If the candidate key length exactly matches the consumed length, that step also yields a translation.

This design gives incremental prefix search without building a trie in RAM. The tradeoff is that dictionary producers must guarantee sorted keys and stable serialization.

## Dictionary Implementations

### `RamDict`

`RamDict` is a simple in-memory implementation used by tests and tooling.

- Keys are flattened into a single `Vec<Stroke>`.
- Values are flattened into one `String`.
- Tables store `(start, end)` ranges into those flat buffers.

This keeps the implementation simple while preserving the same sorted-array access pattern used by flash dictionaries.

### `MemDict`

`MemDict` is the firmware-oriented dictionary representation.

- A CBOR header describes offsets and lengths of the key and text regions.
- Keys are stored as contiguous `Stroke` values.
- Per-entry key and value metadata are packed into `u32`, with high bits as length and low 24 bits as offset.
- `from_raw_ptr()` can decode either a single dictionary or a grouped dictionary image.
- Group images can mix flash dictionaries with built-in dictionaries such as `"emily-symbols"`.

The loader intentionally treats the mapped data as `'static`. That matches the flash use case, but it means callers must only pass pointers to stable, unmoving memory.

## Translation Semantics

Dictionary values are not plain text. They are encoded strings containing embedded control markers, decoded by `Replacement::decode()` into structured operations.

Supported operations include:

- literal text insertion
- delete/force space
- capitalize or suppress capitalization of the next word
- stitch behavior
- retroactive edits of previous words
- raw key actions
- retroactive break/currency/number operations

This encoding keeps the dictionary storage compact and lets dictionary tooling emit one serialized string format regardless of whether the target is flash or RAM.

## `Lookup`: Incremental Translation

`Lookup` is the first stateful runtime stage.

For each stroke:

- If the stroke is an undo stroke, it pops one history item and returns `Action::Undo`.
- Otherwise it advances every currently active selector plus a fresh selector from every dictionary root.
- It tracks the longest matching translation encountered at this step.
- If nothing matches, it synthesizes a raw-steno fallback translation from the stroke text.
- It discards shorter partial matches once a longer translation is accepted.

The result is:

- `Action::Add { text, strokes }` for a translation
- `Action::Undo` for undo

`strokes` is the number of input strokes consumed by the chosen translation. That value is what allows later stages to replace prior output when a longer multi-stroke word becomes known.

### Priority Rules

- Longer matches beat shorter matches.
- On equal match length, later dictionaries override earlier ones.

That is how user dictionaries can override base dictionaries while still allowing multi-stroke entries to replace earlier partial output.

### Undo Model

`Lookup` keeps a bounded history of selector states. Undo removes the latest lookup state, but it does not itself reconstruct typed text. That responsibility is delegated to `Joiner`.

If a translation includes a `Replacement::Raw`, `Lookup` clears its history. The design assumption is that raw keypress actions create an undo barrier because replaying or reverting them at the text level is not meaningful.

## `Joiner`: Text Reconstruction

`Joiner` is the second stateful runtime stage. Its input is `lookup::Action`; its output is `Joined::Type { remove, append }`.

Internally it tracks:

- `typed`: its own model of what text currently exists
- `history`: per-stroke edit records for undo/replacement
- `actions`: queued host edit operations
- a lightweight `State` carried across translations:
  - `cap`
  - `space`
  - `force_space`
  - `stitch`

When a new translation arrives, `Joiner`:

1. Determines how much prior output must be removed based on `strokes`.
2. Rewinds its internal `typed` buffer accordingly.
3. Applies each `Replacement` in order, updating both text and join state.
4. Simplifies redundant delete-and-retype prefixes.
5. Records an `Add` history item so undo can restore the previous text.

This separation is important:

- `Lookup` reasons about dictionary matches.
- `Joiner` reasons about visible text and editing side effects.

That split keeps the prefix-search logic independent from spacing, capitalization, and retroactive formatting rules.

## Built-in Dictionary: Emily Symbols

`dict/emily.rs` implements Emily-style symbols without storing them as ordinary dictionary rows.

The code decodes a qualifying stroke by masking subfields:

- starter
- spacing flags
- variant selection
- repetition count
- capitalization flag
- symbol code

If the stroke matches, it emits an encoded replacement string equivalent to a normal dictionary translation. Operationally, this makes the built-in dictionary look like any other `DictImpl`, while keeping storage near zero.

## Firmware Integration

The active firmware integration is in `bbq-keyboard/src/dict.rs`.

- Main and user dictionaries are loaded from fixed flash addresses.
- The resulting dictionary list is passed to `Lookup`.
- `Joiner` produces `Joined::Type` actions for the host typing layer.
- A separate raw mode bypasses lookup and types literal stroke text.

This means `bbq-steno` is not just a parser or search library. It is effectively the full text-generation engine for steno mode.

## Current Tests

Current test coverage is light but real:

- stroke text/bitfield round-trip
- replacement encode/decode round-trip
- basic `Typer` behavior
- `RamDict` selector behavior

Notably absent are focused tests for:

- `Lookup` longest-match behavior
- `Joiner` spacing/capitalization edge cases
- grouped `MemDict` loading
- undo interactions involving retroactive replacements
- Emily symbol decoding

## Design Constraints and Tradeoffs

### Why sorted-array search instead of a trie

Advantages:

- compact serialized format
- direct flash access
- no runtime trie construction
- shared abstraction for RAM and flash dictionaries

Costs:

- dictionary build tools must sort correctly
- lookup is binary-search based rather than pointer-chasing
- dynamic insertion is not a goal

### Why split `Lookup` and `Joiner`

Advantages:

- dictionary search stays independent from text formatting rules
- undo can be handled at the text layer with explicit history
- raw key actions can act as barriers without complicating dictionary search

Costs:

- two histories must stay semantically aligned
- replacement bugs can be subtle because the stages are stateful in different ways

## Technical Debt and Open Questions

### Legacy translator path

`dict/translate.rs` and `dict/typer.rs` implement an older translation pipeline with overlapping responsibilities. It is still exported, but the firmware path uses `Lookup` and `Joiner`.

That creates three risks:

- the public API suggests two competing architectures
- fixes may land in the inactive path by mistake
- tests may not cover the live path well enough

### Unsafe flash loading

`MemDict::from_raw_ptr()` is intentionally unsafe and trusts offsets from the serialized header. That is appropriate for trusted flash images, but it means malformed dictionary images can cause invalid memory access or invalid UTF-8 assumptions.

### Unicode correctness

`Joiner` mixes byte lengths and character iteration in a few places. Some paths explicitly account for Unicode scalar values, but others still use `.len()` on `String`. That deserves careful review if dictionaries will emit non-ASCII text regularly.

### History bounds

Undo and typed-text history are bounded. Extremely long translations or repeated retroactive operations can eventually fall off history. That is acceptable for firmware, but it should be documented as a design constraint rather than assumed away.

## Suggested Next Documentation Work

Useful follow-on sections, if this document is expanded later:

- exact serialized `MemDict` layout with diagrams
- `Replacement` encoding reference with examples
- worked example of a multi-stroke word replacing earlier output
- undo walkthrough across `Lookup` and `Joiner`
- explicit note on the intended future of the legacy `Translator`
