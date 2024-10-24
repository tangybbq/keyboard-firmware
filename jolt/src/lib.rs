// Copyright (c) 2024 Linaro LTD
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![allow(unexpected_cfgs)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use bbq_keyboard::boardinfo::BoardInfo;
use bbq_steno::dict::Joined;
use leds::LedSet;
use zephyr::sync::{Arc, Mutex};

use core::cell::RefCell;
use core::{mem, slice};

use log::info;

use matrix::Matrix;
use zephyr::{kobj_define, printkln};
use zephyr::device::uart::LineControl;
use zephyr::sync::channel::{
    self,
    Sender,
    Receiver,
    Message,
};

use bbq_keyboard::{
    dict::Dict,
    Event,
    EventQueue,
    InterState,
    Keyboard,
    KeyAction,
    KeyEvent,
    layout::LayoutManager,
    LayoutMode,
    Mods,
    Side,
    Timable,
    UsbDeviceState,
    usb_typer::{enqueue_action, ActionHandler},
};
use bbq_steno::Stroke;

#[allow(unused_imports)]
use crate::inter::InterHandler;
use crate::leds::manager::LedManager;

mod devices;
mod inter;
mod leds;
mod matrix;
mod translate;

#[no_mangle]
extern "C" fn rust_main() {
    printkln!("Hello world from Rust on {}",
              zephyr::kconfig::CONFIG_BOARD);

    zephyr::set_logger();

    // Initialize the main event queue.
    let (equeue_send, equeue_recv) = channel::unbounded::<Event>();

    // This is the steno queue.
    let (stenoq_send, stenoq_recv) = channel::unbounded::<Stroke>();

    let stats = Arc::new(Stats::new());

    // Spawn the steno thread.
    // TODO: This needs to be lower priority.
    let sc = equeue_send.clone();
    let statsc = stats.clone();
    let mut thread = STENO_THREAD.init_once(STENO_STACK.init_once(()).unwrap()).unwrap();
    thread.set_priority(5);
    thread.set_name(c"steno");
    thread.spawn(move || {
        steno_thread(stenoq_recv, sc, statsc);
    });

    unsafe {
        // Store a sender for the USB callback.
        USB_CB_MAIN_SEND = Some(equeue_send.clone());
        // Store a sender for the Heartbeat callback.
        HEARTBEAT_MAIN_SEND = Some(equeue_send.clone());
    }

    add_heartbeat_box();

    // After the callbacks have the queue handles, we can start the heartbeat.
    setup_heartbeat();

    // Retrieve our information.
    let side_data = (zephyr::kconfig::CONFIG_FLASH_BASE_ADDRESS + 2*1024*1024 - 256) as *const u8;
    let info = unsafe { BoardInfo::decode_from_memory(side_data) }.expect("Board info not present");

    // Retrieve the side select.
    // For now, if we are a single setup, consider that the "left" side,
    // which will avoid any bias of the scancodes.
    let side = info.side.unwrap_or(Side::Left);
    printkln!("Our side: {:?}", side);

    // Initialize USB HID.
    usb_setup();

    // Is this the best way to do this?  These aren't that big.
    let rows = zephyr::devicetree::aliases::matrix::get_rows();
    let cols = zephyr::devicetree::aliases::matrix::get_cols();

    // Build a Vec for these.
    let rows: Vec<_> = rows.into_iter().map(|p| p.unwrap()).collect();
    let cols: Vec<_> = cols.into_iter().map(|p| p.unwrap()).collect();

    let matrix = Matrix::new(rows, cols, side);
    let mut scanner = Scanner::new(matrix, equeue_send.clone(), &info);

    let mut layout = LayoutManager::new();

    let leds = LedSet::get_all();
    let mut leds = LedManager::new(leds, stats.clone());

    let mut inter = get_inter(side, equeue_send.clone());

    let mut acm = zephyr::devicetree::labels::acm_uart_0::get_instance().unwrap();
    let mut acm_active;

    let mut eq_send = SendWrap(equeue_send.clone());
    let mut keys = VecDeque::new();

    // TODO: We should really ask for the current mode, instead of hoping to align them.
    let mut current_mode = LayoutMode::Steno;
    let mut state = InterState::Idle;
    // let mut suspended = true;
    // let mut woken = false;
    let mut has_global = true;

    let mut heap_counter = 0;
    let mut stat_counter = 0;

    let mut led_counter = 0;

    loop {
        // Update the state of the Gemini indicator.
        if let Ok(1) =  unsafe { acm.line_ctrl_get(LineControl::DTR) } {
            leds.set_base(2, &leds::manager::GEMINI_INDICATOR);
            acm_active = true;
        } else {
            leds.set_base(2, &leds::manager::OFF_INDICATOR);
            acm_active = false;
        }

        let ev = equeue_recv.recv().unwrap();

        let mut is_tick = false;
        match ev {
            Event::Tick => is_tick = true,
            Event::Matrix(key) => {
                // info!("Matrix: {:?}", key);
                match state {
                    InterState::Primary | InterState::Idle => {
                        stats.start("layout");
                        layout.handle_event(key, &mut eq_send);
                        stats.stop("layout");
                    }
                    InterState::Secondary => {
                        if let Some(ref mut inter) = inter {
                            inter.add_key(key);
                        }
                    }
                }
            }

            Event::Key(key) => {
                // Keypresses are queued up, to be sent to the hid layer.
                keys.push_back(key);
            }

            Event::InterKey(key) => {
                if state == InterState::Primary {
                    stats.start("layout");
                    layout.handle_event(key, &mut eq_send);
                    stats.stop("layout");
                }
            }

            Event::RawSteno(stroke) => {
                if current_mode == LayoutMode::Steno {
                    // TODO: Send a steno stroke
                    stenoq_send.send(stroke).unwrap();
                } else {
                    // Send Gemini data if possible.
                    if acm_active {
                        // Put as much as we can in the FIFO.  This should be drained if active.
                        // TODO: Better management.
                        // TODO: Do the tx enable tx disable stuff.
                        let packet = stroke.to_gemini();
                        // Deal with errors and such.
                        match unsafe { acm.fifo_fill(&packet) } {
                            Ok(_) => (),
                            Err(_) => (),
                        }
                    }
                }
            }

            // Once the steno layer has translated the strokes, it gives us a TypeAction to send
            // off to HID.
            Event::StenoText(Joined::Type { remove, append }) => {
                for _ in 0..remove {
                    keys.push_back(KeyAction::KeyPress(Keyboard::DeleteBackspace, Mods::empty()));
                    keys.push_back(KeyAction::KeyRelease);
                }
                // Then, just send the text.
                enqueue_action(&mut KeyActionWrap(&mut keys), &append);
            }

            // Mode select and mode affect the LEDs.
            Event::ModeSelect(mode) => {
                info!("modeselect: {:?}", mode);
                let next = match mode {
                    LayoutMode::Steno => &leds::manager::STENO_SELECT_INDICATOR,
                    LayoutMode::StenoRaw => &leds::manager::STENO_RAW_SELECT_INDICATOR,
                    LayoutMode::Taipo => &leds::manager::TAIPO_SELECT_INDICATOR,
                    LayoutMode::Qwerty => &leds::manager::QWERTY_SELECT_INDICATOR,
                    _ => &leds::manager::QWERTY_SELECT_INDICATOR,
                };
                leds.set_base(0, next);
            }

            // Mode select and mode affect the LEDs.
            Event::Mode(mode) => {
                info!("modeselect: {:?}", mode);
                let next = match mode {
                    LayoutMode::Steno => &leds::manager::STENO_INDICATOR,
                    LayoutMode::StenoRaw => &leds::manager::STENO_RAW_INDICATOR,
                    LayoutMode::Taipo => &leds::manager::TAIPO_INDICATOR,
                    LayoutMode::Qwerty => &leds::manager::QWERTY_INDICATOR,
                    _ => &leds::manager::QWERTY_INDICATOR,
                };
                leds.set_base(0, next);
                current_mode = mode;
            }

            // Handle the USB becoming configured.
            Event::UsbState(UsbDeviceState::Configured) | Event::UsbState(UsbDeviceState::Resume) => {
                if has_global {
                    leds.clear_global(0);
                    has_global = false;
                }
                // suspended = false;
                if let Some(ref mut inter) = inter {
                    inter.set_state(bbq_keyboard::InterState::Primary);
                }
            }

            Event::UsbState(UsbDeviceState::Suspend) => {
                leds.set_global(0, &leds::manager::SLEEP_INDICATOR);
                has_global = true;
                // suspended = true;
                // woken = false;
            }

            Event::BecomeState(new_state) => {
                if state != new_state {
                    if new_state == InterState::Secondary {
                        leds.clear_global(0);
                    } else if new_state == InterState::Idle {
                        leds.clear_global(0);
                    }
                }
                state = new_state;
            }

            Event::Heartbeat => {
            }

            ev => {
                printkln!("Event: {:?}", ev);
            }
        }

        // Only continue when the tick is received.
        if !is_tick {
            continue;
        }

        stats.start("matrix");
        scanner.scan();
        stats.stop("matrix");

        stats.start("layout.tick");
        layout.tick(&mut eq_send);
        stats.stop("layout.tick");
        usb_hid_push(&mut keys);

        if let Some(ref mut inter) = inter {
            stats.start("inter");
            inter.tick();
            stats.stop("inter");
        }

        // Update the LEDs every 100ms.
        led_counter += 1;
        if led_counter >= 100 {
            led_counter = 0;
            leds.tick();
        }

        // Print out heap stats every few minutes.
        heap_counter += 1;
        if heap_counter >= 120_000 {
            heap_counter = 0;
            show_heap_stats();
        }

        // Print out other stats periodically as well.
        stat_counter += 1;
        if stat_counter >= 60_000 {
            stat_counter = 0;
            stats.start("stats");
            stats.show();
            stats.stop("stats");

            #[cfg(CONFIG_THREAD_ANALYZER)]
            {
                unsafe {
                    zephyr::raw::thread_analyzer_print(0);
                }
            }
        }

        // After processing the main loop, generate a message for the tick irq handler.  This will
        // allow ticks to be missed if processing takes too long.
        add_heartbeat_box();
    }
}

/// Conditionally return the inter-board code.
#[cfg(CONFIG_JOLT_INTER)]
fn get_inter(side: Side, equeue_send: Sender<Event>) -> Option<InterHandler> {
    let uart = zephyr::devicetree::chosen::inter_board_uart::get_instance().unwrap();
    Some(InterHandler::new(side, uart, equeue_send))
}

#[cfg(not(CONFIG_JOLT_INTER))]
fn get_inter(_side: Side, _equeue_send: Sender<Event>) -> Option<InterHandler> {
    None
}

/// Scanner.
///
/// The scanner is responsible for receiving scan events from the keyboard, as well as from the
/// other side.
struct Scanner {
    matrix: Matrix,
    events: Sender<Event>,
    translate: fn (u8) -> u8,
}

impl Scanner {
    fn new(matrix: Matrix, events: Sender<Event>, info: &BoardInfo) -> Scanner {
        let translate = translate::get_translation(&info.name);
        Scanner { matrix, events, translate }
    }

    fn scan(&mut self) {
        self.matrix.scan(|code, press| {
            let code = (self.translate)(code);
            let event = if press {
                KeyEvent::Press(code)
            } else {
                KeyEvent::Release(code)
            };
            self.events.send(Event::Matrix(event)).unwrap();
        });
    }
}

/// Push usb-hid events to the USB stack, when possible.
fn usb_hid_push(keys: &mut VecDeque<KeyAction>) {
    if !devices::hid_is_accepting() {
        return;
    }

    if let Some(key) = keys.pop_front() {
        match key {
            KeyAction::KeyPress(code, mods) => {
                let code = code as u8;
                devices::hid_send_keyboard_report(mods.bits(), slice::from_ref(&code));
            }
            KeyAction::KeyRelease => {
                devices::hid_send_keyboard_report(0, &[]);
            }
            KeyAction::KeySet(keys) => {
                // TODO We don't handle more than 6 keys, which qwerty mode can do.  For now, just
                // report if we can.
                let (mods, keys) = keyset_to_hid(keys);
                devices::hid_send_keyboard_report(mods.bits(), &keys);
            }
            KeyAction::ModOnly(mods) => {
                devices::hid_send_keyboard_report(mods.bits(), &[]);
            }
            KeyAction::Stall => (),
        }
    }
}

// Qwerty mode just sends scan codes, but not the mod bits as expected by the HID layer.  To fix
// this, convert the codes from QWERTY into a proper formatted data for a report.
fn keyset_to_hid(keys: Vec<Keyboard>) -> (Mods, Vec<u8>) {
    let mut result = Vec::new();
    let mut mods = Mods::empty();
    for key in keys {
        match key {
            Keyboard::LeftControl => mods |= Mods::CONTROL,
            Keyboard::LeftShift => mods |= Mods::SHIFT,
            Keyboard::LeftAlt => mods |= Mods::ALT,
            Keyboard::LeftGUI => mods |= Mods::GUI,
            key => result.push(key as u8),
        }
    }
    (mods, result)
}

struct KeyActionWrap<'a>(&'a mut VecDeque<KeyAction>);

impl<'a> ActionHandler for KeyActionWrap<'a> {
    fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for act in events {
            self.0.push_back(act);
        }
    }
}

/// The lower priority steno lookup thread.
fn steno_thread(recv: Receiver<Stroke>, events: Sender<Event>, stats: Arc<Stats>) {
    printkln!("Steno thread running");
    let mut dict = Dict::new();
    loop {
        let stroke = recv.recv().unwrap();
        stats.start("steno");
        for action in dict.handle_stroke(stroke, &WrapTimer) {
            // Enqueue the action, and the actual typing will be queued up by the main thread.
            events.send(Event::StenoText(action)).unwrap();
        }
        stats.stop("steno");
    }
}

struct WrapTimer;

impl Timable for WrapTimer {
    fn get_ticks(&self) -> u64 {
        unsafe { zephyr::raw::k_cycle_get_64() }
    }
}

/// A wrapper around a Sender to implement the EventQueue trait.
struct SendWrap(Sender<Event>);

impl EventQueue for SendWrap {
    fn push(&mut self, val: Event) {
        self.0.send(val).unwrap();
    }
}

/// Event queue sender for main queue.  Written once during init, and should be safe to just
/// directly use.
static mut USB_CB_MAIN_SEND: Option<Sender<Event>> = None;

/// Rust USB callback.
#[no_mangle]
extern "C" fn rust_usb_status(state: u32) {
    let send = unsafe { USB_CB_MAIN_SEND.as_mut().unwrap() };

    let state = match state {
        0 => UsbDeviceState::Configured,
        1 => UsbDeviceState::Suspend,
        2 => UsbDeviceState::Resume,
        _ => unreachable!(),
    };
    send.send(Event::UsbState(state)).unwrap();
}

static mut HEARTBEAT_MAIN_SEND: Option<Sender<Event>> = None;
static HEARTBEAT_BOX: critical_section::Mutex<RefCell<Option<Box<Message<Event>>>>> =
    critical_section::Mutex::new(RefCell::new(None));

#[no_mangle]
extern "C" fn rust_heartbeat() {
    let send = unsafe { HEARTBEAT_MAIN_SEND.as_ref().unwrap() };
    let boxed = critical_section::with(|cs| {
        HEARTBEAT_BOX.borrow_ref_mut(cs).take()
    });
    // Send it, if there was one there to send.
    if let Some(boxed) = boxed {
        unsafe {
            send.send_boxed(boxed).unwrap();
        }
    }
}

/// Give the heartbeat IRQ a box holding a message it can send.
fn add_heartbeat_box() {
    let tick = Box::new(Message::new(Event::Tick));
    critical_section::with(|cs| {
        HEARTBEAT_BOX.borrow(cs).replace(Some(tick));
    });
}

/// Initialize the USB.
fn usb_setup() {
    unsafe {
        use core::ffi::c_int;

        extern "C" {
            fn usb_setup() -> c_int;
        }

        if usb_setup() != 0 {
            panic!("Unable to initialize USB");
        }
    }
}

/// Initialize the heartbeat.
fn setup_heartbeat() {
    unsafe {
        extern "C" {
            fn setup_heartbeat();
        }

        setup_heartbeat();
    }
}

/// Show heap stats.
fn show_heap_stats() {
    unsafe {
        extern "C" {
            fn show_heap_stats();
        }

        show_heap_stats();
    }
}

/// Statistics gathered.
pub struct Stats(Mutex<BTreeMap<&'static str, StatInfo>>);

#[derive(Debug)]
struct StatInfo {
    start: Option<u64>,
    samples: usize,
    best: u64,
    worst: u64,
    total: u64,
}

impl Default for StatInfo {
    fn default() -> Self {
        StatInfo {
            start: None,
            samples: 0,
            best: u64::MAX,
            worst: 0,
            total: 0,
        }
    }
}

impl Stats {
    fn new() -> Stats {
        Stats(Mutex::new(BTreeMap::new()))
    }

    pub fn start(&self, name: &'static str) {
        let mut lock = self.0.lock().unwrap();
        let entry = lock.entry(name).or_default();
        if entry.start.is_some() {
            panic!("Stats::start double use");
        }
        entry.start = Some(Self::get_ticks());
    }

    pub fn stop(&self, name: &'static str) {
        let mut lock = self.0.lock().unwrap();
        let entry = lock.entry(name).or_default();
        if let Some(start) = entry.start {
            let elapsed = Self::get_ticks() - start;

            entry.samples += 1;
            entry.best = entry.best.min(elapsed);
            entry.worst = entry.worst.max(elapsed);
            entry.total += elapsed;

            entry.start = None;
        } else {
            // Ignore this.  This happens if the stats are printed while something is performing.
        }
    }

    fn show(&self) {
        let state = {
            let mut lock = self.0.lock().unwrap();
            // Swap the tree out, allowing other threads to run with the fresh stats.  The drop should
            let state = mem::replace(&mut *lock, BTreeMap::new());

            // Capture any entries that have a "start", so we don't lose that.
            for (k, v) in &state {
                if let Some(start) = v.start {
                    lock.insert(k, StatInfo {
                        start: Some(start),
                        ..Default::default()
                    });
                }
            }

            state
        };

        info!("Stats:");
        for (k, v) in state.iter() {
            if v.samples == 0 {
                // Don't try to print stats if there aren't any.  This happens if the stats print
                // while an operation is running for the first time.  For example, this always
                // happens when calculating the stats for stats.
                continue;
            }
            info!(": {:<12}: best:{}, worst:{}, avg:{}",
                  k,
                  StatInfo::humanize(v.best as f64),
                  StatInfo::humanize(v.worst as f64),
                  StatInfo::humanize(v.total as f64 / v.samples as f64),
                  );
        }
    }

    /// Read the current cycle count.
    fn get_ticks() -> u64 {
        unsafe {
            zephyr::raw::k_cycle_get_64()
        }
    }
}

impl StatInfo {
    fn humanize(time: f64) -> String {
        const TICKS: f64 = zephyr::kconfig::CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC as f64;

        let time = time / TICKS;

        if time >= 1.0 {
            format!("{:8.03}s ", time)
        } else if time >= 1.0e-3 {
            format!("{:8.03}ms", time * 1.0e3)
        } else if time >= 1.0e-6 {
            format!("{:8.03}us", time * 1.0e6)
        } else {
            format!("{:8.03}ns", time * 1.0e9)
        }
    }
}

kobj_define! {
    // The steno thread.
    static STENO_THREAD: StaticThread;
    static STENO_STACK: ThreadStack<4096>;
}
