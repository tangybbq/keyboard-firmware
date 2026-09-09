# Porting the mesa keyboards to ZMK

This is a work plan for running ZMK on the mesa line, with Dosh as the only
layout.  It replaces the bespoke Rust firmware (`jolt-embassy-rp`) on those
boards; the Rust firmware stays as the reference implementation of the Dosh
engine, and its test corpus becomes the check on the port.

Read this whole document before starting.  Nearly every fact in "What was
found" was read out of a checkout or a manifest rather than remembered, and
the derivation is given so it can be re-checked rather than trusted.  Where
something could not be verified it says so.

Status: **plan only.  Nothing has been built.**

---

## Requirements (from the developer, verbatim)

- Understand how ZMK is intended to be used and developed.  I am very familiar
  with zephyr development, but much of what I've read about using ZMK seems to
  suggest a tuned build environment for it (as well as cloud builds).  I want
  to understand how to set up a local build environment to be able to develop
  my support locally, as well as to understand how I can share both my board
  support (modules?) as well as any specific features I create, such as Dosh
  support.
- Initial focus will be on supporting only Dosh on the mesa line.  Development
  will probably start on the mesa1, while I wait for the mesa3 to come from
  production.  I have only built a single mesa2 and it is my main keyboard.
- There is a possibility that we will need to add DMA support to the sk2612
  driver in Zephyr.  Last time I used zephyr on a keyboard, there was no DMA
  support for the PIO driver and the LEDs would occasionally flicker if an
  interrupt caused the PIO to underrun.
- The initial goal is to make Dosh more available.  The main reason for the
  bespoke Rust keyboard firmware was for the steno support.  With that out of
  the picture, just adding needed functionality to ZMK makes a lot more sense.
- The longer-term goal is to make a future split, wireless design more
  practical.
- (Follow-up.)  The ZMK checkout under `~/linaro/zep-2026-01/zmk` is likely
  rather old.  I'd rather work with a fresh ZMK, perhaps as a submodule.  It
  would also be nice to be able to reuse the `~/zephyrproject` workspace even
  though that will require me to manage ZMK work along with other work I do on
  zephyr.

---

# What was found

## ZMK today

- `zmkfirmware/zmk` `main` is at `641514a97` (2026-08-21).  The newest tag is
  **v0.3.0 (2025-08-01), and it still pins Zephyr `v3.5.0+zmk-fixes`**; `main`
  moved to Zephyr 4.1 in December 2025 (blog post "Zephyr 4.1 Update",
  2025-12-09).  So the "pin your ZMK version" advice on the ZMK blog does not
  apply here: **track `main`**, pinned by the submodule commit, not a tag.
- The old checkout at `~/linaro/zep-2026-01/zmk/zmk` is from 2024-07-05 and
  pins Zephyr 3.5.  Everything it knows about board layout (`app/boards/arm/…`)
  is pre-HWMv2 and wrong for today's ZMK.  Ignore it.
- ZMK's future work, stated in the same blog post: deprecate the `kscan` API in
  favour of Zephyr's input/matrix subsystem, and move to the new USB stack.
  Both matter to us later (see "Open questions"); neither has landed.

## ZMK's Zephyr, and why `~/zephyrproject` cannot host it as-is

ZMK's manifest (`zmk/app/west.yml`) pins `zephyr` to the **`zmkfirmware/zephyr`
fork at branch `v4.1.0+zmk-fixes`**, imports Zephyr's own manifest with a
`name-blocklist` (HALs for altera/cypress/infineon/microchip/nxp/openisa/
xtensa/st/ti, loramac, mcuboot, mcumgr, net-tools, openthread, edtt, tf-m;
`hal_rpi_pico` is *not* blocked), and adds ZMK forks of `hal_stm32`, `lvgl`,
and `zmk-studio-messages`.

The fork branch is v4.1.0 plus 41 commits (read with
`gh api repos/zmkfirmware/zephyr/commits?sha=v4.1.0%2Bzmk-fixes`).  Grouped:

| area | commits | matters to us? |
|---|---|---|
| nRF BLE controller fixes | 3 | only for a future wireless board |
| ls0xx / sharp memory display | 4 | no |
| STM32 (C0 USB/clocks, G0 option bytes, ROM bootloader, WB55) | 11 | no |
| Silabs MG24 board and timer fixes | 5 | no |
| Cirque Pinnacle input driver | 3 | no |
| retained_mem BBRAM driver + tests, retention pre-kernel fix | 4 | no |
| boards: blok, Metro RP2040, xiao_ble runner, xiao MG24 | 5 | no |
| `soc: raspberrypi: rpi_pico: Add RP2 bootloader support` (`8ec72ab09`) | 1 | **yes** — adds `dts/arm/raspberrypi/rpi_pico/rp2040-boot-mode-retention.dtsi`, which ZMK's `rpi_pico//zmk` variant includes and which `&bootloader` on RP2040 relies on.  Absent from upstream at v4.1.0, v4.4.0 and main. |
| `cmake: modules: Add new post_boards_shields extension` (`f50faa7bc`) | 1 | **yes** — ZMK's `app/boards/post_boards_shields.cmake` hooks it.  Absent from upstream at v4.1.0 and v4.4.0. |
| twister fork behaviour, nordic binding validation, pm slots revert | 3 | no |

`~/zephyrproject` is a workspace whose manifest repository is the developer's
own Zephyr fork (`d3zd3z/zephyr`) at **4.4.99 (main)**, with
`project-filter = +zephyr-lang-rust`, a `.venv` (west 1.5.0), and SDK
**1.0.0**.  ZMK wants Zephyr 4.1 (`SDK_VERSION` 0.17.0) and two fork patches.
Three concrete obstacles to a single shared workspace:

1. The zephyr checkout would have to flip between `main` and
   `v4.1.0+zmk-fixes`, and every module the manifest names would have to be
   re-synced (`west update`) each way, because HAL revisions differ between
   4.1 and 4.4.  Every flip invalidates every build directory on both sides.
2. ZMK against Zephyr *main* is a porting project in its own right — three
   releases of API drift since 4.1 (ZMK's own move from 3.5 to 4.1 took most
   of a year).  Not something to take on as a side effect of wanting Dosh.
3. The two fork patches are not in upstream at any version, so even a
   pristine upstream 4.1 checkout is short of what ZMK's RP2040 boards
   include.

What *can* be shared, and is worth sharing: the Python venv (west 1.5 and the
4.4 requirements are a superset of 4.1's), the SDK — **the repository-root
`.envrc` already points at `zephyr-sdk-0.17.1`**, which is the right one for
4.1 (`~/zephyrproject/.envrc` uses 1.0.0 for main) — and the git object store
of `~/zephyrproject/zephyr`, so the fork does not cost another full clone.
See "Workspace layout" below.

One trap: the root `.envrc` exports `ZEPHYR_BASE=~/zephyrproject/zephyr`.
Zephyr's CMake honours `ZEPHYR_BASE` over the west workspace, so a ZMK build
run with that environment would silently build against 4.4.99.  The ZMK
environment must override it.

## The Tiny 2040 board

Every mesa is a Pimoroni Tiny 2040 soldered to the keyboard PCB.  Upstream
Zephyr has a `tiny2040` board (`boards/pimoroni/tiny2040/`, Jonas Berg,
Apache-2.0) — **but it first appears in v4.4.0** (`ac6e27d95b2`).  It is what
`jolt/b-proto4.sh` builds for today, against 4.4.99.  ZMK's 4.1 does not have
it, and ZMK has no interconnect or board for it either.

The board is small (dts + pinctrl dtsi + board.yml + defconfig + Kconfig), and
nothing in it needs anything newer than 4.1: the rp2040.dtsi in v4.1.0 already
has `vreg`, `xosc`, `ssi`, `timer`, `rtc`, `dma`, `pio0`, `usbd`, `die_temp`,
and the 4.1 rpi_pico board already uses `zephyr,flash-controller = &ssi` and
`CONFIG_CLOCK_CONTROL=y`.  Two things differ: 4.4's tiny2040 uses
`compatible = "zephyr,mapped-partition"` for its flash partitions where 4.1's
rpi_pico uses `fixed-partitions` — copy 4.1's form — and ZMK needs a
`storage_partition` for settings.

What ZMK adds to an RP2040 board, from `app/boards/raspberrypi/rpi_pico/`:

```
board.yml                       board: { extend: rpi_pico, variants: [{name: zmk, qualifier: rp2040}] }
rpi_pico_rp2040_zmk.dts         #include <../boards/raspberrypi/rpi_pico/rpi_pico.dts>
                                #include <arm/raspberrypi/rp2040-boot-mode-retention.dtsi>
                                &uart0 { status = "disabled"; };
                                &code_partition { reg = <0x100 (DT_SIZE_M(2) - 0x100 - DT_SIZE_K(512))>; };
                                &flash0 { partitions { storage_partition: partition@180000 {
                                    reg = <0x180000 DT_SIZE_K(512)>; read-only; }; }; };
rpi_pico_rp2040_zmk_defconfig   CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC=125000000
                                CONFIG_RESET=y CONFIG_CLOCK_CONTROL=y CONFIG_GPIO=y
                                CONFIG_USE_DT_CODE_PARTITION=y CONFIG_BUILD_OUTPUT_UF2=y CONFIG_BUILD_OUTPUT_HEX=y
                                CONFIG_ZMK_USB=y
                                CONFIG_MPU_ALLOW_FLASH_WRITE=y CONFIG_NVS=y CONFIG_SETTINGS_NVS=y
                                CONFIG_FLASH=y CONFIG_FLASH_PAGE_LAYOUT=y CONFIG_FLASH_MAP=y
                                CONFIG_RETAINED_MEM=y CONFIG_RETENTION=y CONFIG_RETENTION_BOOT_MODE=y
rpi_pico.zmk.yml                hardware metadata
Kconfig.rpi_pico
```

That `//zmk` variant mechanism (a Zephyr *board extension*) only works for a
board Zephyr already has.  For the Tiny 2040 under ZMK's 4.1 the whole board
has to come from our module.  When ZMK moves to a Zephyr ≥ 4.4, our board
definition becomes a name clash with upstream's and must be replaced by a
`tiny2040//zmk` extension in exactly the rpi_pico shape above.  Plan for that
swap; it is a one-commit change.

## The LEDs and the PIO driver

The developer's memory is right about the driver: in Zephyr 4.1
`drivers/led_strip/ws2812_rpi_pico_pio.c` feeds the PIO from the CPU
(`pio_sm_put_blocking` per pixel) and sleeps for the reset (`k_usleep`).
**DMA support landed in v4.4.0** (`7081a5ff4bb` "Use reset timer instead of
sleep", `18403563412` "Add DMA support", both 2026-01-08, plus `52da13e0ea2`
"fix SM start offset", 2026-06-17): 261 insertions across the driver, its
binding (`dmas`/`dma-names = "tx"`) and `Kconfig.ws2812`.  The RP2040 DMA
driver itself and the `rpi-pico-dma-rp2040.h` binding header are both present
in v4.1.0, so a backport is self-contained.

But look at the FIFO before backporting.  The 4.1 driver joins the TX FIFO
(`sm_config_set_fifo_join(PIO_FIFO_JOIN_TX)`, 8 words deep) and each pixel is
one 32-bit word.  **A 4-LED strip is 4 words: the entire frame is in the FIFO
before the PIO has finished the first pixel.**  An interrupt cannot cause an
underrun on a frame that is already queued.  Underrun-by-interrupt is a real
failure mode for strips longer than 8 LEDs; every mesa has exactly 4.  So the
flicker seen last time was on a longer strip, or had another cause (the two
plausible ones on 4.1: the `k_usleep` reset window being *shorter* than the
part needs because the sleep is a thread sleep, and ZMK's `rgb_underglow.c`
re-sending the strip every 50 ms).  **Measure before backporting**: phase 5
below drives the LEDs with the 4.1 driver first, and the backport is the
contingency, not the plan.

Other LED facts that shape the design:

- ZMK's `rgb_underglow.c` owns the strip completely: a 50 ms timer rewrites
  every pixel from one global HSB colour and an effect; there is no per-pixel
  API.  It cannot show "one modifier per LED".  We do not enable
  `CONFIG_ZMK_RGB_UNDERGLOW` at all; our LED code talks to `led_strip` itself.
- ZMK's newer `app/src/indicators/indicator_leds.c` maps *HID* indicators
  (caps lock etc. from the host) onto Zephyr `led` devices.  Not what we need,
  but the precedent for "a small feature that owns some LEDs".
- ZMK's hardware docs show RP2040 underglow only via `raspberrypi,pico-spi-pio`
  + `worldsemi,ws2812-spi`.  The `worldsemi,ws2812-rpi_pico-pio` driver is
  nonetheless in 4.1 (`WS2812_STRIP_RPI_PICO_PIO`, default y when the
  compatible is present), and `jolt/bbqboards/.../jolt3_mez_2040.overlay`
  already has a working devicetree for it (`bit-waveform = <3>, <3>, <4>`,
  `frequency = <800000>`, `reset-delay = <280>`, GRB colour mapping).
- The parts are SK6812, driven with WS2812 timing, on GP26 on every mesa, 4 in
  a chain, behind the series diode described in `mesa3/DESIGN.md`.

## How ZMK is built, and where things go

ZMK's own development layout (docs: "Native setup"): clone `zmk`, then inside
it `west init -l app && west update && west zephyr-export && west packages pip
--install`.  **The workspace topdir is the `zmk` directory itself**; `zephyr/`,
`modules/`, `bootloader/`, `tools/`, `.west/`, `build/` and `zmk-config/` land
inside it and are all in ZMK's `.gitignore`.  That fits a submodule cleanly.

Builds run from `zmk/app` (or with `-s app`):

```
west build -s app -d build/mesa1 -b tiny2040 -- \
    -DSHIELD=mesa1 \
    -DZMK_CONFIG=/abs/path/to/config \
    -DZMK_EXTRA_MODULES="/abs/path/zmk-mesa;/abs/path/zmk-dosh"
```

`app/CMakeLists.txt` does
`set(ZEPHYR_EXTRA_MODULES "${ZMK_EXTRA_MODULES};…/module;…/keymap-module")`,
so `ZMK_EXTRA_MODULES` is just Zephyr's extra-modules list with ZMK's two
internal modules appended.  `ZMK_CONFIG` is a directory ZMK searches for
`<board>.conf`, `<shield>.conf`, `<shield>.keymap` (and `<board>.keymap`),
overlays of the same names, and a `boards/` subfolder that is added as a board
root; its `.conf` overrides board and shield defaults.  Split builds use
separate `-d` directories.  The `.uf2` comes out of
`CONFIG_BUILD_OUTPUT_UF2=y` at `build/<name>/zephyr/zmk.uf2`.

Modules (docs: "Module Creation", template `zmkfirmware/zmk-module-template`):
a Zephyr module with `zephyr/module.yml` (`build: { cmake: ., kconfig: Kconfig,
settings: { board_root: ., dts_root: ., snippet_root: . } }`), and then
whatever the module is: `boards/` and `dts/` for keyboards; `CMakeLists.txt`,
`Kconfig`, `include/`, `src/` for behaviours, drivers and features.  Naming
convention `zmk-<type>-<description>` with types keyboard, behavior, driver,
feature, vfx, component.  A consumer lists the module in their
`config/west.yml` next to `zmk`; ZMK's GitHub Actions builds it from a
`build.yaml`.  For local work `ZMK_EXTRA_MODULES` points at the directory.

Shields (docs: "New Shield") need, under `boards/shields/<name>/`:
`Kconfig.shield` (`config SHIELD_MESA1 def_bool $(shields_list_contains,mesa1)`),
`Kconfig.defconfig` (`CONFIG_ZMK_KEYBOARD_NAME`), `<name>.overlay`,
`<name>.keymap`, `<name>.zmk.yml` (`type: shield`, `requires: [<board>]`).  The
overlay must have a `zmk,kscan-gpio-matrix` node, a `zmk,matrix-transform`,
and — required since v0.2 — a `zmk,physical-layout` linking the two, with
`chosen { zmk,kscan; zmk,physical-layout; }`.  The `keys` property (positions
and rotations in centi-keyunits) is optional and only ZMK Studio needs it.

`zmk,kscan-gpio-matrix` semantics (docs: "Keyboard Scan Configuration"):
**`col2row` drives the columns and reads the rows; `row2col` drives the rows
and reads the columns**; the read side gets `GPIO_PULL_DOWN`.  Defaults:
`debounce-press-ms` 5, `debounce-release-ms` 5, `debounce-scan-period-ms` 1,
`poll-period-ms` 10 when idle, interrupt-driven idle unless
`CONFIG_ZMK_KSCAN_MATRIX_POLLING`.  The Rust firmware scans every 1 ms with
its own debounce; ZMK's 5 ms press debounce adds latency the chord timer will
see.  Tunable per shield; see "Open questions".

Behaviours (docs: "New Behavior"; `app/include/zmk/behavior.h`): a Zephyr
device driver with `.binding_pressed`/`.binding_released` taking
`(struct zmk_behavior_binding *binding, struct zmk_behavior_binding_event
event)`, where the event carries `layer`, `position`, `timestamp` (ms) and,
on split builds, `source`; the binding carries `param1`/`param2` from the
keymap.  `.locality` selects `BEHAVIOR_LOCALITY_CENTRAL` /
`EVENT_SOURCE` / `GLOBAL`.  Key output is
`raise_zmk_keycode_state_changed_from_encoded(encoded, pressed, timestamp)`
(`app/include/zmk/events/keycode_state_changed.h`), which is what `&kp` does;
`zmk_behavior_queue_add` sequences bindings with delays (what macros use);
`zmk_behavior_invoke_binding` invokes any binding (what combos use).

Events (`app/include/zmk/event_manager.h`): `ZMK_LISTENER(mod, cb)` plus
`ZMK_SUBSCRIPTION(mod, ev_type)` placed in the `.event_subscription` linker
section; **delivery order is section order, i.e. link order**, and a listener
returns `ZMK_EV_EVENT_BUBBLE`, `HANDLED`, or `CAPTURED` (event held, to be
re-raised later with `ZMK_EVENT_RAISE_AFTER`/`_AT`).  `app/src/combo.c` is the
in-tree example of capturing `zmk_position_state_changed` before the keymap
sees it.  For an out-of-tree module the position in link order relative to
`keymap.c` is not something we control from `module.yml` — which is the
reason the engine below is a *behaviour* rather than a listener.

Tests (docs: "Tests"; `app/run-test.sh`): each test is a directory with
`native_sim.keymap`, `events.patterns` (sed), `keycode_events.snapshot`; the
runner builds `-b native_sim//zmk_test_mock -DZMK_CONFIG=<testdir>` and diffs
the filtered log against the snapshot.  **The path argument can be any
directory**, `ZMK_EXTRA_MODULES` is honoured — but for at most one module —
and `ZMK_TESTS_AUTO_ACCEPT=1` rewrites snapshots.  Inputs are scripted with
the `zmk,kscan-mock` device (press/release events with delays), which is
exactly the shape of the Rust key logs.

## Prior art: Taipo on ZMK

`dlip/zmk-taipo` is the known implementation: a `taipo.dtsi` of about 36
`TCOMBO` macros, each expanding to eight combos (bare, +inner thumb, +outer
thumb, +both, per hand) — roughly 290 combos — with `timeout-ms` from one
constant, modifiers as `&sk`, and `cradio.conf` raising
`CONFIG_ZMK_COMBO_MAX_COMBOS_PER_KEY=128`, `…_MAX_KEYS_PER_COMBO=10`,
`…_MAX_PRESSED_COMBOS=10`.  No custom code.

That proves ZMK combos can carry a Taipo table, and it is a fine bootstrapping
experiment, but it is not Dosh as the Rust engine defines it.  Combos are
"these N positions within T ms"; the engine is a per-hand accumulator with a
different end condition set, and its semantics live outside any single combo:

| engine behaviour (`bbq-keyboard/src/layout/taipo.rs`) | combos |
|---|---|
| chord ends on timer (100 ms), on all keys released, **or when the other hand presses** | timer only |
| the chord's key stays held until the chord's keys release (auto-repeat) | `slow-release` approximates |
| one-shot modifiers released by the next key; **double press makes the whole held set sticky**; null chord (both thumbs) releases all | `&sk` covers one-shot only |
| `Text` chords (`th`/`Th`) with per-character shift | macros, per entry |
| `Variant` chords `rsni`/`aote` selecting the live table | second combo set per layer, plus a layer toggle |
| an unmapped chord types nothing (a pure error, worth logging) | falls through to the single keys |
| every chord/timing visible to a key log | invisible |

## What the Dosh engine actually does — the inventory to port

From `taipo.rs`, `dosh.rs`, `DOSH.md`, `TAIPO.md`, `docs/mesa2.md` and
`TASKS.md`.  Everything in the left column has a test in
`bbq-keyboard/tests/taipo.rs` or `golden.rs`.

- Ten bits per hand (`0x001 a`, `0x002 o`, `0x004 t`, `0x008 e`, `0x010 r`,
  `0x020 s`, `0x040 n`, `0x080 i`, `0x100`/`0x200` thumbs), the two hands
  independent and freely alternated.  `SCAN_MAP` maps proto3 key codes to
  (side, bit).
- Chord accumulation per hand, ended by: `CHORD_TIME` (100 ms) since the
  first key; all keys of the hand released; a key-down on the other hand
  (`SideManager::force_down`).  On end: table lookup; the action's key is
  pressed and held; released when the hand's keys are all up.
- Actions: `Simple(key)`, `Shifted(key)`, `Text(&str)` (per-character shift,
  last character left held), `OneShot(mods)`, `Release` (null chord),
  `Variant(Taipo|Dosh)` (`rsni` = `0x0f0` → Taipo, `aote` = `0x00f` → Dosh;
  takes effect for the next chord; only when it changes does it report).
- Modifiers: one-shot set applied to the next key; re-pressing a modifier
  chord whose modifiers are all already held promotes the *whole* held set to
  sticky; sticky survives keys until the null chord.  `set_mod_state(oneshot,
  sticky)` is reported on change, and drives the LEDs.
- Default table is **Dosh** (`TaipoVariant::DEFAULT`); the choice is not
  persisted across power-off.
- Tables: `TAIPO_ACTIONS` and `DOSH_ACTIONS` (130 entries), exported as
  `bbq-keyboard/layouts.json` by `cargo run --example gen-layouts`, with a
  byte-level fingerprint (`fingerprint.rs`) that the key log and the host
  replay use to know which table produced a chord.  **`layouts.json` is the
  interface a generator should read** — it exists precisely so that a second
  implementation cannot drift.
- Not needed on the mesa: the mode key, the row-position toggle, the `#` key
  Dosh toggle (mesa1 has those two keys; mesa2/3 have none), steno, qwerty.
- Pending in `TASKS.md` and still wanted: consumer-control (media) keys.
- LEDs (`jolt-embassy-rp/src/leds/manager.rs`, commits `753fb8e`, `d9ae0b2`):
  four LEDs, one modifier each — Control green `(0,3,0)`, Shift red `(8,0,0)`,
  Alt blue `(0,0,24)`, GUI yellow `(3,3,0)` — dark when not held, the colour
  while one-shot, colour plus a white `LATCH` when sticky; a quarter of the
  earlier brightness.  Nothing else is on the LEDs any more.
- Key log (`minder/src/keylog.rs`, `taipo-teacher.md`): 4-byte records over a
  USB vendor bulk interface, consumed by TaipoTeacher.  Only the device can
  see chords, hands, per-key timing, how a chord ended, and unmapped chords.
  This is the one host-facing feature the Rust firmware has that ZMK has no
  analogue for.

## The boards

All three are one Tiny 2040 scanning everything, `Inter::None`, four SK6812 on
GP26, no inter-board protocol (mesa1 and mesa3 carry a passive half over an
RJ-45).  The matrices, as ZMK's `kscan-gpio-matrix` wants them:

### mesa1 — 30 positions, `col2row`

From `jolt-embassy-rp/src/board.rs` (`mod mesa1`) and
`bbq-keyboard/src/translate.rs` (`MESA1`): the diodes conduct column → row, so
the scanner drives `COL_1..6` and reads `ROW_A..E`.

| ZMK role | mesa1 net | GPIO |
|---|---|---|
| `col-gpios` (driven) | `COL_1 COL_2 COL_3 COL_4 COL_5 COL_6` | 27 28 29 7 6 5 |
| `row-gpios` (read, pull-down) | `ROW_A ROW_B ROW_C ROW_D ROW_E` | 4 3 2 1 0 |

What sits at each (column, row), in Taipo key names, from `MESA1`'s proto3
codes cross-checked against `SCAN_MAP` (left `r a s o n t i e Sp Bk` = 4 5 8 9
12 13 16 17 19 23, right = 28 29 32 33 36 37 40 41 43 47):

| | `ROW_A` | `ROW_B` | `ROW_C` | `ROW_D` | `ROW_E` |
|---|---|---|---|---|---|
| `COL_1` | *mode key* (2) | L-n | *`#` key* (1) | L-t | — (18) |
| `COL_2` | L-r | L-i | L-a | L-e | L-Sp |
| `COL_3` | L-s | — (20) | L-o | — (21) | L-Bk |
| `COL_4` | — (24) | R-n | — (25) | R-t | — (42) |
| `COL_5` | R-r | R-i | R-a | R-e | R-Sp |
| `COL_6` | R-s | — (44) | R-o | — (45) | R-Bk |

The eight `—` positions are the steno-only keys, physically removed from the
mesa1.  The mode key and `#` key exist and are free for ZMK bindings (a
`&bootloader` on one of them is worth having on the development board).

### mesa2 Rev A — 20 positions, `row2col` (the daily driver)

From `docs/mesa2.md`, which derived it from the KiCad netlist.  The diodes
conduct row → column, so the scanner drives `ROW_A..D` and reads `COL_1..5`.

| ZMK role | mesa2 net | GPIO |
|---|---|---|
| `row-gpios` (driven) | `ROW_A ROW_B ROW_C ROW_D` | 28 27 0 1 |
| `col-gpios` (read, pull-down) | `COL_1 COL_2 COL_3 COL_4 COL_5` | 4 5 6 7 29 |

| | `COL_1` | `COL_2` | `COL_3` | `COL_4` | `COL_5` |
|---|---|---|---|---|---|
| `ROW_A` (left far) | L-r | L-s | L-n | L-i | L-Sp |
| `ROW_B` (left near) | L-a | L-o | L-t | L-e | L-Bk |
| `ROW_C` (right far) | R-i | R-n | R-s | R-r | R-Bk |
| `ROW_D` (right near) | R-e | R-t | R-o | R-a | R-Sp |

Note the right hand's columns run index → pinky, the mirror of the left's.

### mesa2 Rev B and mesa3 — 18 positions in the same 5 × 4, `row2col`

From `~/Documents/Keyboards/mesa3/DESIGN.md` and `mesa3-tasks.md`: same pins as
Rev A, same rows, but **Rev B fixes the column pairing so each column is one
finger on both hands** (`COL_1` pinky `A`, `COL_2` ring, `COL_3` middle,
`COL_4` index, `COL_5` thumbs), and the two upper-pinky `R` keys are deleted.
The mesa3 is Rev B cut in two, with `COL_1..5`, `ROW_C`, `ROW_D` crossing the
RJ-45, and the design requirement is that firmware *cannot* tell them apart.

| | `COL_1` | `COL_2` | `COL_3` | `COL_4` | `COL_5` |
|---|---|---|---|---|---|
| `ROW_A` | *(no key)* | L-s | L-n | L-i | L-Sp |
| `ROW_B` | L-a | L-o | L-t | L-e | L-Bk |
| `ROW_C` | *(no key)* | R-s | R-n | R-i | R-Bk |
| `ROW_D` | R-a | R-o | R-t | R-e | R-Sp |

So Rev A and Rev B/mesa3 are **two shields sharing one pin set and differing
in the right-hand transform**: `mesa2` (Rev A) and `mesa3` (Rev B and mesa3).
The Rust firmware's CBOR board-info blob has no ZMK equivalent; each board is
flashed with its own build, which with only two matrices is no burden.

Geometry for the optional Studio `keys` property is in `mesa2/LAYOUT.md` and
`mesa3/LAYOUT.md` (exact x, y, rotation per key, generated from
`mesa2/layout.py`).

---

# The plan

## 1. Workspace layout

```
keyboard-firmware/
  zmk/                 git submodule: zmkfirmware/zmk, pinned to a main commit
    app/               ZMK's application; builds run with -s app
    zephyr/            (untracked, ZMK's .gitignore) zmkfirmware/zephyr @ v4.1.0+zmk-fixes
    modules/           (untracked) HALs etc. at ZMK's 4.1 revisions
    .west/             (untracked) west topdir is here
  zmk-mesa/            module: tiny2040 board, mesa shields, LEDs (see 2)
  zmk-dosh/            module: the Dosh behaviour and its tests (see 2)
  zmk-config/          ZMK_CONFIG dir: mesa1.keymap, mesa2.keymap, mesa3.keymap, *.conf
  zmk-env.sh           sources ~/zephyrproject/.venv, SDK 0.17.1, *unsets ZEPHYR_BASE*
  justfile (or scripts) build/uf2 recipes per shield, mirroring jolt-embassy-rp's
```

Bring-up commands, once:

```sh
git submodule add https://github.com/zmkfirmware/zmk.git zmk
cd zmk
# Share objects with the existing Zephyr clone instead of downloading it again.
git clone --reference-if-able ~/zephyrproject/zephyr \
    https://github.com/zmkfirmware/zephyr.git zephyr
west init -l app
west update            # adopts the pre-seeded zephyr/, fetches modules at 4.1 revisions
west zephyr-export
west packages pip --install   # into ~/zephyrproject/.venv; expected to be a no-op or near it
```

Why this and not one workspace: the three obstacles above.  What it costs: a
second set of HAL checkouts (~hal_rpi_pico, cmsis, picolibc… small) and a
second zephyr *worktree* but not a second object store.  If a `git worktree`
of `~/zephyrproject/zephyr` on a `zmk` remote's branch is preferred over
`--reference`, that works too — west only needs `zmk/zephyr` to be a git
checkout at the manifest revision; `west update` will not touch a checkout
that is already there and clean.  Both variants keep `~/zephyrproject`
untouched for other Zephyr work.

Deferred, deliberately: the `zmk-config`-style GitHub Actions cloud build.  It
is the way to *share* (a consumer's `west.yml` names `zmk` and our modules and
the action builds `build.yaml`), and phase 8 sets it up; it is not needed to
develop.

## 2. Two modules, not one

- **`zmk-mesa`** (`zmk-keyboard-mesa` by ZMK's naming): the `tiny2040` board,
  the `mesa1`/`mesa2`/`mesa3` shields, the LED strip devicetree, the
  modifier-LED feature, and — if it turns out to be needed — the DMA
  `ws2812` driver backport.  Everything hardware.
- **`zmk-dosh`** (`zmk-behavior-dosh`): the engine, its tables, its generator,
  its native_sim tests.  Depends on nothing in `zmk-mesa`; any 18–20-key
  board can bind it.

The split is what makes Dosh shareable on its own, and it is also what
`run-test.sh` wants: it passes at most one `ZMK_EXTRA_MODULES`, and the engine
tests need only `zmk-dosh`.  Both live as directories in this repository for
now; when they are published each becomes its own repository (with a
`west.yml` for consumers) and this repository points at them as submodules or
drops them.  Decide then, not now.

## 3. The board: `zmk-mesa/boards/pimoroni/tiny2040/`

Copied from Zephyr v4.4.0's `boards/pimoroni/tiny2040/` (keep the Apache-2.0
header and attribution), then:

- flash partitions in 4.1's `fixed-partitions` form, `code_partition` shrunk
  by 512 KiB, and a `storage_partition` at the end of the 8 MiB, mirroring
  `rpi_pico_rp2040_zmk.dts` but for 8 MiB;
- `#include <arm/raspberrypi/rp2040-boot-mode-retention.dtsi>` (from the ZMK
  fork) so `&bootloader` works;
- `&uart0 { status = "disabled"; }` — GP0/GP1 are matrix lines on every mesa;
- `tiny2040_defconfig` = 4.4's plus the ZMK rpi_pico list above (USB, NVS
  settings, retention);
- `tiny2040.zmk.yml` metadata, `Kconfig.tiny2040` (`select SOC_RP2040`), and
  `board.yml` with `name: tiny2040`.

Keep the name `tiny2040`: it is what upstream calls it and what `jolt`'s
scripts use.  Record in the module README that this directory is a stand-in
until ZMK's Zephyr is ≥ 4.4, at which point it turns into
`board.yml: { extend: tiny2040, variants: [zmk] }` plus a `tiny2040_rp2040_zmk.dts`
carrying only the ZMK deltas.

The Tiny 2040's own RGB LED (GP18/19/20, active low, PWM-capable) is on the
board dts as `gpio-leds`.  Leave it dark; it is not one of the four.

## 4. The shields: `zmk-mesa/boards/shields/{mesa1,mesa2,mesa3}/`

Each has the five files listed under "How ZMK is built".  The overlays carry
the kscan node from the tables above, a `zmk,matrix-transform` whose `map`
lists the *live* positions in a fixed order — **left hand `r s n i Sp a o t e
Bk`, then right hand in the same order** (the mesa1 appends its mode and `#`
keys; the mesa3 has no `r`) — so that the keymaps read the same on every
board, then the physical layout and `chosen` nodes, and the LED strip:

```dts
&pinctrl {
    ws2812_pio0_default: ws2812_pio0_default {
        ws2812 { pinmux = <PIO0_P26>; };
    };
};

&pio0 {
    status = "okay";
    pio-ws2812 {
        compatible = "worldsemi,ws2812-rpi_pico-pio";
        status = "okay";
        pinctrl-0 = <&ws2812_pio0_default>;
        pinctrl-names = "default";
        bit-waveform = <3>, <3>, <4>;

        led_strip: ws2812 {
            status = "okay";
            gpios = <&gpio0 26 GPIO_ACTIVE_HIGH>;
            chain-length = <4>;
            color-mapping = <LED_COLOR_ID_GREEN LED_COLOR_ID_RED LED_COLOR_ID_BLUE>;
            reset-delay = <280>;
            frequency = <800000>;
        };
    };
};
```

(Lifted from `jolt3_mez_2040.overlay`, pin changed; the SK6812 has been
happy with WS2812 timing under the Rust firmware.)  The strip is *not* set as
`zmk,underglow`; the modifier-LED feature finds it through its own node.

`mesa2` and `mesa3` share a `mesa2-common.dtsi` (pins, strip, physical
layout) and differ only in the transform.  `Kconfig.defconfig` sets
`ZMK_KEYBOARD_NAME` ("Mesa 1" etc.).  No split configuration on any of them.

Physical-layout `keys` for Studio: generate from `LAYOUT.md`/`placement.json`
later if Studio is ever wanted; leave the property out to start.

## 5. The Dosh engine as a behaviour: `zmk-dosh/`

Every layout key in the keymap is bound to the engine:

```dts
#include <dt-bindings/zmk/dosh.h>     /* DOSH_L_A … DOSH_R_BK: (hand, bit) */
/ { keymap { compatible = "zmk,keymap";
    default_layer { bindings = <
        &dosh L_R  &dosh L_S  &dosh L_N  &dosh L_I  &dosh L_SP
        &dosh L_A  &dosh L_O  &dosh L_T  &dosh L_E  &dosh L_BK
        &dosh R_R  &dosh R_S  &dosh R_N  &dosh R_I  &dosh R_SP
        &dosh R_A  &dosh R_O  &dosh R_T  &dosh R_E  &dosh R_BK
    >; };
}; };
```

Why a behaviour and not a `combo.c`-style listener: the keymap's own listener
is the thing that turns positions into bindings, and a module cannot pin its
listener ahead of `keymap.c` in link order.  Bound as a behaviour, the engine
is *called by* the keymap with the press/release and its timestamp, needs to
capture nothing, coexists with ordinary ZMK layers (a `&mo`/`&tog` to a
non-Dosh layer, `&bootloader` on the mesa1's spare keys), and on a split
board runs on the central with `BEHAVIOR_LOCALITY_CENTRAL` for free.

`src/behavior_dosh.c` — a single-instance behaviour
(`ZMK_BEHAVIOR_DT_INST_DEFINE(0, …)`, compatible `zmk,behavior-dosh`) that is
a transliteration of `TaipoManager`/`SideManager`:

- two `side` structs (bits down, accumulated code, first-press timestamp, a
  `k_work_delayable` for `CHORD_TIME`), the modifier state (`oneshot`,
  `sticky`), the live table pointer;
- `binding_pressed`: record the bit; call the other side's `force_down`
  (end its chord now, as the Rust engine does); schedule/keep this side's
  timer from `event.timestamp`;
- chord end (timer, other hand, or all-up): look the code up; `Simple`/
  `Shifted` raise the keycode via `raise_zmk_keycode_state_changed_from_encoded`
  with the one-shot modifiers folded into `implicit_modifiers`, and remember
  it as held; `Text` queues the characters with `zmk_behavior_queue_add`
  (all but the last released, the last held, same as Rust); `OneShot` and
  `Release` update the modifier set; `Variant` swaps the table pointer;
  unmapped → nothing, but logged;
- `binding_released`: when the side's bits reach zero, release the held key
  (the same "held until the chord's keys are up" rule);
- on every change of `(oneshot, sticky)` or of the variant, raise a module
  event (`zmk_dosh_state_changed`, declared in `include/zmk/events/`) that the
  LED feature and, later, a key logger subscribe to;
- `CHORD_TIME` and the default variant as Kconfig (`ZMK_DOSH_CHORD_MS`,
  default 100; `ZMK_DOSH_DEFAULT_TAIPO`, default n).

Modifiers are emitted as ordinary modifier *keycodes* pressed and released by
the engine (what `&sk` does), so the HID layer, split transport and host all
see plain key events.

`gen/gen-tables.py` reads `bbq-keyboard/layouts.json` and writes
`src/dosh_tables.c` (both tables as `{code, kind, arg}` arrays, sorted so the
lookup is a binary search) and a header carrying each table's **fingerprint
bytes** verbatim.  The Rust tables stay the source; a change there is
`cargo run --example gen-layouts`, then `gen-tables.py`, then a commit that
shows both diffs.  What `layouts.json` says about keycodes must be checked
against `bbq-keyboard/src/layout/export.rs` before writing the generator —
the encoding into ZMK's `HID_USAGE_KEY(...)` / `keys.h` names is the one
non-mechanical part.

`tests/`: ZMK native_sim tests, most of them **generated from the Rust test
corpus**.  `bbq-keyboard/tests/golden.rs`, `synth.rs` and `replay.rs` already
express "these key events at these times produce these keys" in the key-log
record format; a second small generator turns each into a
`native_sim.keymap` with `zmk,kscan-mock` events and a
`keycode_events.snapshot`.  That is the port's correctness argument: the C
engine is held to the Rust engine's own vectors, not to a re-reading of the
spec.  Hand-written tests cover what the corpus does not (cross-hand ending
at the boundary, sticky promotion, `Text` release ordering, variant chords).
Run with `ZMK_EXTRA_MODULES=../zmk-dosh ./run-test.sh ../zmk-dosh/tests`.

Not ported: the mode key and row toggle (nothing to switch to on a mesa; the
mesa1's extra keys get plain ZMK bindings), steno, qwerty.  The `#`-key Dosh
toggle is replaced by the `rsni`/`aote` chords, which the mesa2 already
needs.

## 6. LEDs: `zmk-mesa/src/mod_leds.c`

A feature, not a behaviour: subscribes to `zmk_dosh_state_changed`, keeps the
four `led_rgb` values, and calls `led_strip_update_rgb` from a work item
**only when something changed** — no 50 ms tick.  Devicetree:

```dts
/ { mod_leds: mod_leds { compatible = "mesa,modifier-leds";
    led-strip = <&led_strip>;
    /* one LED per modifier, in strip order: Control, Shift, Alt, GUI */
    modifiers = <MOD_LCTL MOD_LSFT MOD_LALT MOD_LGUI>;
}; };
```

Colours and the `LATCH` white come from `manager.rs`; brightness stays at the
quarter level `d9ae0b2` settled on.  Comes up dark, as today.  The Tiny
2040's own LED stays untouched.

DMA (contingency, see "The LEDs and the PIO driver"): if the 4.1 driver
flickers *with the tick removed*, backport 4.4's three commits into
`zmk-mesa/drivers/led_strip/` under compatible
`tangybbq,ws2812-rpi_pico-pio` and a Kconfig that does not collide with the
in-tree `WS2812_STRIP_RPI_PICO_PIO`, add `dmas = <&dma RPI_PICO_DMA_…>` and
`CONFIG_DMA=y` to the shields, and remove the whole thing when ZMK's Zephyr
catches up.  Also write down what actually flickered, since the FIFO
arithmetic says a 4-LED frame cannot underrun.

## 7. USB, bootloader, key log

- USB: `CONFIG_ZMK_USB=y` from the board defconfig; ZMK sets VID/PID and the
  product string from `ZMK_KEYBOARD_NAME`.  Nothing to write.
- Bootloader: `&bootloader` via the fork's boot-mode retention, bound to the
  mesa1's mode key; on the mesa2/3, which have no spare key, reserve a chord
  (a table entry kind `Binding` that invokes an arbitrary ZMK binding is the
  clean way, and the same mechanism gives `&bt BT_CLR` etc. to a wireless
  board later).  The physical reset pad and the Tiny 2040's BOOT button remain
  the fallback.
- Key log: **not in the initial scope**; see "Open questions".  The design
  above raises `zmk_dosh_state_changed` and could raise a per-chord event with
  the same content as a `keylog` record, so a later `zmk-feature-keylog`
  (ring buffer, drained over `CONFIG_ZMK_USB_LOGGING`'s CDC-ACM as framed
  lines, or over a vendor bulk endpoint on the legacy USB stack) is additive.

## 8. Toward a split, wireless mesa

Nothing in 1–7 has to change.  ZMK's split support is BLE (the mature path;
needs an nRF52840-class MCU, e.g. a nice!nano or XIAO BLE per half) or the
newer full-duplex wired UART (any MCU, one central + one peripheral).  A
future board is a new shield pair `mesaN_left`/`mesaN_right` in `zmk-mesa`
with `CONFIG_ZMK_SPLIT=y`; the peripheral only forwards positions, and
`&dosh` runs on the central as it does now.  Two things to keep in mind when
that design starts: BLE split adds 3.75 ms average / 7.5 ms worst-case per
event, inside the 100 ms window but not inside the per-key timing the key log
wants; and the ZMK fork's nRF controller patches are the reason to keep
building against ZMK's Zephyr rather than upstream's.

---

# Phases and commits

Each phase ends in commits; each commit says what was built and what was
tested.  Hardware testing happens in the developer's review, per the project
convention.  Where a phase touches the mesa2 — the only one, and the daily
driver — the Rust `.uf2` is the way back and takes seconds.

1. **Workspace.**  Add the `zmk` submodule; `zmk-env.sh`; `.gitignore`
   entries for `zmk/build`; a `justfile` with `build-<shield>` and
   `uf2-<shield>`.  Prove the toolchain by building an in-tree RP2040 target
   (`-b rpi_pico//zmk -DSHIELD=…` any small in-tree shield) — no hardware.
   Also record the exact `zmk` commit and its Zephyr revision in the module
   README.  *One commit.*
2. **`zmk-mesa` skeleton, `tiny2040`, `mesa1`.**  Module files, the board,
   the mesa1 shield with a plain keymap where every key is `&kp` of its Taipo
   letter (so the matrix can be verified key by key without any engine), and
   `zmk-config/mesa1.keymap`.  Test on the mesa1: every key types its letter,
   the eight removed positions are `&none`, the mode key is `&bootloader`.
   *Two commits: board; shield.*
3. **`mesa2` and `mesa3` shields.**  Same plain keymap.  Test `mesa2` on the
   daily driver briefly; `mesa3` cannot be tested until the boards arrive and
   the commit message says so.  *One commit.*
4. **`zmk-dosh`.**  In order: the table generator and generated tables (with
   the fingerprint check that they match `layouts.json`); the behaviour with
   `Simple`/`Shifted` and chord ending; modifiers and sticky; `Text`;
   `Variant`; the test generator and the generated corpus; hand-written tests.
   Each step is a commit with its tests passing under native_sim.  Then
   `zmk-config/*.keymap` switch from `&kp` to `&dosh`, and the mesa1 is
   typed on for real.  *Six or so commits.*
5. **LEDs.**  Strip node in the shields; `mod_leds.c`; test on the mesa1 for
   correctness and for flicker while typing and while USB is busy.  The DMA
   backport only if that shows flicker, as its own commit with the
   observation in the message.  *One to three commits.*
6. **Daily use.**  Flash the mesa2 with ZMK and live on it; tune
   `debounce-*` and `ZMK_DOSH_CHORD_MS` from feel; the `Binding` table kind
   for `&bootloader`.  Fixes as they come.
7. **Consumer keys** (`TASKS.md`'s pending item) as table entries — ZMK's
   `&kp C_VOLUME_UP` encodings go straight into `Simple`.
8. **Sharing.**  Split `zmk-mesa` and `zmk-dosh` into their own repositories
   with `west.yml`, `build.yaml` and the ZMK GitHub Action; a `zmk-config`
   repository that consumes them, so the cloud build path the ZMK docs
   describe works for anyone.  The mesa3, when it arrives, is brought up on
   this.
9. **Key log**, if wanted (open question).

What this plan deliberately leaves out: ZMK Studio (needs `keys` geometry and
a runtime keymap the engine would ignore anyway), the Tiny 2040's onboard
LED, steno and qwerty modes, any change to the Rust firmware.  The `jolt`
Zephyr-Rust port is not touched by this plan; whether it continues is a
separate decision (open question).

---

# Open questions

1. **Workspace: submodule-topdir (proposed) vs. anything in
   `~/zephyrproject`.**  The proposal shares the venv, SDK 0.17.1 and git
   objects but not the workspace, for the three reasons given.  If the
   preference is still to run ZMK out of `~/zephyrproject`, the way is a
   `git worktree` of that zephyr on `zmk/v4.1.0+zmk-fixes` used as
   `zmk/zephyr`, which is the same layout with a shared `.git` — say so and
   the bring-up commands change accordingly.
2. **Is the key log / TaipoTeacher a requirement of the port, or does it
   stay with the Rust firmware?**  It decides whether phase 9 exists, and it
   affects one design choice now: whether ZMK's 5 ms debounce is acceptable
   for the timing the log measures.  (The engine itself only cares about
   100 ms.)
3. **Debounce.**  ZMK defaults 5 ms press / 5 ms release; the Rust scanner is
   1 ms with its own debounce.  Start with ZMK's defaults or match the Rust
   behaviour (`debounce-press-ms = <1>` or eager `0`)?  Proposal: defaults
   first, retune from feel in phase 6.
4. **Shield naming.**  `mesa2` = Rev A (20 keys), `mesa3` = Rev B and the
   mesa3 (18 keys, per-finger columns), as proposed?  The alternative is
   `mesa2` for Rev A and `mesa2b` + `mesa3` as two names for one shield.
5. **Bootloader chord on the mesa2/3.**  Which chord, and is the `Binding`
   table kind (arbitrary ZMK binding from a chord) wanted, or should
   `&bootloader` wait for a physical key?  It is also how BLE profile
   management would be reached on a wireless board.
6. **What did flicker last time?**  Which board, how many LEDs, which driver.
   If it was a strip of more than 8 LEDs, the FIFO explanation holds and the
   mesa is safe; if it was 2–4 LEDs, the cause was something else and the DMA
   backport would not fix it.
7. **Does `jolt` (the Zephyr-Rust port) continue?**  This plan supersedes its
   reason to exist for the mesa line; `CLAUDE.md`, `TASKS.md` and its
   `bbqboards` shields would want updating to say so, or not.
8. **ZMK's coming changes.**  `kscan` → Zephyr input subsystem and the new USB
   stack are both announced.  Neither blocks anything here, but the shields
   are written to the `kscan` API and would follow ZMK's migration when it
   lands.  Track `zmkfirmware/zmk` PRs before bumping the submodule.
9. **Verify before phase 4:** how `layouts.json` encodes keycodes
   (`export.rs`), and whether `zmk,kscan-mock` in native_sim delivers
   millisecond-accurate timestamps to a behaviour (it should — the mock
   raises position events with `k_uptime`, and ZMK's own hold-tap tests
   depend on it — but the cross-hand and 100 ms tests hinge on it).
