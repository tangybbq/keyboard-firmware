# Bringing the Zephyr port (`jolt`) up on the mesa keyboards

This is a work plan for making the Rust-on-Zephyr firmware in this tree run
the mesa line, with Dosh as the layout, so that it can replace
`jolt-embassy-rp` as the daily driver and become the base for a future
split, wireless design.  It is the alternative to `port-zmk.md`, which stays
as written; the reason for preferring this route is at the end of that
document's story: ZMK is still on Zephyr's legacy USB stack and on Zephyr
4.1, while `jolt` is already on the new USB stack against Zephyr main.

Read this whole document before starting.  The facts in "What was found"
were read out of the tree, the `~/zephyrproject` workspace and one build
attempt, not remembered; the derivation is given so it can be re-checked.

Status: **plan only.  `jolt` does not currently configure, for reasons given
below; nothing has been changed.**

---

## Requirements (from the developer, verbatim)

- I hadn't realized that ZMK was still on the old USB stack.  This is kind of
  a wet blanket on my ideas of supporting ZMK and am wondering if I should
  just get the jolt on zephyr working again.  There is work on rust bindings,
  esp for I2C, and we'd have to figure out quite a bit on how to do a split
  wireless.
- Let's go ahead and leave this plan file, and make a new one to give a plan
  to get the latest zephyr port in this tree working on mesa.  I'm not even
  sure if that is jolt or another dir here that works with the rust on
  zephyr work.
- (Carried over from `port-zmk.md`.)  Initial focus is Dosh only, on the mesa
  line, starting on the mesa1 while the mesa3 is in production; the single
  mesa2 is the daily driver.  Longer term, a split wireless design.

---

# What was found

## Which directory is the port

**`jolt/`.**  Three directories look like Zephyr ports; only one is alive:

| dir | what it is | last real change |
|---|---|---|
| `jolt/` | The port.  Rebuilt from scratch in February 2026 ("Create new 'clean' jolt framework", `5b6fa64`; the previous incarnation went to `archive/jolt`).  146 commits.  Built on `zephyr-lang-rust`, the embassy executor on Zephyr threads, and the **new** USB device stack. | 2026-03-17 (`a63ee12`).  Later commits (`030403f`, `277f20f`, `e796461`) are ripples from `bbq-keyboard` changes, not port work. |
| `zbbq/` | 2024 lineage with its own hand-written bindings (`zephyr.rs`, `wrappers.c`, `gpios.c`), from before `zephyr-lang-rust` existed. | 2024-06-19 (the 2026-08-28 touch is the steno feature gate). |
| `zephyr-play/` | A 2024 rename of an even earlier `zbbq`. | 2024-03-06. |

`zbbq/` and `zephyr-play/` are candidates for `archive/`; nothing below uses
them.

## What `jolt` is, today

- **Layout**: `jolt/CMakeLists.txt` is a Zephyr app calling
  `rust_cargo_application()` and adding two C files; `jolt/Cargo.toml` is the
  `rustapp` staticlib depending on `zephyr` (features `time-driver`,
  `executor-zephyr`), embassy-executor 0.7 / embassy-sync 0.6 / embassy-time
  0.4, and `bbq-keyboard` with `proto3, log, qwerty, steno`.  `.cargo/config.toml`
  is a symlink to `build/rust/sample-cargo-config.toml`, which the Zephyr
  configure step generates — so **cargo cannot run at all until `west build`
  has configured once.**
- **Boards**: `jolt/bbqboards/` is a Zephyr module (`board_root`, `dts_root`)
  with shields `proto2`, `proto3`, `proto4`, `jolt1`, `jolt2`, `jolt2dir`,
  `jolt3_mez_2040`, `highboard`, and bindings `bbq,keyboard-matrix` (a copy of
  `gpio-kbd-matrix` with extra properties; `jolt` only reads its `row-gpios`
  and `col-gpios`), `bbq,board-info`, `bbq,gpio-mode-selector`.  The build
  scripts `b-proto4.sh` and `b-jolt3-mez-2040.sh` target **`-b tiny2040`**,
  which upstream Zephyr has had since v4.4.0.
- **USB** (`src/usb.c`, `src/usbd_support.c`): `CONFIG_USB_DEVICE_STACK_NEXT=y`,
  `CONFIG_USBD_HID_SUPPORT=y`; a `zephyr,hid-device` node in `app.overlay`
  (8-byte boot keyboard report, 1 ms polling), the descriptor from
  `HID_KEYBOARD_REPORT_DESC()`, and device setup copied from Zephyr's
  `sample_usbd` (`jolt/Kconfig` sources `samples/subsys/usb/common/Kconfig.sample_usbd`,
  which still exists on main).  Reports are queued from Rust through a
  channel and an in-flight semaphore, with a generation counter so reports
  from before a USB reset are dropped.  Serial number comes from `hwinfo`
  when `CONFIG_HWINFO` is on (RP2040's hwinfo is the flash unique ID — the
  same 8 bytes `jolt-embassy-rp` derives its `unique` string from).
- **Startup** (`src/lib.rs`): reads the board-info CBOR block from the
  chosen `board-info` flash node at `0x101fff00` — the same address
  `jolt-embassy-rp/memory.x` and `bbq-tool/sides.sh` use, and both mesas
  already carry a block (`sides.sh` has `mesa1` and `mesa2` entries).
  Then `usb_setup()`, a steno thread, and the embassy executor on the main
  thread running `usb_sender_task`, `steno_event_task`, `led_task`, and
  `keyboard_task`.
- **Matrix** (`src/matrix.rs`): its own scanner — drives `cols`, reads `rows`
  with pull-downs, `k_busy_wait(5)` per column, 1 ms `Ticker`, 20-sample
  debounce.  Identical constants to `jolt-embassy-rp/src/matrix.rs`
  (`DEBOUNCE_COUNT = 20`), so key timing behaviour should match; the
  difference is that embassy idles on a GPIO interrupt after 500 ms and
  `jolt` scans forever, which does not matter on USB power.  The GPIO pins
  are pulled out of the `matrix` alias's **raw** devicetree properties
  (`RAW_COL_GPIOS`, `Value::Words`, `Word::Gpio`) and built with
  `GpioPin::raw_new` — an API that exists only on a `zephyr-lang-rust`
  feature branch (see below).
- **Key mapping** (`src/mapping.rs`): a local `PROTO4_MAPPING` that
  `TASKS.md` records as **wrong at scan indices 1, 24 and 29** against the
  hardware-verified table now in `bbq-keyboard/src/translate.rs`, and a
  `JOLT3_LEFT_MAPPING`.  `keyboard_task` picks the table by board-info name
  and panics on any other name; the mesas are "other names".
- **Layout**: `LayoutManager::new(two_row)` with the full three-layout
  engine, `manager.tick(&ACTION, 1)` per millisecond.  `Action` implements
  `LayoutActions` with the *old* LED scheme — a mode colour per `LayoutMode`
  on LED 0, steno state on LED 1, and `MinorMode::Dosh` mapped to "off" —
  and **does not implement `set_mod_state`**, so the modifier LEDs that are
  the whole of the embassy firmware's LED behaviour do not exist here.  No
  key log; no minder.  `LayoutMode::Qwerty`/`Steno` are referenced
  unconditionally, so a taipo-only `bbq-keyboard` (no `qwerty`, `steno`
  features) does not compile `jolt` as written.
- **LEDs** (`src/leds/`): a `LedGroup`/`LedSet` shaped like the embassy one,
  discovered from chosen nodes at compile time via `#[cfg(dt = "chosen::…")]`.
  `pwm.rs` drives the Tiny 2040's onboard RGB LED through
  `zephyr::device::led::Led` (the "Use Zephyr LED API for proto4 PWM LEDs"
  work, 2026-03-16).  **`led_strip.rs` is a stub** that reports zero LEDs.
  `manager.rs` is the old indication-sequence manager, not
  `jolt-embassy-rp/src/leds/manager.rs`'s one-modifier-per-LED one.
- **Stale bits**: `check.sh` sources `jolt/.envrc`, which moved to the
  repository root in `ef3cbe8`; `proto4/Kconfig.shield` defines
  `SHIELD_PROTO2` (copy-paste; harmless because nothing tests it).

## Why it does not build right now

`./jolt/b-proto4.sh` was run once for this document.  It fails at the
devicetree step:

```
proto4.overlay:50 (column 1): parse error: undefined node label 'pwm_leds'
```

Two things moved out from under the port since March:

1. **The Tiny 2040's PWM LED node was a local Zephyr patch, never
   upstreamed.**  `boards: pimoroni: tiny2040: add disabled pwm-leds support`
   (2026-03-16) exists on several of the fork's branches (`rust-wip`, and a
   handful of merges of the same commit), adding a disabled `pwm_leds` node,
   `pwm-led0..2` aliases and a `&pwm` pinctrl/divider block to
   `tiny2040.dts`.  `TASKS.md`'s "Upstream PWM led changes to Zephyr" is this
   patch.  `~/zephyrproject/zephyr` is now a detached HEAD at upstream `main`
   (2026-09-08), which does not have it, and neither does the fork's `main`
   (2026-01-28).  Every `bbqboards` overlay that says `&pwm_leds` fails the
   same way; the mesa overlays will not need it at all.
2. **`zephyr-lang-rust` is pinned to a commit from before every binding
   `jolt` uses.**  `west list` shows `modules/lang/rust` at `dd73abc`
   (2025-06-04), from `zephyr/submanifests/optional.yaml`.  That commit has
   `device/gpio.rs` and `device/flash.rs` and nothing else.  What `jolt`
   needs is on the developer's own PR stack in that same checkout:

   | branch | PR | adds | `jolt` uses it for |
   |---|---|---|---|
   | `davidb-dt-all-properties` | #146 | raw DT properties (`RAW_*`), GPIO detection in raw props, byte props, `GpioPin::raw_new` | matrix pins, `board_info` address |
   | `davidb-pwm-led` (on #146) | #148 | `device::led::{LedController, Led}` | `leds/pwm.rs` |
   | `davidb-led-strip` (on #148) | #149 | `device::led_strip::{LedStrip, LedRgb}` with `update_rgb(&mut [LedRgb])` and `length()`; an RP2040 test | the missing `led_strip.rs` backend |
   | `davidb-i2c` (on #149) | #150 | I2C controller `transfer()`, I2C target registration with safe callbacks | the future split link (`TASKS.md`: "Port … inter.rs … to jolt") |

   None of the four is merged; all are based on a March–April `main`.
   Upstream `main` has since gained (May–August) Clippy CI, the `__UINTxx_C`
   bindgen fix, **kconfig-gated device modules and selective binding
   exports** (`8c6a29d`, `a763400`) and the **`zephyr::blocking` module**
   (`5a41774`, 2026-08-13) that the module's `TASKS.md` re-plans async I2C
   on.  `davidb-embassy-upgrade` (`f36729b`, on top of that August `main`)
   moves the `zephyr` crate to embassy-executor **0.10** / embassy-sync
   **0.8** / time-queue-utils 0.3; `jolt/Cargo.toml` pins 0.7 / 0.6, and
   `jolt-embassy-rp` is on 0.8-era releases.  Whichever branch `jolt`
   builds against, its embassy versions must be the ones the `zephyr` crate
   was built with, or the executor feature fails to resolve.

   In short: the last configuration `jolt` was known to build in was
   `zephyr` at the fork's `rust-wip` (2026-03-27 base, with the PWM patch)
   and `zephyr-lang-rust` at `davidb-led-strip` or `davidb-i2c`.

Two more environment notes from the build log: the toolchain found was
**SDK 1.0.0** (Zephyr main requires it; the root `.envrc` putting
`zephyr-sdk-0.17.1` on `PATH` is harmless but misleading), and the
module's `wbuild.sh` sets `LIBCLANG_PATH` and `BINDGEN_EXTRA_CLANG_ARGS`
(resource dir) because bindgen otherwise mixes Xcode's and Homebrew's
clang headers on thumbv6m.  `b-proto4.sh` sets neither, and the failed
configure never reached cargo, so whether bindgen works from the jolt
scripts on this machine is untested.

## What the mesas need that `jolt` does not have

Against `jolt-embassy-rp`, which is the behaviour to match (same
`bbq-keyboard` engine underneath, so typing is identical by construction):

| feature | embassy | `jolt` today | needed for the mesa |
|---|---|---|---|
| board modules `mesa1`, `mesa2` | `board.rs` | — | yes: shields in `bbqboards` |
| scan-code translation | `bbq_keyboard::translate` by board name | local, wrong, panics on mesa names | yes: use `translate` |
| taipo-only build | default; `qwerty`/`steno` are cargo features | hard-coded full build | yes |
| 4 × SK6812 on GP26 | PIO + DMA driver, own task | stub | yes: `led_strip` binding |
| one modifier per LED, quarter brightness, `set_mod_state` | `leds/manager.rs` | old mode-colour manager, no `set_mod_state` | yes |
| USB HID keyboard | embassy-usb, custom 8-byte descriptor, 1 ms | new-stack `usbd_hid`, boot descriptor, 1 ms | have |
| serial number `name-` + 16 nibble letters | flash unique ID | hwinfo (same ID) — format differs | later, with minder |
| key log ring + minder vendor bulk interface | `keylog.rs`, `minder.rs`, `usb.rs` | — | later (`TASKS.md` phase 6) |
| board-info from flash | `_board_info` | chosen node, same address | have |
| consumer-control keys | pending | pending | later, both |
| Tiny 2040 onboard PWM LED | not supported | supported via the unmerged Zephyr patch | no; not on the critical path |
| inter-board I2C/UART (jolt3, jolt2) | `inter.rs`, `inter_uart.rs` | — | no (single-MCU boards) |

Matrix roles in `jolt`'s scanner: `Matrix::new(rows, cols)` **drives `cols`
and reads `rows`**, from the shield's `col-gpios` and `row-gpios`.  The mesa
tables from `port-zmk.md` (derived from `board.rs` and `docs/mesa2.md`)
translate as:

| shield | `col-gpios` (driven) | `row-gpios` (read, pull-down) | codes | translation |
|---|---|---|---|---|
| `mesa1` | `COL_1..6` = GP 27 28 29 7 6 5 | `ROW_A..E` = GP 4 3 2 1 0 | `col*5 + row`, 30 | `translate::mesa1` |
| `mesa2` (Rev A) | `ROW_A..D` = GP 28 27 0 1 | `COL_1..5` = GP 4 5 6 7 29 | `row*5 + col`, 20 | `translate::mesa2` |
| `mesa3` (Rev B / mesa3) | same pins as `mesa2` | same | 20 slots, 18 keys | a new `translate::mesa3`: the right hand's columns run pinky→index, the reverse of Rev A |

The naming swap on the mesa2 (the board's *rows* go in `col-gpios`) is the
one already documented in `board.rs`'s `mod mesa2`; the overlay must say so
in a comment or the next reader will "fix" it.  The mesa3's board-info name
and `translate` entry do not exist yet on either firmware; `mesa3-tasks.md`
requires that Rev B and the mesa3 be indistinguishable, so one name serves
both.

## The split-wireless question, honestly

The reason ZMK was attractive is that BLE split is its native mode.  On the
`jolt` route it has to be built:

- **BLE HID** on an nRF52840-class half: Zephyr's Bluetooth stack and HOG
  service are C; the pattern `jolt` already uses for USB (a C file exposing a
  handful of functions and callbacks to Rust) extends to `bt_*` and a HIDS
  characteristic without any Rust binding work.  A Zephyr `peripheral_hids`
  sample is the starting point.
- **The split link**: either the wired I2C/UART protocols `jolt-embassy-rp`
  already has (`inter.rs`, `inter_uart.rs`) on top of `davidb-i2c`'s
  controller/target bindings — the `TASKS.md` entry for that port already has
  its questions answered — or a BLE central/peripheral pair, which is a
  substantial C project.
- Everything in this plan is independent of that choice.  The Dosh engine,
  the LED manager, the USB layer and the scanner do not change when a second
  MCU or a radio appears; only a new shield and a link module do.  So doing
  the mesa work in `jolt` does not foreclose ZMK for a *later* board either
  — `port-zmk.md` remains usable as written — it just means the wireless
  design's firmware is a decision to make when that board is designed.

---

# The plan

Order matters: nothing after phase 1 can even be compiled until phase 1 is
done, and nothing can be flashed to a mesa until phase 2.

## Phase 1 — make `jolt` configure and build on today's workspace

Goal: `./jolt/b-proto4.sh` produces `build/zephyr/zephyr.uf2` against
`~/zephyrproject/zephyr` as checked out (upstream main) and a chosen
`zephyr-lang-rust` branch.  Test on the proto4 if it is still around, since
that is the last hardware `jolt` was seen on; otherwise this phase is
"builds and enumerates on USB".

1. **Pick the `zephyr-lang-rust` branch and pin it.**  Proposal: a new
   integration branch in `~/zephyrproject/modules/lang/rust`, `jolt-base`,
   made by rebasing the #146 → #148 → #149 → #150 stack onto `upstream/main`
   and then `davidb-embassy-upgrade` on top.  That is the branch the PRs want
   to be on anyway, and rebasing them is due regardless (upstream's
   kconfig-gated device modules will touch `device.rs`, where all four
   branches add their `pub mod`s).  The fallback if the rebase is a fight:
   check out `davidb-i2c` as-is and put `jolt` on embassy 0.7 — it is what
   `jolt` last built with.  Either way, record the commit in `jolt/README`
   or the build script, and point `zephyr/submanifests/optional.yaml` in the
   fork at it (or accept that `west update` must be told to leave
   `modules/lang/rust` alone).
2. **Match embassy versions** in `jolt/Cargo.toml` to the `zephyr` crate's
   (0.10 / 0.8 / 0.5.x with `jolt-base`), and regenerate `Cargo.lock`.
3. **Stop depending on the PWM patch.**  In `proto4.overlay` (and the other
   `bbqboards` overlays that reference `&pwm_leds`), either define the
   `pwm_leds` node and `&pwm` pinctrl in the shield overlay itself — it is
   an ordinary node, the board does not have to declare it — or drop the
   Tiny 2040 LED from `proto4` as the embassy firmware already has.  Proposal:
   define it in a shared `bbqboards/dts/tiny2040-pwm-leds.dtsi` that the
   overlays include, so the upstream patch stays a separate, optional
   contribution.  `build.rs`'s `chosen::bbq_pwm_leds` cfg keeps working.
4. **Environment**: fold `wbuild.sh`'s `LIBCLANG_PATH` /
   `BINDGEN_EXTRA_CLANG_ARGS` exports into the root `.envrc`, drop the
   0.17.1 `PATH` entry, and fix `check.sh` to source the root `.envrc`.
5. Commit in that order — lang-rust pin and Cargo versions; overlays;
   environment — each with what compiled.

## Phase 2 — mesa shields and translation

1. Delete `jolt/src/mapping.rs`; select the translation with
   `bbq_keyboard::translate::get_translation(board_name)` and derive
   `two_row` from the name as `jolt-embassy-rp/src/board.rs` does (or from
   the `bbq,two-row` chosen node the `proto4` overlay already sets — pick one
   and remove the other).  Unknown names: panic with the name, as now.
2. Add `bbqboards/boards/shields/mesa1/` and `mesa2/` from the table above,
   each with `Kconfig.shield`, `Kconfig.defconfig`, and an overlay carrying
   the matrix, `bbq,two-row`, `chosen board-info` (move it out of
   `app.overlay` if it is per-board; it is not), and the LED strip node from
   `jolt3_mez_2040.overlay` with `PIO0_P26`, `gpios = <&gpio0 26 …>`,
   `chain-length = <4>` (LEDs are wired in phase 4 but the node can land
   now).  Add `b-mesa1.sh`/`b-mesa2.sh`, or better one `b.sh <shield>`.
3. `mesa3`: a `translate::mesa3` table and shield once the Rev B / mesa3
   matrix is final; `sides.sh` gets `gen_files mesa3 --name mesa3`.  Same
   commit shape as the `docs/mesa2.md` part 1 commits.
4. Test on the mesa1 with the full-layout build (qwerty still available is
   useful for a first "does every key type" pass): every key, the mode key,
   the `#` key, the eight removed positions silent.  `printkln!` of raw and
   translated codes over RTT is the bring-up tool, as `just build-debug` is
   on the embassy side.

## Phase 3 — a taipo-only build

`rust_cargo_application()` passes no cargo features (the support is
commented out in the module's `CMakeLists.txt`), so the app crate's own
`[features]` decide.  Make `jolt`'s default features taipo-only, with
`qwerty` and `steno` features forwarding to `bbq-keyboard`/`bbq-steno`, and
`#[cfg(feature = …)]`-gate `lib.rs` the way `jolt-embassy-rp/src/dispatch.rs`
and `main.rs` do: the steno thread, `Event`/`Dict`, `send_raw_steno`, and
every `LayoutMode::Qwerty`/`Steno`/`NKRO` match arm.  `Action::new`'s
initial mode becomes `LayoutMode::Taipo`.  A full build stays one cargo
flag away for the jolt3.  (Whether to enable feature passthrough in
`rust_cargo_application()` upstream is a lang-rust question; not needed
here.)

## Phase 4 — LEDs

1. `leds/led_strip.rs`: real backend over
   `zephyr::device::led_strip::LedStrip`, obtained from
   `zephyr::devicetree::chosen::bbq_led_strip::get_instance()`; `len()` from
   `length()`, `update()` converting `RGB8` to `LedRgb`.  `update_rgb` is
   blocking (per-pixel PIO FIFO puts, then a 280 µs `k_usleep` for the
   reset — with 4 LEDs and the joined 8-word FIFO the whole frame is queued
   before the PIO finishes the first pixel, so there is no underrun to
   worry about; Zephyr main's driver also has DMA now, optional via
   `dmas`).  Run it off the scanner's thread: a small `#[zephyr::thread]`
   fed by a signal, mirroring `LedStripGroup::update_task` in the embassy
   tree, so a LED write can never delay a matrix tick.
2. Replace `leds/manager.rs` with a port of
   `jolt-embassy-rp/src/leds/manager.rs`: `MOD_LEDS`, `LATCH`, `set_mods`,
   the 100 ms rewrite `tick`; implement `set_mod_state` on `Action`; delete
   the mode/steno indications and the `set_mode`/`set_sub_mode` LED calls.
   Same colours, same quarter brightness (`d9ae0b2`).
3. `pwm.rs` stays behind its cfg for the proto4 and the jolt3; the mesa
   overlays do not choose it.
4. Test on the mesa1: the four LEDs follow one-shot/sticky exactly as on the
   embassy firmware; watch for flicker while typing.

## Phase 5 — live on the mesa2

Flash the mesa2 (`west build` → `zephyr.uf2` → `RPI-RP2`; a `just uf2`
equivalent for `jolt`) and use it.  Expected differences from embassy to
watch for: none in chording (same engine, same 1 ms tick, same debounce),
possibly USB report latency under the new stack's HID class, and boot time.
Anything that differs is a bug in this port, not a tuning question.  Keep
`jolt-embassy-rp` flashable as the way back; nothing about the flash layout
changes (board info at the same address).

## Phase 6 — key log and minder on the new USB stack

`TASKS.md` "Phase 6: port to `jolt` (Zephyr)" and the "Minder" bullets.
Needed for TaipoTeacher; not needed to type.

1. A vendor-specific USB function with one bulk IN and one bulk OUT
   endpoint, as a `usbd` class in C.  The new stack has no generic vendor
   class; `subsys/usb/device_next/class/loopback.c` is the template
   (`USBD_DEFINE_CLASS`, its own interface/endpoint descriptors, a
   request handler).  Expose `minder_read`/`minder_write` plus callbacks to
   Rust the way `usb.c` does for HID.
2. Port `keylog.rs` as-is (it is `embassy-sync` + a critical section, both
   available) and `minder.rs` with the endpoint traits replaced by the C
   functions; `ReadFlash`/`Hash`/`Program` over `zephyr::device::flash`
   (already in the pinned module); `Reset` via `sys_reboot`; `boot_id` and
   the `unique` string in the embassy format (`name-` then 16 nibble
   letters) from `hwinfo_get_device_id`, so TaipoTeacher sees the same
   identity for the same keyboard under both firmwares.
3. Serial number descriptor from the same string (`CONFIG_HWINFO=y` already
   wires `USBD_DESC_SERIAL_NUMBER_DEFINE`).

## Phase 7 — consumer-control keys

`TASKS.md`'s pending item, for both firmwares: a second HID report (or a
report-ID'd composite descriptor) and a `KeyAction` variant; on `jolt`, the
`zephyr,hid-device` node grows accordingly.  Independent of everything
above.

## Phase 8 — housekeeping

- `CLAUDE.md`/`AGENT.md`: `jolt` is no longer "in-progress"; `zbbq/` and
  `zephyr-play/` to `archive/`; the sibling-checkout note gains
  `~/zephyrproject/modules/lang/rust` and the branch it must be on.
- `TASKS.md`: fold the "Jolt" and "proto4 support" leftovers (the
  `PROTO4_MAPPING` bug is fixed by deletion in phase 2; the PWM LED items
  become "optional, upstream patch pending").
- The `davidb-*` lang-rust PRs: the rebase in phase 1 is most of the work of
  getting them merged, which removes the private-branch dependency this plan
  otherwise carries.

---

# Commit plan

Phases are sequential; within a phase, one commit per bullet unless the
compiler forces two together (say so in the message).  Per the project's
testing convention, hardware testing happens in review; each response says
what was built, what was tested under what conditions, and what could not
be.

1. lang-rust integration branch + `optional.yaml` pin; `jolt/Cargo.toml`
   embassy versions.  *(2 commits, one in each repository.)*
2. `bbqboards` PWM LED dtsi and overlay updates; root `.envrc` and
   `check.sh`.  *(2 commits.)*  → `b-proto4.sh` builds.
3. `mapping.rs` → `translate`; `mesa1` shield; `mesa2` shield; build
   script.  *(3 commits.)*  → keys type on the mesa1.
4. Taipo-only features and cfg gating.  *(1–2 commits.)*
5. `led_strip.rs` backend; manager port and `set_mod_state`.  *(2 commits.)*
6. `mesa3` translation and shield, when the matrix is final.  *(2 commits.)*
7. Vendor USB class; keylog + minder port.  *(3–4 commits.)*
8. Consumer control.  Housekeeping.

---

# Open questions

1. **Which `zephyr-lang-rust` to build against.**  Rebase the #146–#150
   stack plus the embassy upgrade onto `upstream/main` now (proposed — it is
   work the PRs need anyway), or build against `davidb-i2c` as it stands
   with embassy 0.7 and rebase later?  The answer also decides whether
   `jolt/Cargo.toml` goes to embassy 0.10 or stays at 0.7.
2. **The manifest pin.**  `zephyr/submanifests/optional.yaml` in the fork
   pins `dd73abc`; a `west update` resets the module to it.  Bump the pin in
   the fork (clean, but the fork's checkout is a detached upstream HEAD, so
   it means a local branch), or leave the module on a branch and never run
   `west update` on it?
3. **Is the proto4 still available** for the phase 1 smoke test, or does the
   mesa1 take that role too?
4. **PWM LED**: keep Tiny 2040 onboard-LED support alive in `jolt` (for
   proto4/jolt3) via the shield dtsi as proposed, or drop it until the
   Zephyr patch is upstream?
5. **Key log before or after daily use?**  Phase 6 is after phase 5 above;
   if TaipoTeacher's logs matter more than switching the daily driver early,
   swap them.  Also whether the mesa2 should switch at all before the key
   log exists, since a gap in its logs is a gap in the training data.
6. **Feature passthrough**: `rust_cargo_application()` has no way to select
   cargo features per shield.  Fine for now (default = taipo-only, and the
   jolt3 build script can pass `-DCONFIG_…`-free cargo flags another way),
   but if a per-shield feature set is wanted, that is a lang-rust change.
7. **What happens to `jolt-embassy-rp`** once the mesas run on `jolt` — kept
   as the reference and the jolt3's firmware, or retired?  This affects
   whether phase 7 (consumer control) is done twice.
8. **The wireless design's firmware** is deliberately not decided here; see
   "The split-wireless question".  When that board is on the bench, the
   inputs are: whether `davidb-i2c` has merged, whether a C-shim BLE HID
   proved tolerable, and whether ZMK has moved off the legacy USB stack by
   then (it is on ZMK's announced roadmap).
