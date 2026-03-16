#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use bbq_keyboard::{
    Event,
    EventQueue,
    KeyAction,
    KeyEvent,
    Keyboard,
    LayoutMode,
    MinorMode,
    Mods,
    Timable,
    dict::Dict,
    layout::{LayoutActions, LayoutManager},
    usb_typer::{ActionHandler, enqueue_action},
};
use bbq_steno::Stroke;
use core::ffi::c_int;
use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    mutex::Mutex,
    semaphore::{GreedySemaphore, Semaphore},
};
use embassy_time::{Duration, Ticker};
use static_cell::StaticCell;
use zephyr::{
    device::gpio::GpioPin,
    devicetree::Value,
    embassy::Executor,
    printkln,
    raw::k_cycle_get_64,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

mod leds;
mod mapping;
mod matrix;

unsafe extern "C" {
    fn usb_setup() -> i32;
    fn usb_send_report(report: *const u8, len: u16) -> i32;
}

const HID_QUEUE_DEPTH: usize = 32;
const STENO_COMMAND_DEPTH: usize = 16;
const STENO_EVENT_DEPTH: usize = 16;
const STENO_THREAD_STACK_SIZE: usize = 8192;
const STENO_THREAD_PRIO: c_int = 5;

#[repr(C)]
struct HidReport {
    generation: usize,
    bytes: [u8; 8],
}

static HID_READY: AtomicBool = AtomicBool::new(false);
static HID_GENERATION: AtomicUsize = AtomicUsize::new(0);
static HID_REPORTS: Channel<CriticalSectionRawMutex, HidReport, HID_QUEUE_DEPTH> = Channel::new();
static HID_IN_FLIGHT: GreedySemaphore<CriticalSectionRawMutex> = GreedySemaphore::new(0);
static STENO_COMMANDS: Channel<CriticalSectionRawMutex, StenoCommand, STENO_COMMAND_DEPTH> = Channel::new();
static STENO_EVENTS: Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH> = Channel::new();
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
static LEDS: Mutex<CriticalSectionRawMutex, Option<leds::manager::LedManager>> = Mutex::new(None);
static ACTION: Action = Action::new();

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

    let ret = unsafe { usb_setup() };
    if ret != 0 {
        panic!("usb_setup failed: {}", ret);
    }

    let steno = steno_thread(&STENO_COMMANDS, &STENO_EVENTS);
    steno.set_priority(STENO_THREAD_PRIO);
    let _steno = steno.start();

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(main(spawner)).unwrap();
    })
}

fn get_board_info() -> Option<bbq_keyboard::boardinfo::BoardInfo> {
    let addr = board_info_addr();
    unsafe { bbq_keyboard::boardinfo::BoardInfo::decode_from_memory(addr as *const u8) }
}

fn board_info_addr() -> usize {
    match zephyr::devicetree::chosen::board_info::RAW_REG {
        [Value::Words([zephyr::devicetree::Word::Number(addr), ..])] => *addr as usize,
        _ => panic!("chosen board-info node does not provide a usable reg property"),
    }
}

#[embassy_executor::task]
async fn main(spawner: Spawner) -> () {
    let mut cols = Vec::new();
    extract_gpios(zephyr::devicetree::aliases::matrix::RAW_COL_GPIOS, &mut cols);
    printkln!("n columns: {}", cols.len());

    let mut rows = Vec::new();
    extract_gpios(zephyr::devicetree::aliases::matrix::RAW_ROW_GPIOS, &mut rows);
    printkln!("n rows: {}", rows.len());

    // Find the keyboard matrix definitions.
    printkln!("Cols: {:?}", rows);
    printkln!("Rows: {:?}", cols);

    let mut led_manager = leds::manager::LedManager::new(leds::LedSet::get_all());
    led_manager.clear_global(0);
    *LEDS.lock().await = Some(led_manager);

    spawner.spawn(usb_sender_task()).unwrap();
    spawner.spawn(steno_event_task(&ACTION)).unwrap();
    spawner.spawn(led_task()).unwrap();
    // Spawn a task to manage the keyboard matrix.
    spawner.spawn(keyboard_task(rows, cols)).unwrap();
}

/// A single executor to run most of the system. Runs in the main thread.
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn keyboard_task(rows: Vec<GpioPin>, cols: Vec<GpioPin>) -> () {
    let mut count = 0u64;
    let mut matrix = matrix::Matrix::new(rows, cols);
    let mut ticker = Ticker::every(embassy_time::Duration::from_millis(1));
    let mut manager = LayoutManager::new(true);
    loop {
        let mut events = Vec::new();
        matrix.scan(|code, pressed| {
            let code = mapping::PROTO4_MAPPING.get(code as usize).copied().unwrap_or_else(|| {
                panic!("Invalid code from matrix: {}", code);
            });
            let ev = if pressed {
                KeyEvent::Press(code)
            } else {
                KeyEvent::Release(code)
            };
            events.push(ev);
            // printkln!("Key {} {}", code, if pressed { "pressed" } else { "released" });
        });
        for ev in events {
            // printkln!("Event: {:?}", ev);
            manager.handle_event(ev, &ACTION).await;
        }
        manager.tick(&ACTION, 1).await;
        manager.poll();
        count += 1;
        if count % 30000 == 0 {
            printkln!("Keyboard task running: count={}", count);
        }
        ticker.next().await;
    }
}

enum StenoCommand {
    Lookup(Stroke),
}

struct Action {
    current_mode: AtomicUsize,
    raw_mode: AtomicBool,
}

impl Action {
    const fn new() -> Self {
        Self {
            current_mode: AtomicUsize::new(LayoutMode::Qwerty as usize),
            raw_mode: AtomicBool::new(false),
        }
    }

    async fn handle_steno_event(&self, event: Event) {
        match event {
            Event::RawMode(raw) => {
                self.raw_mode.store(raw, Ordering::Release);
                if self.current_mode.load(Ordering::Acquire) == LayoutMode::Steno as usize {
                    with_leds(|leds| leds.set_base(0, get_steno_indicator(raw))).await;
                }
                printkln!("Steno raw mode: {}", raw);
            }
            Event::StenoState(state) => {
                with_leds(|leds| leds.set_base(1, leds::manager::get_steno_state(&state))).await;
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
        self.current_mode.store(mode as usize, Ordering::Release);
        with_leds(|leds| leds.set_base(0, get_mode_indicator(mode, self.raw_mode.load(Ordering::Acquire)))).await;
        printkln!("Set mode: {:?}", mode);
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        with_leds(|leds| leds.set_base(0, get_mode_select_indicator(mode, self.raw_mode.load(Ordering::Acquire)))).await;
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
        let indicator = match &submode {
            MinorMode::ArtseyNav => &leds::manager::ARTSEY_NAV_INDICATOR,
        };
        with_leds(|leds| leds.set_base(1, indicator)).await;
        printkln!("Set submode: {:?}", submode);
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        with_leds(|leds| leds.set_base(1, &leds::manager::OFF_INDICATOR)).await;
        printkln!("Clear submode: {:?}", submode);
    }

    async fn send_raw_steno(&self, steno: Stroke) {
        if STENO_COMMANDS.try_send(StenoCommand::Lookup(steno)).is_err() {
            printkln!("Dropping steno stroke: {}", steno);
        }
    }
}

#[embassy_executor::task]
async fn led_task() -> ! {
    let mut ticker = Ticker::every(Duration::from_millis(100));
    loop {
        ticker.next().await;
        let mut leds = LEDS.lock().await;
        if let Some(leds) = leds.as_mut() {
            leds.tick();
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

        let ret = unsafe { usb_send_report(report.bytes.as_ptr(), report.bytes.len() as u16) };
        if ret != 0 {
            HID_IN_FLIGHT.release(1);
            printkln!("usb_send_report failed: {}", ret);
        }
    }
}

#[embassy_executor::task]
async fn steno_event_task(action: &'static Action) -> () {
    loop {
        let event = STENO_EVENTS.receive().await;
        action.handle_steno_event(event).await;
    }
}

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

#[zephyr::thread(stack_size = STENO_THREAD_STACK_SIZE)]
fn steno_thread(
    commands: &'static Channel<CriticalSectionRawMutex, StenoCommand, STENO_COMMAND_DEPTH>,
    events: &'static Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH>,
) -> ! {
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(steno_lookup_task(commands, events)).unwrap();
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

struct SubmitActionHandler;

impl ActionHandler for SubmitActionHandler {
    async fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for event in events {
            submit_key_action(event);
        }
    }
}

struct StenoEventSender(
    &'static Channel<CriticalSectionRawMutex, Event, STENO_EVENT_DEPTH>,
);

impl EventQueue for StenoEventSender {
    fn push(&mut self, val: Event) {
        let _ = self.0.try_send(val);
    }
}

struct WrapTimer;

impl Timable for WrapTimer {
    fn get_ticks(&self) -> u64 {
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

fn get_mode_indicator(mode: LayoutMode, raw: bool) -> &'static leds::manager::Indication {
    match mode {
        LayoutMode::StenoDirect => &leds::manager::STENO_DIRECT_INDICATOR,
        LayoutMode::Steno => get_steno_indicator(raw),
        LayoutMode::Artsey => &leds::manager::ARTSEY_INDICATOR,
        LayoutMode::Taipo => &leds::manager::TAIPO_INDICATOR,
        LayoutMode::Qwerty => &leds::manager::QWERTY_INDICATOR,
        LayoutMode::NKRO => &leds::manager::NKRO_INDICATOR,
    }
}

fn get_mode_select_indicator(mode: LayoutMode, raw: bool) -> &'static leds::manager::Indication {
    match mode {
        LayoutMode::StenoDirect => &leds::manager::STENO_DIRECT_SELECT_INDICATOR,
        LayoutMode::Steno => get_steno_select_indicator(raw),
        LayoutMode::Artsey => &leds::manager::ARTSEY_SELECT_INDICATOR,
        LayoutMode::Taipo => &leds::manager::TAIPO_SELECT_INDICATOR,
        LayoutMode::Qwerty => &leds::manager::QWERTY_SELECT_INDICATOR,
        LayoutMode::NKRO => &leds::manager::NKRO_SELECT_INDICATOR,
    }
}

fn get_steno_indicator(raw: bool) -> &'static leds::manager::Indication {
    if raw {
        &leds::manager::STENO_RAW_INDICATOR
    } else {
        &leds::manager::STENO_INDICATOR
    }
}

fn get_steno_select_indicator(raw: bool) -> &'static leds::manager::Indication {
    if raw {
        &leds::manager::STENO_RAW_SELECT_INDICATOR
    } else {
        &leds::manager::STENO_SELECT_INDICATOR
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
                    if *name != "gpio0" {
                        panic!("Unexpected GPIO controller name: {}", name);
                    }
                    out.push(unsafe {
                        GpioPin::raw_new(
                            zephyr::devicetree::labels::gpio0::get_instance_raw(),
                            zephyr::devicetree::labels::gpio0::get_static_raw(),
                            args[0],
                            args[1],
                        )
                    });
                }
            }
        }
    }
}
