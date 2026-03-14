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
    semaphore::{GreedySemaphore, Semaphore},
};
use embassy_time::Ticker;
use static_cell::StaticCell;
use zephyr::{
    device::gpio::GpioPin,
    devicetree::Value,
    embassy::Executor,
    printkln,
    raw::k_cycle_get_64,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

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
static ACTION: Action = Action::new();

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    printkln!("Jolt keyboard firmware");
    printkln!("Time tick: {}", zephyr::time::SYS_FREQUENCY);

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

    spawner.spawn(usb_sender_task()).unwrap();
    spawner.spawn(steno_event_task(&ACTION)).unwrap();
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
            printkln!("Event: {:?}", ev);
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
    raw_mode: AtomicBool,
}

impl Action {
    const fn new() -> Self {
        Self {
            raw_mode: AtomicBool::new(false),
        }
    }

    async fn handle_steno_event(&self, event: Event) {
        match event {
            Event::RawMode(raw) => {
                self.raw_mode.store(raw, Ordering::Release);
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
        printkln!("Set mode: {:?}", mode);
    }

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
        printkln!("Set submode: {:?}", submode);
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        printkln!("Clear submode: {:?}", submode);
    }

    async fn send_raw_steno(&self, steno: Stroke) {
        if STENO_COMMANDS.try_send(StenoCommand::Lookup(steno)).is_err() {
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
