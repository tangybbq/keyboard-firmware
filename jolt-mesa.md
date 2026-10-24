# Bringing the Zephyr port (`jolt`) up on the mesa keyboards

This is a work plan for making the Rust-on-Zephyr firmware in this tree run
the mesa line, Dosh and Orsy, so that it can replace `jolt-embassy-rp` as
the daily driver and become the base for a future split, wireless design.
It is the alternative to `port-zmk.md`, which stays as written; the reason
for preferring this route is at the end of that document's story: ZMK is
still on Zephyr's legacy USB stack and on Zephyr 4.1, while `jolt` is
already on the new USB stack against Zephyr main.

Read this whole document before starting.  The facts in "What was found"
were read out of the tree, the `~/zephyrproject` workspace and build
attempts, not remembered; the derivation is given so it can be re-checked.

Status (2026-10-24): **phases 1 to 5 built; phases 1–4 tried on the test
mesa3b, phase 5 (minder and the key log) not yet.**  The first version of
this plan (2026-09-09) was blocked on the `zephyr-lang-rust` binding PRs;
they have all merged.  Phase 1 also turned up, and fixed, a HID report
buffer that could be overwritten before the USB stack was done with it
(latent on the RP2040; see `3f1fa04`), and phase 4 got the jolt3 building
again.  Next: phase 5 against keyminder and TaipoTeacher, then phase 6.

Decisions taken on 2026-09-09 (developer), still standing: build against
`zephyr-lang-rust` `upstream/main`; `west update` runs periodically and the
wanted lang-rust revision is restored with jj afterwards; a board that is
a daily driver stays on the embassy firmware until the key log is ported.

Decisions taken on 2026-10-24 (developer): a second mesa3, with JTAG, is
the test board for phases 2–5; both built boards are believed to be
`mesa3b`; the lang-rust pin stays on the `jj new main@upstream` routine,
and the Zephyr `optional.yaml` bump is sent after the release, as upstream
is in a code freeze for about another week; the build script is
`b-mesa3.sh`.

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
- (Answers to the first draft's open questions.)  Let's try to build on
  upstream/main of zephyr-lang-rust, with the plan to get the needed changes
  merged into upstream.  That will also give us a nice real app to test
  against.  The proto4 should be available, I just have to find it.  For
  pinning, I'll need to periodically west update, but it is fine to restore
  our desired branch in jj after each time.  Let's leave the mesa2 on the
  embassy code until we have the logging ported.
- (2026-10-24.)  now that the changes we need have been merged into the Rust
  repo, I think we might be better ready to try to get the Rust Zefir version
  of this in the Jolt directory to work. Can you come up with a plan and
  figure out what's needed to get the Jolt Zefir code working on the Mesa 3
  keyboards?
- (Answers to the revision's open questions.)  I do have a second mesa 3 to
  use for testing. I have jtag for it as well. I think these are both
  mesa3b. For now, lets stick with the jj new, and we can push the update
  after the release (upstream is in a code freeze for another week,
  roughly). Perhaps make a b-mesa3.sh to build for this.

---

# What was found

## Which directory is the port

**`jolt/`.**  `zbbq/` (2024, hand-written bindings from before
`zephyr-lang-rust`) and `zephyr-play/` (an even earlier rename of it) are
dead and are candidates for `archive/`.  `jolt/` was rebuilt from scratch in
February 2026 (`5b6fa64`); its last real port work is `a63ee12`
(2026-03-17), and later commits are ripples from `bbq-keyboard`.

## What changed since the first version of this plan

**The bindings merged.**  `zephyr-lang-rust` `main@upstream` is `6823b69`
(2026-10-15) and contains everything `jolt` uses:

| upstream commit | what | `jolt` uses it for |
|---|---|---|
| `6889ba3` (#179, the #146 work re-proposed against `main`) | raw DT properties (`RAW_*`, `Value::Words`, `Word::Gpio`), `GpioPin::raw_new` | matrix pins, `board_info` address |
| `4c96580` | `device::led::{LedController, Led}` | `leds/pwm.rs` |
| `0a7f3bd` | `device::led_strip::{LedStrip, LedRgb}`, `update_rgb`, `length` | the missing `led_strip.rs` backend |
| `ea29d68` (#150) | I2C controller `transfer()` and target registration | a future split link only |
| `404adaa` (#161) | embassy-executor 0.10 / embassy-sync 0.8 / embassy-time 0.5 | — (forces `jolt`'s versions) |
| `9dec8fd` (#169) | the panic handler moved into a `zephyr-panic` crate | — (`jolt` must depend on it) |
| `ab3e546`, `5a41774`, `b7c19a6`, #181/#182 | bindgen `__UINTxx_C` fix, `zephyr::blocking`, Kconfig rebuilds, `Mutex` soundness | nothing directly |

`#179` also fixed the `RawInfo::Parent` bug that `jolt3_mez_2040.overlay`
works around with `/delete-node/`, so that workaround can go.

The workspace's lang-rust checkout is already on it: `@` is an empty jj
change on `6823b69`.  `jolt-base` (the merge bookmark the first plan called
for) and `~/zephyrproject/modules/lang/rust/jolt-base.md` are obsolete.
Zephyr itself is upstream `main` at `3f01a8a` (2026-10-17), and its
`submanifests/optional.yaml` **still pins lang-rust at `dd73abc`**
(2025-06-04), which has none of the above, so a `west update` still puts
the module back where `jolt` cannot build.

**The mesa3 exists, and the embassy firmware supports it.**  `ccc613d` and
`988d5e8` (2026-09-15, 09-22) added `mod mesa3` to
`jolt-embassy-rp/src/board.rs` and three tables to
`bbq-keyboard/src/translate.rs`.  Electrically:

- A Tiny 2040 on the left half; the right half is passive (switches, diodes,
  an RJ-45) and the left MCU scans the whole matrix.  `Inter::None`, as on
  the mesa1 and mesa2: no inter-board protocol, no I2C.
- One module serves three board-info names that differ only in the
  translation table: **`mesa3`** (mode key on the left), **`mesa3b`** (an Fn
  key on each hand, `FN_LEFT`/`FN_RIGHT`; the Fn behaviour is in
  `LayoutManager`, so the firmware does nothing for it), and **`mesa2b`**
  (the mesa2 Rev B unibody, neither).  `bbq-tool/sides.sh` has
  `mesa3` and `mesa3b` entries.
- Diodes conduct row → column, so the scanner drives the board's rows and
  senses its columns, with pull-downs:

  | scanner role | board nets | GPIOs, in this order |
  |---|---|---|
  | driven (`jolt`'s `col-gpios`) | `ROW_A`..`ROW_D` | 28 27 0 1 |
  | sensed (`jolt`'s `row-gpios`) | `COL_1`..`COL_5` | 7 6 5 4 29 |

  `jolt/src/matrix.rs` walks driven-outer, sensed-inner exactly as the
  embassy scanner does, so with the pins in that order the codes come out
  `ROW_x * 5 + COL_n` and `translate::mesa3`/`mesa3b`/`mesa2b` apply
  unchanged.  The overlay needs a comment saying the board's *rows* go in
  `col-gpios`, or the next reader will "fix" it.
- Four SK6812/ws2812 LEDs on **GP26**.

**The embassy firmware moved on** in ways that change what "match it" means:
Orsy is in the default build (`276fc95`); the board comes up in Dosh
(`00ad3e3`), which lives in `TaipoVariant::DEFAULT` inside the engine, so
`jolt` gets it for free; the LED rules moved into
`bbq_keyboard::indicator` (`6d21c26`) and the LEDs are rewritten on key
events, not on a timer (`94770df`); the layout is tickless through
`TimedLayout` (`92a37fa`).  The new `LayoutActions` methods
(`set_mod_state`, `set_orsy_pending`, `orsy_stroke`, `taipo_chord`,
`set_row_position`) all have default bodies, so `jolt`'s old `impl` still
compiles; it just ignores them.

## Building it today: what breaks, in order

Verified on a scratch copy of `jolt/` with a mesa3 shield written from the
table above:

1. **`b-proto4.sh` fails in the devicetree**:
   `proto4.overlay:50: undefined node label 'pwm_leds'`.  The Tiny 2040 PWM
   LED node was a local Zephyr patch, never upstreamed.  It only affects
   overlays that say `&pwm_leds` (proto4, and jolt3's workaround); **a mesa3
   shield does not touch it** and configures cleanly.
2. **bindgen**, in `zephyr-sys`'s build script:
   `arm_acle.h: load and store exclusive builtins are not available on this
   architecture`.  Xcode's clang 21 headers with Homebrew's libclang.  Fixed by
   lang-rust's `wbuild.sh` recipe: `LIBCLANG_PATH=/opt/homebrew/opt/llvm/lib`
   and `BINDGEN_EXTRA_CLANG_ARGS=-resource-dir=$(…/llvm/bin/clang
   -print-resource-dir)`.  The jolt scripts set neither.
3. **`rustapp`, 13 errors**: `#[panic_handler] function required` (add
   `zephyr-panic = "0.1.0"` and `use zephyr_panic as _;`), and six
   `spawner.spawn(task()).unwrap()` calls: under embassy-executor 0.10 the
   task function returns the `Result` and `spawn` returns `()`, so they become
   `spawner.spawn(task().unwrap())`.  The `Cargo.toml` bump is 0.7 → 0.10,
   0.6.2 → 0.8.0, 0.4 → 0.5.1; the `task-arena-size-2048` feature is dropped
   to match the upstream samples.
4. **clippy, 7 errors.**  lang-rust now runs `cargo clippy -- -D warnings -D
   clippy::undocumented_unsafe_blocks` as part of `ALL` (its
   `CMakeLists.txt`, unconditionally), and it fails the build even though the
   ELF and UF2 have already been written.  Six `unsafe` blocks without a
   `// SAFETY:` comment (`matrix.rs:52`; `lib.rs:75, 92, 334, 489, 552`) and
   one `count % 30000 == 0` that wants `is_multiple_of`.  That is the full list
   for the mesa3 configuration; `leds/pwm.rs` is not compiled there and may
   add more on boards that enable it.

With all four handled: exit 0, **FLASH 182,616 B, RAM 31,168 B**,
`zephyr.uf2` written.  The toolchain is SDK 1.0.0, which Zephyr main
requires; the root `.envrc`'s `zephyr-sdk-0.17.1` `PATH` entry is unused and
misleading.

That image would still **panic at boot on a mesa3**: `keyboard_task` picks
its key map by board name from `jolt/src/mapping.rs` and knows only
`proto4` and `jolt3`.

## Things on the Tiny 2040 that collide with the mesa3

The upstream `tiny2040.dts` enables, by default:

- **`uart0` on GP0/GP1**, as `zephyr,console` and `zephyr,shell-uart`, and
  `CONFIG_SERIAL=y` follows.  GP0/GP1 are `ROW_C`/`ROW_D`, driven lines.  The
  console is RTT (`prj.conf`), so the UART is unused, but its driver still
  applies its pinctrl at boot.  The shield must disable `uart0` and delete
  those two `chosen` properties.
- **`adc` with pinctrl on GP26–GP29**: the LED data line and three matrix
  pins.  `CONFIG_ADC` is off, so nothing applies it today; disable the node
  anyway so that never changes by accident.
- `i2c1` on GP2/GP3 is unused by the mesa3 and harmless.

## What the mesa3 needs that `jolt` does not have

Against `jolt-embassy-rp`, the behaviour to match (the same `bbq-keyboard`
engine underneath, so typing is identical by construction):

| feature | embassy | `jolt` today | needed |
|---|---|---|---|
| builds against today's workspace | — | no, the four fixes above | first |
| `mesa3` shield (serves `mesa3`/`mesa3b`/`mesa2b`) | `board.rs` | — | yes |
| scan-code translation | `bbq_keyboard::translate` by name | local `mapping.rs`, panics on mesa names (and wrong for proto4 at 1, 24, 29) | yes: use `translate` |
| layouts | taipo + orsy; qwerty/steno are features | qwerty + steno + taipo, no orsy, all hard-coded | yes |
| USB identity | `0xc0de:0xcafe`, "TangyBBQ" / "Jolt Keyboard", serial `name-` + 16 nibble letters | `0x2FE3:0x4201`, "BBQ", serial from hwinfo in Zephyr's format | **yes**: `keyminder`, TaipoTeacher's `USBTransport.swift` and the `hidutil` remap in `globe-key.md` all match on `0xc0de:0xcafe`, and TaipoTeacher tells boards apart by serial |
| 4 × ws2812 on GP26 | PIO + DMA | `led_strip.rs` is a stub; `CONFIG_LED_STRIP` is off | yes |
| LED rules: one modifier per LED, quarter brightness, Orsy white, Orsy pending cap/space | `bbq_keyboard::indicator` + `leds/manager.rs` | the old mode-colour sequence manager; no `set_mod_state` | yes |
| key log ring + minder on a vendor bulk interface (class `0xff`, 64-byte bulk IN/OUT) | `keylog.rs`, `minder.rs`, `usb.rs` | — | before a daily-driver mesa3 switches |
| tickless layout | `TimedLayout`, sleeps to the deadline | 1 ms `manager.tick` | no; USB powered, and identical typing either way |
| board info from flash at `0x101fff00` | `_board_info` | chosen node, same address | have |
| HID keyboard, 8-byte report, 1 ms | embassy-usb | new-stack `usbd_hid` | have |
| consumer control | pending | pending | later, both |
| Tiny 2040 onboard PWM LED, inter-board I2C/UART | — | — | no |

## The split-wireless question

Unchanged from the first version of this plan, and still deliberately not
decided here.  Everything below is independent of it: the engine, the LED
manager, the USB layer and the scanner do not change when a second MCU or a
radio appears; only a new shield and a link module do.  On the `jolt` route
BLE HID is a C shim over Zephyr's HOG service (the pattern `usb.c` already
uses), and the split link is either the wired `inter.rs` protocol over the
now-merged I2C bindings or a BLE central/peripheral pair.  `port-zmk.md`
stays usable for a later board.

---

# The plan

Each phase leaves something that builds.  Nothing can be flashed to a mesa3
until phase 2.

## Phase 1 — `jolt` builds on today's workspace

Goal: a `jolt` build exits 0 against the workspace as checked out.  This is
the recipe the scratch build proved, made permanent.

1. **Environment.**  Fold `LIBCLANG_PATH`/`BINDGEN_EXTRA_CLANG_ARGS` (Darwin
   branch of lang-rust's `wbuild.sh`) into the root `.envrc`; drop the
   `zephyr-sdk-0.17.1` `PATH` line; fix `jolt/check.sh`, which still sources
   the `jolt/.envrc` that moved in `ef3cbe8`.  The build scripts only source
   `.envrc` when `west` is missing, so in a direnv shell the exports come from
   direnv; either way they come from one place.
2. **Dependencies** (one commit; the compiler forces these together):
   embassy-executor 0.10 / embassy-sync 0.8 / embassy-time 0.5.1,
   `zephyr-panic`, the six `spawn` calls, regenerated `Cargo.lock`.
3. **Clippy clean**: `// SAFETY:` comments on the six `unsafe` blocks (real
   ones: why the busy-wait, the board-info read, the FFI calls are sound) and
   `is_multiple_of`.  From now on every `jolt` change has to keep clippy at
   zero warnings, or the build fails.
4. **lang-rust pin.**  After a `west update`, `jj new main@upstream` in
   `modules/lang/rust` (no longer `jolt-base`).  Have the build script refuse
   to start, with a message saying so, when that checkout lacks
   `zephyr/src/device/led_strip.rs`, since the cargo errors it would
   otherwise produce do not point at the cause.  Delete the lang-rust
   `jolt-base` bookmark and `jolt-base.md`, and update its `TASKS.md`.
   After the Zephyr release, a one-line PR moving the `optional.yaml` pin to
   a lang-rust commit at or after `6823b69` removes the routine for
   everyone.
5. **proto4**: not on the critical path any more.  Leave `b-proto4.sh`
   broken on `&pwm_leds`, and say so in its header comment, until open
   question 1 is answered.

## Phase 2 — the mesa3 shield, translation, and USB identity

Goal: a mesa3 types, in Dosh, through `jolt`.

1. Delete `jolt/src/mapping.rs`; translate with
   `bbq_keyboard::translate::get_translation(board_name)`.  Take `two_row`
   from the name, as `board.rs` does, and drop the `bbq,two-row` chosen node
   rather than keeping two sources.  An unknown name still panics with the
   name.
2. `bbqboards/boards/shields/mesa3/`: `Kconfig.shield`, `Kconfig.defconfig`
   (`CONFIG_LED_STRIP=y`, and the ws2812 PIO driver if it is not implied), and
   `mesa3.overlay` with the matrix from the table above (with the roles
   comment), `uart0` and `adc` disabled, the console `chosen` entries deleted,
   and the `ws2812` node on `pio0` at GP26 with `chain-length = <4>`, GRB, as
   in `jolt3_mez_2040.overlay`.  One shield for all three names, with the
   overlay's comment saying so, as `mod mesa3` does.
3. USB identity to match the embassy firmware: VID/PID `0xc0de:0xcafe`,
   manufacturer "TangyBBQ", product "Jolt Keyboard".  This comes before the
   key log because the `hidutil` remap and `keyminder` match on it.  The
   serial string's format waits for phase 5, which needs `unique` anyway.
4. `b-mesa3.sh`, shaped like `b-proto4.sh`, carrying the phase 1 lang-rust
   check.
5. Bring-up output is RTT over the test board's JTAG (`CONFIG_RTT_CONSOLE`,
   already in `prj.conf`).
6. Test on hardware: every key, both Fn keys (`mesa3b`), Dosh after reset,
   Orsy once phase 3 lands.  Until then the board comes up in the full
   build's modes, which is fine for "does every key type".

## Phase 3 — the embassy firmware's layout set

`rust_cargo_application()` passes no cargo features, so `jolt`'s own
`[features]` decide.  Default to **taipo + orsy** (`bbq-keyboard` features
`proto3, log, orsy`), with `qwerty` and `steno` as forwarding features, and
`#[cfg]`-gate `lib.rs` the way `jolt-embassy-rp/src/dispatch.rs` and
`main.rs` do: the steno thread and lookup task, `Event`/`Dict`,
`send_raw_steno`, the `LayoutMode::Qwerty`/`Steno`/`NKRO` arms, and
`Action::new`'s `LayoutMode::Qwerty`.  Check that `bbq-orsy` builds without
`std` on `thumbv6m` under Zephyr's allocator (it does for embassy, so it
should).  The jolt3 can keep a full build one feature away.

## Phase 4 — LEDs

1. `leds/led_strip.rs` over `zephyr::device::led_strip::LedStrip` from
   `chosen::bbq_led_strip`.  `update_rgb` blocks (PIO FIFO puts, then the
   280 µs reset), so run it on its own small thread fed by a signal, as
   `LedStripGroup::update_task` is its own task on embassy, so that a LED write
   cannot delay a matrix scan.
2. Replace `leds/manager.rs` with what `jolt-embassy-rp/src/leds/manager.rs`
   now is: a thin driver of `bbq_keyboard::indicator::Indicator`, rewritten
   on key events.  Implement `set_mode`, `set_mod_state` and
   `set_orsy_pending` on `Action` by forwarding to it.  Since the rules are
   shared, the port is the plumbing, not the rules.
3. Remove the `jolt3_mez_2040.overlay` `/delete-node/` workaround that #179
   made unnecessary (build-test it; the jolt3 is not on the test list).
4. Test on hardware: one-shot and sticky modifiers on their LEDs, Orsy white,
   Orsy's pending capital and in-word state, no flicker while typing.

## Phase 5 — key log and minder on the new USB stack

Not needed to type, but a daily driver's logs are TaipoTeacher's training
data, and a gap in them is not worth an early switch.

1. A vendor USB function, class `0xff`, one 64-byte bulk OUT and one bulk IN,
   as a `usbd` class in C.  The new stack has no generic vendor class;
   `samples/subsys/usb/webusb/src/sfunc.c` is a vendor function with bulk
   endpoints and is a closer template than `class/loopback.c`.  Expose
   read/write and completion callbacks to Rust the way `usb.c` does for HID.
2. Port `keylog.rs` as it is (`embassy-sync` and a critical section, both
   available).  Port `minder.rs` with: the endpoint traits replaced by the C
   functions; `ReadFlash`/`Hash`/`Program` over `zephyr::device::flash`;
   `Reset` by `sys_reboot`; `boot_id` from Zephyr's entropy (`sys_rand_get`)
   in place of `RoscRng`; `unique` in the embassy format (`name-` then 16
   nibble letters) from `hwinfo_get_device_id`, which on the RP2040 is the
   same flash unique ID.
3. Serial number descriptor from the same `unique` string, replacing
   `USBD_DESC_SERIAL_NUMBER_DEFINE`'s hwinfo format.
4. Done when `keyminder` and TaipoTeacher talk to a `jolt` mesa3 exactly as
   to an embassy one: same serial, the `mode` marker at boot, and a replayed
   day's log matching what was typed.

## Phase 6 — a mesa3 goes live

Flash a daily mesa3 and use it.  Same engine, same 1 ms scan, same
20-sample debounce, so chording should not differ; watch for USB report
latency under the new stack's HID class and for boot time.  Anything that
differs is a bug in this port, not a tuning question.  `jolt-embassy-rp`
stays flashable as the way back; the flash layout (board info at
`0x101fff00`) is the same for both.

## Later

- **Tickless**: move `keyboard_task` to `TimedLayout` and let the matrix
  wait on a GPIO interrupt when idle, as the embassy side does.  It only
  matters on battery, so it waits for the wireless board.
- **Consumer control**: `TASKS.md`'s pending item, for both firmwares.
- **Housekeeping**: `CLAUDE.md`/`AGENT.md` (`jolt` no longer
  "in-progress"; the sibling-checkout note gains
  `~/zephyrproject/modules/lang/rust` and the revision it must be at);
  `zbbq/` and `zephyr-play/` to `archive/`; `TASKS.md`'s "Jolt" and
  "proto4 support" leftovers folded in (the `PROTO4_MAPPING` bug is fixed by
  deleting the file in phase 2).

---

# Commit plan

One commit per numbered step unless the compiler forces two together, which
the commit message then says.  Hardware testing happens in review; each
response says what was built, what was tested, and what could not be.

1. Root `.envrc` and `check.sh`.  Dependencies + spawn + panic handler.
   Clippy.  lang-rust check in the build script.  *(4 commits; lang-rust
   cleanup is in that repository.)*  → a `jolt` build exits 0.
2. `translate` in place of `mapping.rs`.  `mesa3` shield.  USB identity.
   `b-mesa3.sh`.  *(4 commits.)*  → a mesa3 types.
3. Taipo + orsy features and cfg gating.  *(1–2 commits.)*
4. `led_strip.rs` backend.  Indicator-driven manager.  jolt3 workaround
   removal.  *(3 commits.)*
5. Vendor USB class.  Key log.  Minder and `unique`.  *(3–4 commits.)*  →
   TaipoTeacher talks to it.
6. The daily mesa3 switches.

---

# Open questions

Answered 2026-10-24: the test board is a second mesa3 with JTAG, both
boards are believed to be `mesa3b`, and the pin stays on `jj new` until
after the Zephyr release.  Still open:

1. **proto4 and the Tiny 2040 onboard PWM LED**: define the `pwm_leds` node in
   a shared `bbqboards` dtsi so `b-proto4.sh` builds again, upstream the
   board patch, or drop the onboard LED from `jolt` as the embassy firmware
   already has?  Nothing on the mesa path needs it.
2. **What happens to `jolt-embassy-rp`** once the mesas run on `jolt`: kept as
   the reference and the jolt3's firmware, or retired?  This decides whether
   consumer control is done twice.
