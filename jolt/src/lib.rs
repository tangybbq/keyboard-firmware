#![no_std]

extern crate alloc;
use alloc::{string::String, vec::Vec};
#[cfg(feature = "steno")]
use bbq_keyboard::{
    Event, EventQueue, Timable,
    dict::Dict,
    usb_typer::{ActionHandler, enqueue_action},
};
use bbq_keyboard::{
    KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode, Mods,
    layout::{LayoutActions, LayoutManager, TimedLayout},
};
#[cfg(feature = "steno")]
use bbq_steno::Stroke;
use core::{cell::UnsafeCell, ffi::c_int};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    mutex::Mutex,
    semaphore::{GreedySemaphore, Semaphore},
    signal::Signal,
};
use embassy_time::{Instant, Ticker, Timer};
use static_cell::StaticCell;
#[cfg(feature = "steno")]
use zephyr::raw::k_cycle_get_64;
use zephyr::{
    device::gpio::GpioPin,
    devicetree::Value,
    embassy::Executor,
    printkln,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use zephyr_panic as _;

use ::minder::keylog::Marker;
use bbq_keyboard::layout::taipo::TaipoVariant;

mod keylog;
mod leds;
mod matrix;
mod minder;
mod minder_usb;

unsafe extern "C" {
    fn usb_setup(serial: *const u8, serial_len: usize) -> i32;
    fn jolt_device_id(id: *mut u8, len: usize) -> c_int;
    fn usb_send_report(report: *const u8, len: u16) -> i32;
}

const HID_QUEUE_DEPTH: usize = 32;
#[cfg(feature = "steno")]
const STENO_COMMAND_DEPTH: usize = 16;
#[cfg(feature = "steno")]
const STENO_EVENT_DEPTH: usize = 16;
#[cfg(feature = "steno")]
const STENO_THREAD_STACK_SIZE: usize = 8192;
#[cfg(feature = "steno")]
const STENO_THREAD_PRIO: c_int = 5;

#[repr(C)]
struct HidReport {
    generation: usize,
    bytes: [u8; 8],
}

/// The report the USB stack is sending.  `hid_device_submit_report` does not copy the report: the
/// buffer has to stay put, and pointer aligned, until `input_report_done`.  So the one report in
/// flight lives here rather than in `usb_sender_task`, which is the only writer and writes only
/// while it holds the `HID_IN_FLIGHT` permit that the done callback gives back.
#[repr(C, align(4))]
struct InFlightReport(UnsafeCell<[u8; 8]>);

// SAFETY: the only access is from `usb_sender_task`, serialized with the USB stack's reads by the
// `HID_IN_FLIGHT` permit, as described on the type.
unsafe impl Sync for InFlightReport {}

static HID_READY: AtomicBool = AtomicBool::new(false);
static HID_GENERATION: AtomicUsize = AtomicUsize::new(0);
static HID_REPORTS: Channel<CriticalSectionRawMutex, HidReport, HID_QUEUE_DEPTH> = Channel::new();
static HID_IN_FLIGHT: GreedySemaphore<CriticalSectionRawMutex> = GreedySemaphore::new(0);
static HID_IN_FLIGHT_REPORT: InFlightReport = InFlightReport(UnsafeCell::new([0; 8]));
#[cfg(feature = "steno")]
static STENO_COMMANDS: Channel<CriticalSectionRawMutex, StenoCommand, STENO_COMMAND_DEPTH> =
    Channel::new();
#[cfg(feature = "steno")]
static STENO_EVENTS: Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH> = Channel::new();
#[cfg(feature = "steno")]
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static LEDS: Mutex<CriticalSectionRawMutex, Option<leds::manager::LedManager>> = Mutex::new(None);
static ACTION: Action = Action;
static LAYOUT: StaticCell<Mutex<CriticalSectionRawMutex, TimedLayout>> = StaticCell::new();
/// Tells `layout_task` that a key event may have moved the layout's deadline.
static LAYOUT_WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// How many times `layout_task` has woken, for `keyboard_task` to report.
static LAYOUT_WAKES: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    printkln!("Jolt keyboard firmware");
    printkln!("Time tick: {}", zephyr::time::SYS_FREQUENCY);
    let addr = board_info_addr();
    let board_info = get_board_info();
    match &board_info {
        Some(info) => printkln!("Board info @ {:#x}: {:?}", addr, info),
        None => printkln!("Board info decode failed @ {:#x}", addr),
    }
    let board_info = board_info.expect("Board info not present");

    let unique = get_unique(&board_info.name);
    printkln!("Unique ID: {}", unique);

    // SAFETY: `usb_setup` touches only usb.c's own state, and this is its one call, made before the
    // executor starts the tasks that send reports.  It copies the serial number before returning.
    let ret = unsafe { usb_setup(unique.as_ptr(), unique.len()) };
    if ret != 0 {
        panic!("usb_setup failed: {}", ret);
    }

    #[cfg(feature = "steno")]
    {
        let steno = steno_thread(&STENO_COMMANDS, &STENO_EVENTS);
        steno.set_priority(STENO_THREAD_PRIO);
        let _steno = steno.start();
    }

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(main(spawner, unique).unwrap());
    })
}

fn get_board_info() -> Option<bbq_keyboard::boardinfo::BoardInfo> {
    let addr = board_info_addr();
    // SAFETY: the address is the `reg` of the chosen board-info node, a 256-byte block in the
    // memory-mapped flash, which is always readable.
    unsafe { bbq_keyboard::boardinfo::BoardInfo::decode_from_memory(addr as *const u8) }
}

/// The keyboard's identity, as jolt-embassy-rp's `get_unique` forms it: the board name, a dash,
/// and the device's 64-bit unique ID as 16 letters `a`..`p`, low nibble first.  It is the USB
/// serial number, which TaipoTeacher tells keyboards apart by, so a keyboard has to give the same
/// string under both firmwares.
fn get_unique(name: &str) -> &'static str {
    let mut id = [0u8; 8];
    // SAFETY: `id` is writable for its whole length, which is what is passed.
    let ret = unsafe { jolt_device_id(id.as_mut_ptr(), id.len()) };
    if ret != id.len() as c_int {
        panic!("Device ID unavailable: {}", ret);
    }

    let mut unique = String::with_capacity(name.len() + 1 + 16);
    unique.push_str(name);
    unique.push('-');
    let mut tmp = u64::from_le_bytes(id);
    for _ in 0..16 {
        unique.push((b'a' + (tmp & 0x0f) as u8) as char);
        tmp >>= 4;
    }
    unique.leak()
}

fn board_info_addr() -> usize {
    match zephyr::devicetree::chosen::board_info::RAW_REG {
        [Value::Words([zephyr::devicetree::Word::Number(addr), ..])] => *addr as usize,
        _ => panic!("chosen board-info node does not provide a usable reg property"),
    }
}

#[embassy_executor::task]
async fn main(spawner: Spawner, unique: &'static str) -> () {
    let mut cols = Vec::new();
    extract_gpios(
        zephyr::devicetree::aliases::matrix::RAW_COL_GPIOS,
        &mut cols,
    );
    printkln!("n columns: {}", cols.len());

    let mut rows = Vec::new();
    extract_gpios(
        zephyr::devicetree::aliases::matrix::RAW_ROW_GPIOS,
        &mut rows,
    );
    printkln!("n rows: {}", rows.len());

    // Find the keyboard matrix definitions.
    printkln!("Cols: {:?}", rows);
    printkln!("Rows: {:?}", cols);

    *LEDS.lock().await = Some(leds::manager::LedManager::new(leds::LedSet::get_all()));

    spawner.spawn(usb_sender_task().unwrap());
    spawner.spawn(minder::minder_task(unique).unwrap());
    spawner.spawn(minder::test_event_task().unwrap());
    spawner.spawn(minder::reset_task().unwrap());
    #[cfg(feature = "steno")]
    spawner.spawn(steno_event_task(&ACTION).unwrap());
    // Spawn a task to manage the keyboard matrix.
    spawner.spawn(keyboard_task(spawner, rows, cols).unwrap());
}

/// A single executor to run most of the system. Runs in the main thread.
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

/// How many scans in a row, 1 ms apart, with no key down before the matrix goes back to waiting
/// for a key.  Waiting is cheap to start, so this needn't be long; as on jolt-embassy-rp.
const IDLE_SCANS: u32 = 500;

#[embassy_executor::task]
async fn keyboard_task(spawner: Spawner, rows: Vec<GpioPin>, cols: Vec<GpioPin>) -> () {
    let board_info = get_board_info().expect("Board info not present");
    let board_name = board_info.name.as_str();
    // The scan code tables are shared with jolt-embassy-rp and the host tools, and panic on a
    // name they do not know.
    let xlate = bbq_keyboard::translate::get_translation(board_name);
    let two_row = is_two_row(board_name);
    printkln!("Board: {}, two_row: {}", board_name, two_row);

    let mut matrix = matrix::Matrix::new(rows, cols);
    let layout = LAYOUT.init(Mutex::new(TimedLayout::new(
        LayoutManager::new(two_row),
        Instant::now().as_millis(),
    )));
    spawner.spawn(layout_task(layout).unwrap());
    // What the last active stretch cost, reported at the next wake.
    let mut active_scans = 0usize;
    let mut active_layout_wakes = 0usize;
    loop {
        // With no key down there is nothing to scan until one goes down.
        let idle_start = Instant::now();
        let layout_wakes = LAYOUT_WAKES.load(Ordering::Relaxed);
        matrix.key_wait().await;

        // A check that the keyboard sleeps when idle: while the matrix waited, nothing should have
        // woken the layout.  One line per wake from idle, so at most one per burst of typing.
        let idle_layout_wakes = LAYOUT_WAKES.load(Ordering::Relaxed) - layout_wakes;
        printkln!(
            "Matrix wake: idle {} ms, {} layout wakes; last active {} scans, {} layout wakes",
            idle_start.elapsed().as_millis(),
            idle_layout_wakes,
            active_scans,
            active_layout_wakes,
        );
        let layout_wakes = LAYOUT_WAKES.load(Ordering::Relaxed);
        active_scans = 0;

        let mut ticker = Ticker::every(embassy_time::Duration::from_millis(1));
        let mut idle_scans = 0;
        while idle_scans < IDLE_SCANS {
            active_scans += 1;
            let mut events = Vec::new();
            matrix.scan(|code, pressed| {
                let mapped = xlate(code);
                if mapped == 255 {
                    // Dead key position — no physical switch here.
                    return;
                }
                let ev = if pressed {
                    KeyEvent::Press(mapped)
                } else {
                    KeyEvent::Release(mapped)
                };
                events.push(ev);
                // printkln!("Key {} {}", mapped, if pressed { "pressed" } else { "released" });
            });
            for ev in events {
                // printkln!("Event: {:?}", ev);
                if ev.is_press() {
                    with_leds(|leds| leds.refresh()).await;
                }
                // Logged before the layout sees it.  A no-op until a host turns logging on.
                keylog::log_key(ev.key(), ev.is_press());
                // Every event has to signal: one that doesn't can leave `layout_task` asleep past a
                // chord's deadline.
                layout
                    .lock()
                    .await
                    .handle_event(ev, Instant::now().as_millis(), &ACTION)
                    .await;
                LAYOUT_WAKE.signal(());
            }
            if matrix.pressed() == 0 {
                idle_scans += 1;
            } else {
                idle_scans = 0;
            }
            ticker.next().await;
        }
        active_layout_wakes = LAYOUT_WAKES.load(Ordering::Relaxed) - layout_wakes;
    }
}

/// Run the layout's timers.
///
/// The layout only needs time to pass while a timer is running, which is only while a chord is
/// being built.  So this sleeps until the layout's deadline, or indefinitely if there is none.  A
/// key event can create, move or remove the deadline, so every event signals `LAYOUT_WAKE`, and
/// the loop throws away what it was waiting on and asks again.  The deadline is only read under
/// the lock, never carried across a wait, so the loop can't act on an old one.  A spurious wake
/// does no harm: the timers only fire once their time is up.  This is jolt-embassy-rp's
/// `layout_loop`, without the LEDs' mode flash.
#[embassy_executor::task]
async fn layout_task(layout: &'static Mutex<CriticalSectionRawMutex, TimedLayout>) -> () {
    // Announces the initial mode.
    layout
        .lock()
        .await
        .wake(Instant::now().as_millis(), &ACTION)
        .await;

    loop {
        let deadline = layout.lock().await.deadline();
        let due = async {
            match deadline {
                Some(deadline) => Timer::at(Instant::from_millis(deadline)).await,
                None => core::future::pending().await,
            }
        };

        let woke = select(due, LAYOUT_WAKE.wait()).await;
        LAYOUT_WAKES.fetch_add(1, Ordering::Relaxed);
        match woke {
            Either::First(()) => {
                layout
                    .lock()
                    .await
                    .wake(Instant::now().as_millis(), &ACTION)
                    .await;
            }
            // An event may have moved the deadline; go round and read it again.
            Either::Second(()) => (),
        }
    }
}

/// Is this one of the boards with two rows of finger keys?  These have fewer keys, and select modes
/// differently.  The same per-board answer as `two_row` in jolt-embassy-rp's `board.rs`.
fn is_two_row(board_name: &str) -> bool {
    matches!(
        board_name,
        "proto4" | "mesa1" | "mesa2" | "mesa2b" | "mesa3" | "mesa3b" | "mesa3w"
    )
}

#[cfg(feature = "steno")]
enum StenoCommand {
    Lookup(Stroke),
}

struct Action;

impl Action {
    /// The steno raw mode and the dictionary's cap and space state each had an LED indication,
    /// which the modifier indicator has no room for, as on jolt-embassy-rp; they are only logged.
    #[cfg(feature = "steno")]
    async fn handle_steno_event(&self, event: Event) {
        match event {
            Event::RawMode(raw) => {
                printkln!("Steno raw mode: {}", raw);
            }
            Event::StenoState(state) => {
                printkln!("Steno state: {:?}", state);
            }
            other => {
                printkln!("Unexpected steno event: {:?}", other);
            }
        }
    }
}

impl LayoutActions for Action {
    async fn set_mode(&self, mode: LayoutMode) {
        keylog::log_marker(Marker::Mode, mode.marker());
        with_leds(|leds| leds.set_mode(mode)).await;
        printkln!("Set mode: {:?}", mode);
    }

    /// Nothing to show: there is no mode LED to flash for the selection.
    async fn set_mode_select(&self, mode: LayoutMode) {
        printkln!("Set mode select: {:?}", mode);
    }

    async fn send_key(&self, key: KeyAction) {
        match key {
            KeyAction::KeyPress(key, mods) => {
                submit_report(keypress_report(key, mods));
            }
            KeyAction::ModOnly(mods) => {
                submit_report([modifier_bits(mods), 0, 0, 0, 0, 0, 0, 0]);
            }
            KeyAction::KeyRelease => {
                submit_report([0; 8]);
            }
            KeyAction::KeySet(keys) => {
                submit_report(keyset_report(&keys));
            }
            KeyAction::Stall => {
                printkln!("USB stall action");
            }
        }
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Dosh => keylog::log_marker(Marker::Variant, TaipoVariant::Dosh.marker()),
        }
        printkln!("Set submode: {:?}", submode);
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Dosh => keylog::log_marker(Marker::Variant, TaipoVariant::Taipo.marker()),
        }
        printkln!("Clear submode: {:?}", submode);
    }

    async fn set_row_position(&self, lower: bool) {
        keylog::log_marker(Marker::RowShift, lower as u8);
    }

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        with_leds(|leds| leds.set_mods(oneshot, sticky)).await;
    }

    #[cfg(feature = "orsy")]
    async fn set_orsy_pending(&self, cap: bool, in_word: bool) {
        with_leds(|leds| leds.set_orsy_pending(cap, in_word)).await;
    }

    #[cfg(feature = "steno")]
    async fn send_raw_steno(&self, steno: Stroke) {
        if STENO_COMMANDS
            .try_send(StenoCommand::Lookup(steno))
            .is_err()
        {
            printkln!("Dropping steno stroke: {}", steno);
        }
    }
}

fn submit_report(report: [u8; 8]) {
    let report = HidReport {
        generation: HID_GENERATION.load(Ordering::Acquire),
        bytes: report,
    };

    if HID_REPORTS.try_send(report).is_err() {
        printkln!("Dropping HID report: queue full");
    }
}

#[embassy_executor::task]
async fn usb_sender_task() -> () {
    loop {
        let report = HID_REPORTS.receive().await;

        if report.generation != HID_GENERATION.load(Ordering::Acquire)
            || !HID_READY.load(Ordering::Acquire)
        {
            continue;
        }

        let permit = HID_IN_FLIGHT.acquire(1).await.unwrap();
        permit.disarm();

        if report.generation != HID_GENERATION.load(Ordering::Acquire)
            || !HID_READY.load(Ordering::Acquire)
        {
            HID_IN_FLIGHT.release(1);
            continue;
        }

        let buf = HID_IN_FLIGHT_REPORT.0.get();
        // SAFETY: holding the permit means no report is in flight, so the USB stack is not
        // reading the buffer, and this task is its only writer.
        unsafe { *buf = report.bytes };
        // SAFETY: `buf` is 8 aligned bytes in a static, and nothing writes them again until
        // `input_report_done` returns the permit.
        let ret = unsafe { usb_send_report(buf.cast::<u8>(), 8) };
        if ret != 0 {
            HID_IN_FLIGHT.release(1);
            printkln!("usb_send_report failed: {}", ret);
        }
    }
}

#[cfg(feature = "steno")]
#[embassy_executor::task]
async fn steno_event_task(action: &'static Action) -> () {
    loop {
        let event = STENO_EVENTS.receive().await;
        action.handle_steno_event(event).await;
    }
}

#[cfg(feature = "steno")]
#[embassy_executor::task]
async fn steno_lookup_task(
    commands: &'static Channel<CriticalSectionRawMutex, StenoCommand, STENO_COMMAND_DEPTH>,
    events: &'static Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH>,
) -> ! {
    let mut dict = Dict::new();
    let mut event_send = StenoEventSender(events);

    let mut state = dict.state();
    events.send(Event::StenoState(state.clone())).await;

    loop {
        match commands.receive().await {
            StenoCommand::Lookup(stroke) => {
                for action in dict.handle_stroke(stroke, &mut event_send, &WrapTimer) {
                    type_joined(action).await;
                }

                let new_state = dict.state();
                if state != new_state {
                    state = new_state;
                    events.send(Event::StenoState(state.clone())).await;
                }
            }
        }
    }
}

#[cfg(feature = "steno")]
#[zephyr::thread(stack_size = STENO_THREAD_STACK_SIZE)]
fn steno_thread(
    commands: &'static Channel<CriticalSectionRawMutex, StenoCommand, STENO_COMMAND_DEPTH>,
    events: &'static Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH>,
) -> ! {
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(steno_lookup_task(commands, events).unwrap());
    })
}

#[unsafe(no_mangle)]
extern "C" fn usb_iface_ready_callback(ready: bool) {
    HID_READY.store(ready, Ordering::Release);
    HID_GENERATION.fetch_add(1, Ordering::AcqRel);
    HID_IN_FLIGHT.set(if ready { 1 } else { 0 });
}

#[unsafe(no_mangle)]
extern "C" fn usb_input_report_done_callback() {
    if HID_READY.load(Ordering::Acquire) {
        HID_IN_FLIGHT.release(1);
    }
}

fn keypress_report(key: Keyboard, mods: Mods) -> [u8; 8] {
    [modifier_bits(mods), 0, key as u8, 0, 0, 0, 0, 0]
}

fn keyset_report(keys: &[Keyboard]) -> [u8; 8] {
    let mut report = [0u8; 8];
    for (index, key) in keys.iter().take(6).enumerate() {
        report[index + 2] = *key as u8;
    }
    report
}

fn modifier_bits(mods: Mods) -> u8 {
    let mut bits = 0u8;
    if mods.contains(Mods::CONTROL) {
        bits |= 0x01;
    }
    if mods.contains(Mods::SHIFT) {
        bits |= 0x02;
    }
    if mods.contains(Mods::ALT) {
        bits |= 0x04;
    }
    if mods.contains(Mods::GUI) {
        bits |= 0x08;
    }
    bits
}

#[cfg(feature = "steno")]
async fn type_joined(action: bbq_steno::dict::Joined) {
    match action {
        bbq_steno::dict::Joined::Type { remove, append } => {
            for _ in 0..remove {
                submit_key_action(KeyAction::KeyPress(
                    Keyboard::DeleteBackspace,
                    Mods::empty(),
                ));
                submit_key_action(KeyAction::KeyRelease);
            }
            enqueue_action(&mut SubmitActionHandler, &append).await;
        }
    }
}

#[cfg(feature = "steno")]
fn submit_key_action(key: KeyAction) {
    match key {
        KeyAction::KeyPress(key, mods) => {
            submit_report(keypress_report(key, mods));
        }
        KeyAction::ModOnly(mods) => {
            submit_report([modifier_bits(mods), 0, 0, 0, 0, 0, 0, 0]);
        }
        KeyAction::KeyRelease => {
            submit_report([0; 8]);
        }
        KeyAction::KeySet(keys) => {
            submit_report(keyset_report(&keys));
        }
        KeyAction::Stall => {
            printkln!("USB stall action");
        }
    }
}

#[cfg(feature = "steno")]
struct SubmitActionHandler;

#[cfg(feature = "steno")]
impl ActionHandler for SubmitActionHandler {
    async fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for event in events {
            submit_key_action(event);
        }
    }
}

#[cfg(feature = "steno")]
struct StenoEventSender(&'static Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH>);

#[cfg(feature = "steno")]
impl EventQueue for StenoEventSender {
    fn push(&mut self, val: Event) {
        let _ = self.0.try_send(val);
    }
}

#[cfg(feature = "steno")]
struct WrapTimer;

#[cfg(feature = "steno")]
impl Timable for WrapTimer {
    fn get_ticks(&self) -> u64 {
        // SAFETY: reads the cycle counter; no preconditions.
        unsafe { k_cycle_get_64() }
    }
}

async fn with_leds<F>(f: F)
where
    F: FnOnce(&mut leds::manager::LedManager),
{
    let mut leds = LEDS.lock().await;
    if let Some(manager) = leds.as_mut() {
        f(manager);
    }
}

/// Extract GPIO from the devicetree data.
///
/// As we don't have support yet for exporting the pins directly from the
/// devicetree, extrat them from the raw data.
fn extract_gpios(values: &[Value], out: &mut Vec<GpioPin>) {
    for value in values {
        if let Value::Words(words) = value {
            for elt in *words {
                if let zephyr::devicetree::Word::Gpio(name, args) = elt {
                    printkln!("GPIO: {} {:?}", name, args);
                    // The RP2040 has one GPIO controller; the nRF52840 has two ports, and the
                    // mesa3w's matrix uses both.
                    let pin = match *name {
                        // SAFETY: the device and its `GpioStatic` are gpio0's, both static, and
                        // the pin and flags are devicetree cells for that controller, so they
                        // fit its `gpio_pin_t` and `gpio_dt_flags_t`.
                        "gpio0" => unsafe {
                            GpioPin::raw_new(
                                zephyr::devicetree::labels::gpio0::get_instance_raw(),
                                zephyr::devicetree::labels::gpio0::get_static_raw(),
                                args[0],
                                args[1],
                            )
                        },
                        // SAFETY: as for gpio0, with gpio1's device and `GpioStatic`.
                        #[cfg(dt = "labels::gpio1")]
                        "gpio1" => unsafe {
                            GpioPin::raw_new(
                                zephyr::devicetree::labels::gpio1::get_instance_raw(),
                                zephyr::devicetree::labels::gpio1::get_static_raw(),
                                args[0],
                                args[1],
                            )
                        },
                        other => panic!("Unexpected GPIO controller name: {}", other),
                    };
                    out.push(pin);
                }
            }
        }
    }
}
