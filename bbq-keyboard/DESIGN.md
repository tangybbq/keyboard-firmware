# bbq-keyboard Design

## Purpose

`bbq-keyboard` is the policy layer between raw keyboard hardware events and the user-visible behavior of the Jolt keyboards.

It is responsible for:

- defining shared keyboard-side types and events
- decoding matrix/interconnect key events into higher-level layout behavior
- managing mode selection across multiple input systems
- integrating steno translation through `bbq-steno`
- describing inter-half communication formats
- providing helper logic to turn text into USB HID actions

In practice, this crate is the core keyboard behavior engine. Firmware crates such as `jolt-embassy-rp` provide hardware, scheduling, transport, and USB plumbing around it.

## Scope

The crate has four major areas:

- Core shared types in `lib.rs`
- Input/layout engine in `layout/`
- Steno integration in `dict.rs`
- Support modules for board metadata, interconnect protocols, and USB typing

This is not a board-support crate. It does not scan GPIO matrices, drive USB directly, or own async runtimes. Instead, it defines behavior and asks the embedding firmware to implement the side effects through callback traits.

## High-Level Architecture

At runtime, the main data flow is:

1. Hardware produces `KeyEvent`s.
2. Firmware forwards them into `LayoutManager`.
3. `LayoutManager` either consumes them for mode switching or forwards them to the active mode handler.
4. The mode handler emits actions through `LayoutActions`:
   - `send_key(...)`
   - `send_raw_steno(...)`
   - mode indicator updates
5. In steno modes, raw strokes go to the steno dictionary layer (`bbq-keyboard::dict::Dict` plus `bbq-steno`).
6. Steno translations become host typing edits, while non-steno modes emit `KeyAction`s directly.

This split keeps keyboard behavior pure-ish and testable:

- `bbq-keyboard` interprets events and decides what should happen
- the firmware crate decides how to physically perform those actions

## Core Types

The crate root defines the shared vocabulary used by the rest of the system.

### `Side`

`Side` identifies left vs right hardware half. It is used in:

- board metadata
- inter-half protocols
- taipo side-local state

### `KeyEvent`

`KeyEvent` is the normalized key-scanner output:

- `Press(u8)`
- `Release(u8)`

The `u8` is a logical key index, not a hardware pin number. Board-specific firmware is expected to translate hardware events into this stable logical numbering before passing them into the crate.

### `KeyAction`

`KeyAction` is the abstract host action emitted by layout logic:

- `KeyPress(Keyboard, Mods)`
- `ModOnly(Mods)`
- `KeyRelease`
- `KeySet(Vec<Keyboard>)`
- `Stall`

The modes use different variants depending on their semantics:

- qwerty/NKRO emit `KeySet` snapshots
- taipo and some steno helpers emit press/release style actions
- modifier-only transitions use `ModOnly`

### `Event`

`Event` is a broader firmware event enum. It carries:

- matrix and interconnect key changes
- USB state changes
- role changes in split operation
- steno raw mode/state updates
- LED sync signals
- periodic tick notifications

This crate is in the middle of moving away from event-queue-driven internals toward more direct callback-based APIs. As a result, `Event` is still important at crate boundaries even though `LayoutManager` increasingly uses direct traits.

### Traits

- `EventQueue`: lossy push-based event sink used by some integration points
- `Timable`: simple clock source for components that need timestamps
- `layout::LayoutActions`: callback interface through which layout code performs side effects

`LayoutActions` is the most important abstraction. It lets the layout engine remain independent of the async runtime, HID implementation, LED code, and steno transport queue.

## Layout Engine

The `layout/` module is the behavioral center of the crate.

It has one top-level coordinator, `LayoutManager`, and four mode-specific decoders:

- `qwerty`
- `steno`
- `taipo`
- `artsey`

### `LayoutManager`

`LayoutManager` owns one instance of each mode handler plus a `ModeSelector`.

Its responsibilities are:

- maintain the current global mode
- distribute ticks to mode handlers
- route `KeyEvent`s to the active mode
- allow the special mode-selection key to intercept events
- keep taipo available as an escape/layer mechanism while in steno modes

This is intentionally not implemented as a trait object over “layouts”. The modes are heterogeneous and interact in non-trivial ways, especially steno plus taipo, so the manager coordinates them explicitly.

### `LayoutMode`

Supported modes are:

- `StenoDirect`
- `Steno`
- `Artsey`
- `Taipo`
- `Qwerty`
- `NKRO`

Current cycling behavior depends on whether the keyboard is a two-row or three-row design. Two-row boards skip normal qwerty in the direct cycle.

### Mode Selection

Mode changes are managed centrally by `ModeSelector`.

Design details:

- key `2` is the dedicated mode key
- pressing it by itself cycles the mode
- holding it while pressing certain keys selects a specific mode
- while selecting, lower mode handlers do not receive the intercepted events
- releasing the special taipo keys by themselves toggles between `Steno` and `Taipo`

This centralization matters because mode selection has to work regardless of the currently active layout semantics.

## Mode Handlers

Each mode handler translates logical key events into `LayoutActions`, but each uses a different model of time and key grouping.

### Steno: `layout/steno.rs`

`RawStenoHandler` converts key presses into `bbq_steno::Stroke`.

Behavior:

- it maps logical keyboard positions to steno bits with static `stroke!()` tables
- it tracks a chord in progress in `down`
- it emits the stroke on first release, not only after all keys are up
- it keeps collecting/releases state so additional presses after first release can begin a new stroke

This is effectively a “first-up” or hybrid steno collector rather than pure “all-up” steno.

It also cooperates with taipo:

- designated taipo keys are tracked separately
- if a chord is determined to be taipo-only, the steno stroke is suppressed

`RawStenoHandler` does not perform dictionary lookup. It only emits raw strokes via `LayoutActions::send_raw_steno`.

### Qwerty and NKRO: `layout/qwerty.rs`

`QwertyManager` handles traditional keyboard semantics with two extensions:

- layer shifts
- combo keys

#### Layer model

Static mapping tables describe how each logical key maps to:

- `Dead`
- `Key(KeyMapping)`
- `LayerShift(Layout)`

Pressing a layer-shift key swaps the active map while it is held; releasing it returns to the root map.

#### Combo model

Combos are resolved before key meanings are interpreted.

Behavior:

- some physical keys are marked as combo-capable
- a candidate key press is held briefly
- if a matching partner key arrives within the timeout, the pair becomes a synthesized combo keycode
- otherwise the original pending key is emitted as a normal key

This matters because combo handling happens at the physical-key level, not at the post-layer logical-key level. That keeps the combo logic simpler and avoids certain stuck-key cases, but also means combos are tied to hardware positions.

#### Output model

Unlike the other layouts, qwerty emits `KeyAction::KeySet(Vec<Keyboard>)`, effectively describing the complete currently-held HID set. That is a good fit for normal keyboard behavior and modifier folding.

`NKRO` reuses the qwerty machinery with a simpler mapping path that bypasses combo/layer behavior.

### Taipo: `layout/taipo.rs`

Taipo is a symmetric chorded keyboard layout where each half can operate independently and rollover between halves is allowed.

Design structure:

- one `SideManager` per half
- a shared queue of synthesized taipo events
- a second interpretation stage that maps taipo codes to `KeyAction`

Each side:

- collects a local chord for a short timeout window
- emits a press/release pair for the resolved taipo symbol when the chord completes
- can overlap in time with the other side, enabling cross-hand rollover

Modifier behavior is intentionally different from qwerty:

- modifiers are “one-shot” by default
- pressing modifier chords sends them immediately
- the next non-modifier key uses them, then they are released
- special null/release chords can clear them
- repeated use can make them sticky

Taipo also has a special role in steno modes:

- taipo thumb keys can latch taipo behavior temporarily
- while in `Steno`, taipo output is suppressed unless the taipo latch is active

This makes taipo usable as an auxiliary escape/input layer while keeping steno as the main interpretation mode.

### Artsey: `layout/artsey.rs`

Artsey is another compact chorded layout, implemented on 8 logical keys per side.

Key ideas:

- normal output is chord-based
- some corner keys can enter hold modes after a timing threshold
- one-shot modifiers affect the next real key
- “sticky” modifiers are supported for interactions like mouse gestures
- a nav submode exists and is surfaced through `MinorMode`

Compared with taipo, artsey is more modeful and more dependent on timing thresholds and hold interpretation. Its implementation is correspondingly state-heavy.

## Steno Integration

The crate’s steno integration lives in `dict.rs`, but it is not itself the steno engine. It is a thin runtime wrapper around `bbq-steno`.

`bbq-keyboard::dict::Dict` owns:

- a `bbq_steno::dict::Lookup`
- a `bbq_steno::dict::Joiner`
- a raw-mode toggle

Behavior:

- load main and user dictionaries directly from fixed flash addresses using `MemDict::from_raw_ptr`
- feed incoming strokes into `Lookup`
- pass lookup results into `Joiner`
- return the resulting `Joined` typing edits
- support a special raw toggle stroke (`RA*U`) that bypasses translation

This layer exists because firmware wants a keyboard-oriented interface:

- consume one `Stroke`
- optionally notify the rest of the firmware about raw-mode/state changes
- produce a short sequence of typing operations

It keeps the flash-address policy and keyboard integration out of `bbq-steno` itself.

## Transport and Split Keyboard Support

The crate currently contains two inter-half protocol modules.

### `serialize.rs`

This is the older byte-oriented packet protocol.

Characteristics:

- compact custom framing
- side-tagged packets
- CRC16 integrity
- packets for idle, primary, and secondary roles
- secondary packets carry key bitmaps
- primary packets carry LED state

This protocol is optimized for small packets and explicit framing logic.

### `ser2.rs`

This is a newer protocol attempt based on CBOR payloads plus framing/integrity supplied by the `minder` serial layer.

Characteristics:

- structured packet format with optional fields
- role, side, keys, and LEDs encoded with minicbor
- intended to be robust to dropped packets
- designed so idle time can be cheap and most state can simply be mirrored/acked

Both protocols are still in tree. That indicates an active migration or experimentation phase rather than a fully converged transport architecture.

## Board Metadata

`boardinfo.rs` defines `BoardInfo`, a small CBOR structure stored at a fixed flash location.

It currently carries:

- board name
- optional physical side

The firmware uses this to discover identity and split-role information at boot.

As with the steno flash dictionaries, decoding from memory is intentionally unsafe and assumes a trusted fixed flash layout.

## USB Typing Helper

`usb_typer.rs` is a utility layer for turning plain text into HID key actions.

It is not used by the layout engine directly. Instead, it is used by firmware-side code that needs to type arbitrary text, especially steno translation output.

Design:

- ASCII lookup table from character to HID key plus optional shift modifier
- `ActionHandler` trait abstracts the actual enqueueing of key actions
- `enqueue_action()` emits a sequence of `KeyAction`s, inserting releases as needed

This is intentionally limited:

- mostly ASCII
- no full Unicode composition
- focused on “type this text through HID” rather than “be a complete text input method”

That is appropriate for the firmware use case.

## Utility Modules

### `keys.rs`

Defines stable logical key indices for the proto3-style keyboard numbering scheme and related aliases. This is primarily documentation/convenience around the logical scan-code space.

### `modifiers.rs`

Contains a small steno-based modifier helper inspired by Emily’s modifier dictionary. It appears to be a focused utility rather than part of the main runtime path.

## Feature Model

Important cargo features:

- `std`: enables std-only conveniences such as `clap` integration
- `proto2` / `proto3`: select keyboard geometry-specific mappings
- `log` / `defmt`: select logging backend

The crate is designed for `no_std`, but many tests and some utilities assume `std`.

Board geometry is not abstracted dynamically. Instead, compile-time features select different key maps and layout constants. That keeps runtime overhead low at the cost of needing separate builds for different board generations.

## Testing Status

Current direct coverage includes:

- taipo side-manager behavior
- end-to-end-ish `LayoutManager` mode switching and basic qwerty/taipo flow
- old `serialize` packet protocol round-trip
- `ser2` packet round-trip

Important areas with limited or no direct tests:

- qwerty combo and layer edge cases
- artsey behavior
- steno raw collection in `RawStenoHandler`
- `bbq-keyboard::dict::Dict` integration with flash-loaded dictionaries
- `usb_typer` behavior

The existing test set validates several architectural seams, but layout coverage is still uneven.

## Design Constraints and Tradeoffs

### Callback-based side effects

Using `LayoutActions` instead of embedding transport or HID code has clear benefits:

- mode logic is testable
- firmware integration stays flexible
- async runtime details do not leak into keyboard behavior

The cost is that some behavior is spread across crate boundaries, so understanding full runtime flow requires reading both this crate and the embedding firmware.

### One crate, multiple concerns

`bbq-keyboard` currently combines:

- layout behavior
- steno integration
- transport protocols
- board metadata
- USB text typing helpers

This is pragmatic for a firmware project, but it means the crate boundary is broader than a strict “layout engine only” design would choose.

### Multiple input semantics

Supporting qwerty, steno, taipo, and artsey in one engine means there is no single universal key-processing model. The crate handles this by:

- centralizing mode selection
- keeping per-mode logic separate
- reusing only the minimum common interfaces

That is the right tradeoff here. Trying to force all modes into one generic abstraction would likely make the code harder to reason about.

## Technical Debt and Open Questions

### Two transport protocols

Both `serialize.rs` and `ser2.rs` are active enough to keep and test. Until one becomes canonical, the crate has duplicate protocol surface area and duplicated maintenance cost.

### Mixed event/callback architecture

Parts of the crate still speak in terms of `Event` and `EventQueue`, while the layout core increasingly uses direct callbacks. The codebase is clearly mid-transition.

This is workable, but it means a design doc should treat `Event` as integration glue, not as the sole internal architecture.

### Steno mode naming

`LayoutMode::Steno` and `LayoutMode::StenoDirect` are both present, while `bbq-keyboard::dict::Dict` also has a separate raw-mode toggle stroke. The conceptual distinction is understandable in code, but easy for a reader to confuse:

- raw stroke capture at the layout level
- raw translation bypass in the dictionary layer

That is worth documenting carefully if user-facing mode descriptions are added later.

### Static mapping tables

Large static tables in qwerty, taipo, artsey, and steno make the code efficient and explicit, but hard to audit. This is a reasonable firmware tradeoff, though better generated documentation or comments around the intended physical layout would help.

## Suggested Next Documentation Work

If this document is expanded later, the highest-value additions would be:

- a worked example showing one `KeyEvent` stream through `LayoutManager`
- mode transition tables for two-row vs three-row boards
- a visual map of logical key indices for `proto2` and `proto3`
- qwerty combo timing and layer examples
- taipo and artsey user-level semantics with diagrams
- a note declaring whether `serialize` or `ser2` is the intended long-term protocol
