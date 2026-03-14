# jolt-embassy-rp Design

## Status

This document is based on source inspection, not build or runtime verification.

This crate currently depends on locally patched `embassy-*` crates under `../embassy`, and those are not present in the current workspace snapshot. Because of that, I did not build or run tests for `jolt-embassy-rp`. Any statements here about runtime behavior are derived from the code structure and task wiring rather than direct execution.

## Purpose

`jolt-embassy-rp` is the current RP2040 firmware implementation for the Jolt keyboards.

Its job is to take the policy crates:

- `bbq-keyboard`
- `bbq-steno`
- `minder`

and bind them to actual RP2040 hardware using Embassy.

In concrete terms, this crate is responsible for:

- boot and executor setup
- board-specific peripheral initialization
- matrix scanning
- split-half communication
- USB HID and vendor interface setup
- LED driving and LED policy integration
- steno translation task scheduling
- firmware maintenance operations such as reset and flash programming

## Architectural Position

This crate is not where keyboard semantics live.

- `bbq-keyboard` defines layout behavior and host key actions
- `bbq-steno` defines stroke parsing and dictionary translation
- `jolt-embassy-rp` provides the async runtime, devices, and task graph that make those behaviors real on RP2040 hardware

That separation is visible throughout the code:

- layout code is called through `bbq_keyboard::layout::LayoutActions`
- steno translation runs in a dedicated task using `bbq_keyboard::dict::Dict`
- matrix scanning emits normalized `bbq_keyboard::KeyEvent`
- USB receives abstract `bbq_keyboard::KeyAction`

## High-Level Runtime Model

The firmware starts in `main.rs`, initializes the board and executors, and then assembles a set of Embassy tasks connected by channels.

The main data flow is:

1. The board-specific `Matrix` scanner detects key transitions.
2. `Dispatch` receives those `KeyEvent`s.
3. If the board is active/primary, `Dispatch` feeds them into `bbq-keyboard::LayoutManager`.
4. `LayoutManager` emits:
   - direct `KeyAction`s for qwerty/taipo/artsey
   - raw steno `Stroke`s for steno modes
5. Raw steno strokes go over a channel to `steno_task`.
6. `steno_task` uses `bbq_keyboard::dict::Dict` and `bbq-steno` to produce `Joined` typing edits.
7. `typed_loop` converts those edits into USB HID keystrokes.
8. USB task sends HID reports to the host and also runs the vendor-specific Minder interface.

In parallel:

- split-half communication tasks synchronize the other side’s keys and LED state
- LED tasks animate indicators
- a heap-stat task periodically logs allocator usage

## Boot and Task Topology

### Boot Sequence

`main()` performs the following:

1. initialize logging
2. initialize the heap
3. install RP2040 stack guard support
4. initialize Embassy RP peripherals
5. read `BoardInfo` from a fixed flash location
6. derive a unique device identifier from flash unique ID plus board name
7. create the high-priority interrupt executor and low-priority thread executor
8. construct the board-specific hardware bundle with `Board::new(...)`
9. create channels for:
   - generic events
   - raw steno strokes
   - translated steno typing actions
10. create `Dispatch`
11. on active boards, spawn the low-priority steno task

### Executors

There are two executors:

- high-priority interrupt executor
- low-priority thread-mode executor

The intended split is:

- time-sensitive I/O and dispatch tasks run on the high executor
- steno dictionary lookup runs on the low executor because it is computationally heavier and less latency-critical than matrix scanning or USB service

This is one of the most important design choices in the crate. The code explicitly comments that typical steno lookup is around 1 ms on RP2040, which is enough to justify isolating it from the higher-priority I/O path.

## `Dispatch`: Central Runtime Hub

`dispatch.rs` is the central coordinator for the live firmware.

It owns:

- LED state manager
- optional `LayoutManager`
- inter-half transport handle
- optional USB output handle
- channels to the steno task and typed-output task
- current layout mode and steno raw-mode state

### Why `Dispatch` exists

The crate needs one object that can play several roles simultaneously:

- `MatrixAction` sink for scanned key events
- `LayoutActions` implementation for layout output
- holder of shared state for LEDs and mode
- router between matrix/layout, USB, interconnect, and steno tasks

`Dispatch` is that object.

### Spawned Tasks

`Dispatch::new()` starts the high-priority runtime graph:

- `matrix_loop`
- `led_loop`
- `layout_loop` if this side is active
- `event_loop` if this side is active
- `typed_loop` if this side is active
- interconnect active-side input loop for I2C or UART when relevant

Passive boards do not instantiate `LayoutManager` and therefore do not run layout or steno-typing loops. They only scan local keys and forward them to the active side.

### Behavioral Split

`Dispatch` handles three classes of output:

- direct host key output via USB channel
- raw steno strokes via stroke channel
- LED indication updates

That makes it the seam where high-level keyboard policy becomes actual firmware side effects.

## Matrix Scanning

`matrix.rs` implements keyboard matrix scanning for a fixed row/column matrix.

### Model

- columns are actively driven
- rows are read
- each key has a per-key debouncer
- scan codes are translated through a board-specific translation function
- right-side boards bias logical indices into the upper half of the global key space

### Idle Optimization

The scanner has two phases:

- idle wait: all columns are asserted and the task waits on row interrupts/futures
- active scan: once activity is detected, the task scans every millisecond until the matrix has been idle long enough

This is a good fit for battery/performance-conscious embedded firmware:

- no constant busy scan when idle
- once activity starts, scan latency becomes predictable

### Debouncing

Each key tracks:

- stable pressed/released state
- a debounce transition target
- a sample counter

Events are emitted only after `DEBOUNCE_COUNT` stable observations. This keeps matrix noise out of the higher-level layout engine.

## Board Abstraction

`board.rs` is the hardware factory for supported boards.

It turns `(Peripherals, BoardInfo, unique ID)` into a normalized `Board` containing:

- `Matrix`
- `LedSet`
- `Inter`
- optional `UsbHandler`

### Supported board families

The current code supports:

- `jolt3` left and right
- `jolt2` right
- `jolt2dir` left

Any other board identity panics at boot.

### Why `Board` is normalized

The higher layers do not want to know whether split communication is I2C or UART, or whether the board has local USB. `Board` hides those choices behind:

- `Inter`
- optional USB handle
- a normalized matrix scanner
- a normalized LED set

That allows `Dispatch` to stay mostly board-agnostic.

## Board-Specific Variants

### `jolt3`

`jolt3` uses:

- matrix scanner
- WS2812 LED strip via PIO
- I2C-based split communication
- USB on the left side only

The left side is active:

- hosts USB
- polls the right side over I2C
- runs layout and steno processing

The right side is passive:

- scans local matrix
- serves local key state over I2C slave
- receives LED commands from the left side

### `jolt2`

`jolt2` right side uses:

- matrix scanner
- UART-based passive synchronization
- no USB
- effectively no active LED behavior in the current code path

### `jolt2dir`

`jolt2dir` left side uses:

- matrix scanner
- WS2812 LED strip via PIO
- USB
- UART-based active synchronization

This board appears to be the direct-RP2040 counterpart to the older `jolt2` split arrangement.

## Scan-Code Translation

`translate.rs` maps board-specific physical scan order into the canonical logical key numbering expected by `bbq-keyboard`.

This is important because:

- `bbq-keyboard` assumes a stable logical key layout
- physical board wiring varies between board revisions

The translation layer keeps those concerns separate:

- board modules define GPIO wiring and matrix geometry
- translation table normalizes scan order
- layout logic remains untouched

## Layout Integration

Active sides instantiate `bbq_keyboard::layout::LayoutManager` inside `Dispatch`.

The runtime model is:

- matrix events call `Dispatch::handle_key`
- active sides lock the `LayoutManager` and call `handle_event`
- `layout_loop` also calls `tick` every 10 ms

`Dispatch` implements `LayoutActions` so layout output becomes firmware effects:

- `set_mode` and `set_mode_select` update LED indicators
- `send_key` forwards `KeyAction` to USB
- `send_raw_steno` forwards `Stroke` to the steno task

This is a clean boundary:

- `bbq-keyboard` stays hardware-agnostic
- `jolt-embassy-rp` decides how LED, USB, and steno channels are physically realized

## Steno Tasking

Steno translation is deliberately offloaded from the main dispatch path.

### Channel topology

- `LayoutActions::send_raw_steno` sends `Stroke` into `STROKE_QUEUE`
- `steno_task` receives strokes and runs dictionary translation
- output `Joined` actions are sent into `TYPED_QUEUE`
- `typed_loop` converts those actions into USB key events

### `steno_task`

`steno_task` owns one `bbq_keyboard::dict::Dict`, which itself wraps `bbq-steno`.

Its responsibilities:

- consume strokes
- apply lookup and join rules
- emit translated typing edits
- publish steno-state changes back into the generic event queue

This task also publishes the current steno join state as `Event::StenoState`, which is used by the LED layer to reflect spacing/capitalization state.

### `typed_loop`

`typed_loop` is the bridge from logical steno edits to actual HID actions.

For each `Joined::Type { remove, append }`:

- send backspace press/release pairs for `remove`
- feed `append` into `bbq_keyboard::usb_typer::enqueue_action`

That means the firmware does not send text directly. It simulates host keyboard entry for every translated edit.

## USB Subsystem

`usb.rs` sets up two USB functions:

- HID keyboard interface
- vendor-specific bulk interface for Minder

### HID path

The HID task receives `KeyAction` values from a channel and converts them into `KeyboardReport`s.

Supported action shapes:

- `KeyPress`
- `KeyRelease`
- `ModOnly`
- `KeySet`
- `Stall`

`KeySet` is folded into a standard 6-key boot keyboard report plus modifier bits. Extra keys beyond 6 are silently ignored, which is a deliberate limitation of the boot-keyboard-style report format used here.

### Vendor interface

A second interface exposes two bulk endpoints used by `minder.rs`.

This is the device-management channel for:

- hello/version exchange
- hash requests
- flash programming
- reset requests

This cleanly separates normal typing traffic from maintenance/control traffic.

## Minder Maintenance Protocol

`minder.rs` implements the device side of the Minder protocol over USB bulk endpoints.

Current capabilities include:

- `Hello`
- `Reset`
- `Hash`
- `Program`

The code also has a placeholder for flash reads.

### Flash handling

Minder directly uses blocking flash operations on RP2040 XIP flash.

The code explicitly acknowledges the tradeoff:

- erase/write block interrupts and stall normal activity
- but these are user-initiated maintenance operations, so temporary disruption is acceptable

This is a pragmatic firmware design rather than a fully background-safe OTA/update subsystem.

## Split-Half Communication

The crate supports two different split transport designs.

### I2C protocol: `inter.rs`

Used for `jolt3`.

Roles:

- active side periodically requests passive-side key state and pushes LED state
- passive side responds to requests and asserts an IRQ line when local key state changes

Protocol messages are small custom binary packets with CRC16.

Main operations:

- `Hello`
- `SetLeds`
- `ReadKeys`

This protocol is strongly asymmetric, which fits the `jolt3` hardware:

- left side owns USB and overall behavior
- right side acts mostly as a remote key/LED peripheral

### UART sync protocol: `inter_uart.rs`

Used for `jolt2`/`jolt2dir`.

This is a more symmetric replicated-state design:

- local and remote state are combined into a shared packet
- packets are COBS-framed
- the generic `BiSync` engine handles retransmit and freshness tracking

Data synchronized:

- passive-side key bitmap
- two RGB LED values

`BiSync` is the more reusable abstraction in the crate. It could plausibly outlive the current board set if more synchronous side-to-side channels are added later.

## LED System

LED handling is split into three layers:

- `leds.rs`: generic grouping abstraction
- `leds/led_strip.rs`: concrete WS2812 transport
- `leds/manager.rs`: animation/indicator policy

### `LedSet`

`LedSet` is a collection of one or more LED groups. It provides a normalized `update(&[RGB8])` interface regardless of the underlying transport.

### `LedManager`

`LedManager` is the stateful policy engine.

Each LED has:

- a base indication
- an optional global override
- an optional oneshot animation
- current phase/count tracking

It is tick-driven and expected to run about every 100 ms.

### Semantic use

The current firmware uses LEDs to communicate:

- startup/init state
- selected layout mode
- mode-selection preview
- steno raw mode
- steno spacing/capitalization state
- other-side override status

This is tightly integrated with `Dispatch`, which updates LED state in response to layout and steno events.

## Logging and Build Metadata

The crate supports two logging modes:

- `defmt`
- `log`

`build.rs` also injects a `BUILD_ID` environment variable based on the current timestamp and copies `memory.x` into the linker search path. That means every build gets a unique build identifier, which the firmware logs at startup.

## Memory and Allocation

This firmware uses dynamic allocation.

Key points:

- global allocator is `embedded_alloc::LlffHeap`
- heap size is statically configured to 65535 bytes
- several Embassy USB/HID structures are heap-allocated or boxed for convenience
- a periodic task logs heap usage

This is a deliberate choice toward implementation simplicity. The code is not written as a fully allocation-free firmware image.

## Concurrency Model

The concurrency model is mostly message-passing plus a few shared mutex-protected state objects.

### Channels

Main channels include:

- generic event queue
- raw steno strokes
- translated steno actions
- USB key actions
- active-side interconnect key events

### Shared state

Shared mutable state is held in:

- `Mutex<CriticalSectionRawMutex, ...>`
- `Signal<CriticalSectionRawMutex, ...>`
- `StaticCell`-allocated singletons

The code is intentionally conservative:

- static allocation for long-lived task resources
- channels for cross-task sequencing
- mutexes around stateful managers like LEDs and layout

## Testing and Verification Status

I did not run build or tests for this crate because the required patched Embassy checkout under `../embassy` is absent in the current workspace.

From source inspection:

- there do not appear to be standalone crate-local tests under `jolt-embassy-rp`
- confidence therefore mainly comes from architecture readability and the lower-level crates (`bbq-keyboard`, `bbq-steno`, `minder`) having their own tests

This makes `jolt-embassy-rp` a relatively integration-heavy crate with comparatively low direct verification in the current repository snapshot.

## Design Constraints and Tradeoffs

### Integration-first design

This crate prioritizes getting all subsystems wired together over producing a minimal or perfectly layered firmware architecture. That is sensible for an embedded integration crate.

### Static board specialization

Board support is explicit and compile-time/static:

- GPIO pinouts are hard-coded per board module
- scan-code translation tables are hard-coded
- transport choice is hard-coded by board family

This keeps runtime cost low, but adding new boards requires code changes instead of data-only configuration.

### Active/passive split

Only the active half runs layout and steno translation. Passive halves are intentionally simpler.

Benefits:

- one authority for USB and higher-level behavior
- less duplicated work
- simpler passive firmware state

Costs:

- stronger dependence on inter-half link health
- asymmetry between board halves

### Coexistence of two split protocols

The crate currently supports both:

- custom I2C polling/IRQ protocol
- UART replicated-state sync

That is pragmatic for hardware evolution, but it also means the integration crate carries historical transport complexity.

## Technical Debt and Open Questions

### `Event` queue legacy

`main.rs` and `Dispatch` still carry a generic `Event` queue mainly for steno-related status propagation. The code comments already note that this should go away in favor of more direct callbacks.

### `StenoDirect` handling

`Dispatch::set_mode` and `set_mode_select` still have `todo!()` branches for `LayoutMode::StenoDirect`. That suggests this mode is not fully integrated at the firmware indicator layer.

### Board coverage

Only a small set of boards are directly supported in `Board::new`, with rigid name/side matching from flash board info. This is fine for a personal firmware tree, but brittle if board proliferation continues.

### USB HID limitations

The HID report path uses a standard 6-key boot-keyboard-like report. Any `KeySet` larger than six non-modifier keys is truncated. That is probably acceptable for current layout behavior, but it is a built-in ceiling.

### Flash maintenance safety

Minder programming uses blocking flash erase/write during normal runtime. That is explicit and probably acceptable, but it means maintenance operations are not isolated from active firmware tasks in a sophisticated way.

## Suggested Next Documentation Work

If this document is expanded later, the most useful additions would be:

- a task graph diagram showing channels and executors
- per-board tables of transport, USB role, and LED hardware
- a split-half sequence diagram for `jolt3` I2C polling
- a note clarifying intended long-term direction between I2C and UART split protocols
- a short boot-memory map note tying together `memory.x`, board info location, and dictionary flash regions
