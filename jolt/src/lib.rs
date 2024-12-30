// Copyright (c) 2024 Linaro LTD
// SPDX-License-Identifier: Apache-2.0

#![no_std]
#![allow(unexpected_cfgs)]

// As of Rust 1.83, Rust warns about shared references to mutable statics.  Technically, it is
// correct about this, but as I'm doing the initialization once at the start, and then using them
// read only later, it is actually fine.  Well, really, there is a problem that there should be a
// memory barrier after the write.
//
// But, this should be changed to something actually shared, perhaps an atomic pointer, or such,
// just to make it clearer what is going on.
//
// TODO:
// For now, suppress the warning, although this will probably become an error in future Rust
// versions.
#![allow(static_mut_refs)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use bbq_keyboard::boardinfo::BoardInfo;
use bbq_steno::dict::Joined;
use keyminder::Minder;
use leds::manager::Indication;
use leds::LedSet;
use logging::Logger;
use zephyr::kio::yield_now;
use zephyr::sync::Arc;
use zephyr::sys::sync::Semaphore;
use zephyr::time::{Duration, NoWait};
use zephyr::work::futures::sleep;
use zephyr::work::WorkQueueBuilder;

use core::slice;

use log::{info, warn};

use matrix::Matrix;
use zephyr::{kobj_define, printkln};
use zephyr::device::uart::LineControl;
use zephyr::sync::channel::{
    self,
    Sender,
    Receiver,
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
use crate::inter::{InterHandler, InterUpdate};
use crate::leds::manager::LedManager;

mod devices;
mod inter;
mod keyminder;
mod leds;
mod logging;
mod matrix;
mod translate;

#[no_mangle]
extern "C" fn rust_main() {
    printkln!("Hello world from Rust on {}",
              zephyr::kconfig::CONFIG_BOARD);

    let logger = Logger::new();

    // Initialize the main event queue.
    let (equeue_send, equeue_recv) = channel::bounded::<Event>(32);

    // This is the steno queue.
    let (stenoq_send, stenoq_recv) = channel::bounded::<Stroke>(32);

    // The heartbeat semaphore.
    let heart = Arc::new(Semaphore::new(1, 1).unwrap());

    unsafe {
        HEARTBEAT_SEM = Some(heart.clone());
    }

    // Spawn the steno thread.
    // TODO: This needs to be lower priority.
    let sc = equeue_send.clone();
    let mut thread = STENO_THREAD.init_once(STENO_STACK.init_once(()).unwrap()).unwrap();
    thread.set_priority(5);
    thread.set_name(c"steno");
    thread.spawn(move || {
        steno_thread(stenoq_recv, sc);
    });

    // Create a thread to run the main worker.
    // No yield saves a trip through the scheduler, as this is the only thread running at this
    // priority.
    let main_worker = Box::new(WorkQueueBuilder::new()
                               .set_priority(2)
                               .set_name(c"mainloop")
                               .set_no_yield(true)
                               .start(MAIN_LOOP_STACK.init_once(()).unwrap()));

    // The 'inter' worker runs on its own thread, lower priority than the main loop.  As this
    // computation takes longer than a frame, this allows the regular periodic work to continue to
    // run, as the inter work takes several frames.
    let inter_worker = Box::new(WorkQueueBuilder::new()
                                .set_priority(3)
                                .set_name(c"interwork")
                                .set_no_yield(true)
                                .start(INTER_STACK.init_once(()).unwrap()));

    unsafe {
        // Store a sender for the USB callback.
        USB_CB_MAIN_SEND = Some(equeue_send.clone());
        // Store a sender for the Heartbeat callback.
        HEARTBEAT_MAIN_SEND = Some(equeue_send.clone());
    }

    // After the callbacks have the queue handles, we can start the heartbeat.
    setup_heartbeat();

    // Retrieve our information.
    let side_data = (zephyr::kconfig::CONFIG_FLASH_BASE_ADDRESS + 2*1024*1024 - 256) as *const u8;
    let info = unsafe { BoardInfo::decode_from_memory(side_data) }.expect("Board info not present");

    // Retrieve the side select.
    // For now, if we are a single setup, consider that the "left" side,
    // which will avoid any bias of the scancodes.
    let side = info.side.unwrap_or(Side::Left);
    info!("Our side: {:?}", side);

    // Initialize USB HID.
    let usb = Arc::new(devices::usb::Usb::new().unwrap());

    // Is this the best way to do this?  These aren't that big.
    let rows = zephyr::devicetree::aliases::matrix::get_rows();
    let cols = zephyr::devicetree::aliases::matrix::get_cols();

    // Build a Vec for these.
    let rows: Vec<_> = rows.into_iter().map(|p| p.unwrap()).collect();
    let cols: Vec<_> = cols.into_iter().map(|p| p.unwrap()).collect();

    let matrix = Matrix::new(rows, cols, side);
    let scanner = Scanner::new(matrix, equeue_send.clone(), &info);

    // TODO: When we have definable DT properties, use the DT.  For now, just match names.
    let two_row = match info.name.as_str() {
        "proto4" => true,
        _ => false,
    };
    let layout = LayoutManager::new(two_row);

    // Queue for layout events.  These should be processed readily, so this doesn't need to be
    // large.
    let (lm_send, lm_recv) = channel::bounded(2);

    let _ = zephyr::kio::spawn(layout_task(layout, lm_recv, equeue_send.clone()), &main_worker);

    let leds = LedSet::get_all();
    let mut leds = LedManager::new(leds);

    let (inter_task, inter) = get_inter(side, equeue_send.clone()).unzip();

    let mut acm = zephyr::devicetree::labels::acm_uart_0::get_instance().unwrap();

    let minder_uart = zephyr::devicetree::labels::acm_uart_1::get_instance().unwrap();

    let minder_uart = unsafe { minder_uart.into_irq().unwrap() };

    let _minder = Minder::new(minder_uart, logger);

    let mut keys = VecDeque::new();

    // TODO: We should really ask for the current mode, instead of hoping to align them.
    let mut current_mode = LayoutMode::Steno;
    let mut state = InterState::Idle;
    let mut raw_mode = false;
    // let mut suspended = true;
    // let mut woken = false;
    let mut has_global = true;

    let mut heap_counter = 0;

    let mut led_counter = 0;

    // The scanner just runs periodically to scan the matrix.
    let _ = zephyr::kio::spawn(scanner.run(), &main_worker);

    // Startup the inter-update, if it exists.
    let _ = inter_task.map(|inter_task| {
        zephyr::kio::spawn(inter_task.run(), &inter_worker)
    });

    let main_loop = async move {
        let mut acm_active;
        loop {
            // Update the state of the Gemini indicator.
            if let Ok(1) =  unsafe { acm.line_ctrl_get(LineControl::DTR) } {
                leds.set_base(2, &leds::manager::GEMINI_INDICATOR);
                acm_active = true;
            } else {
                leds.set_base(2, &leds::manager::OFF_INDICATOR);
                acm_active = false;
            }

            let ev = equeue_recv.recv_async().await.unwrap();

            let mut is_tick = false;
            match ev {
                Event::Tick => is_tick = true,
                Event::Matrix(key) => {
                    // info!("Matrix: {:?}", key);
                    match state {
                        InterState::Primary | InterState::Idle => {
                            if lm_send.try_send(key).is_err() {
                                warn!("Key event dropped {:?}", key);
                            }
                        }
                        InterState::Secondary => {
                            if let Some(inter) = &inter {
                                if key.is_valid() {
                                    inter.send(InterUpdate::AddKey(key)).unwrap();
                                }
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
                        if lm_send.try_send(key).is_err() {
                            warn!("Key even dropped {:?}", key);
                        }
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
                        // Also, send to the HID Report descriptor.
                        usb.send_plover_report(&stroke.to_plover_hid());
                        usb.send_plover_report(&Stroke::empty().to_plover_hid());
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
                    // info!("modeselect: {:?}", mode);
                    let next = match mode {
                        LayoutMode::Steno => get_steno_select_indicator(raw_mode),
                        LayoutMode::StenoDirect => &leds::manager::STENO_DIRECT_SELECT_INDICATOR,
                        LayoutMode::Taipo => &leds::manager::TAIPO_SELECT_INDICATOR,
                        LayoutMode::Qwerty => &leds::manager::QWERTY_SELECT_INDICATOR,
                        _ => &leds::manager::QWERTY_SELECT_INDICATOR,
                    };
                    leds.set_base(0, next);
                }

                // Mode select and mode affect the LEDs.
                Event::Mode(mode) => {
                    info!("mode: {:?}", mode);
                    let next = match mode {
                        LayoutMode::Steno => get_steno_indicator(raw_mode),
                        LayoutMode::StenoDirect => &leds::manager::STENO_DIRECT_INDICATOR,
                        LayoutMode::Taipo => &leds::manager::TAIPO_INDICATOR,
                        LayoutMode::Qwerty => &leds::manager::QWERTY_INDICATOR,
                        _ => &leds::manager::QWERTY_INDICATOR,
                    };
                    leds.set_base(0, next);
                    current_mode = mode;
                }

                Event::RawMode(raw) => {
                    info!("Switch raw: {:?}", raw);
                    raw_mode = raw;
                    if current_mode == LayoutMode::Steno {
                        leds.set_base(0, get_steno_indicator(raw_mode))
                    }
                }

                // Handle the USB becoming configured.
                Event::UsbState(UsbDeviceState::Configured) | Event::UsbState(UsbDeviceState::Resume) => {
                    if has_global {
                        leds.clear_global(0);
                        has_global = false;
                    }
                    // suspended = false;
                    if let Some(inter) = &inter {
                        inter.send(InterUpdate::SetState(bbq_keyboard::InterState::Primary)).unwrap();
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

            yield_now().await;

            // Only continue when the tick is received.
            if !is_tick {
                continue;
            }

            // Read in the HID out report if we have been sent one.
            let mut buf = [0u8; 8];
            if let Ok(Some(count)) = usb.get_keyboard_report(&mut buf) {
                info!("Keyboard out: {} bytes: {:?}", count, &buf[..count]);
            }

            yield_now().await;

            usb_hid_push(&usb, &mut keys);

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

            // After we process the heartbeat, give to the semaphore so we will get the next tick.  This
            // keeps ticks from building up and only enqueues a tick if the main loop made it through
            // everything.
            heart.give();

            // Yield to reschedule the work.
            yield_now().await;
        }
    };

    let main_loop = zephyr::kio::spawn(main_loop, &main_worker);

    // Wait for the main loop.  This should never happen.
    let () = main_loop.join();

    // Leak the box so the worker is never freed.
    let _ = Box::leak(main_worker);
    let _ = Box::leak(inter_worker);
}

fn get_steno_indicator(raw: bool) -> &'static Indication {
    if raw {
        &leds::manager::STENO_RAW_INDICATOR
    } else {
        &leds::manager::STENO_INDICATOR
    }
}

fn get_steno_select_indicator(raw: bool) -> &'static Indication {
    if raw {
        &leds::manager::STENO_RAW_SELECT_INDICATOR
    } else {
        &leds::manager::STENO_SELECT_INDICATOR
    }
}

/// The layout task.
///
/// Waits for events to be sent to the layout task, invoking the handler for those, and running the
/// periodic tick to handle various timeouts.
///
/// This task never returns.
///
/// TODO: A better versions of this would be able to know when to wake up instead of having to run
/// every tick, only to do nothing.
///
/// TODO: This still sends things back to the main event queue, just to be forwarded on to something
/// else.
async fn layout_task(
    // The layout manager to manage.
    mut layout: LayoutManager,
    // A receiver for the queue that processes layout events.
    keys: Receiver<KeyEvent>,
    // The main event queue.
    events: Sender<Event>,
) {
    let mut events = SendWrap(events);
    zephyr::event_loop!(keys, Duration::millis_at_least(1),
                        Some(ev) => { layout.handle_event(ev, &mut events) },
                        None => { layout.tick(&mut events) },
    );
}

/// Conditionally return the inter-board code.
#[cfg(dt = "chosen::inter_board_uart")]
fn get_inter(side: Side, equeue_send: Sender<Event>) -> Option<(InterHandler, Sender<InterUpdate>)> {
    let uart = zephyr::devicetree::chosen::inter_board_uart::get_instance().unwrap();
    Some(InterHandler::new(side, uart, equeue_send))
}

#[cfg(not(dt = "chosen::inter_board_uart"))]
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

    async fn run(mut self) {
        loop {
            // TODO: Use an absolute timer here.
            sleep(Duration::millis_at_least(1)).await;

            self.scan();
        }
    }
}

/// Push usb-hid events to the USB stack, when possible.
fn usb_hid_push(usb: &devices::usb::Usb, keys: &mut VecDeque<KeyAction>) {

    while let Some(key) = keys.pop_front() {
        match key {
            KeyAction::KeyPress(code, mods) => {
                let code = code as u8;
                usb.send_keyboard_report(mods.bits(), slice::from_ref(&code));
            }
            KeyAction::KeyRelease => {
                usb.send_keyboard_report(0, &[]);
            }
            KeyAction::KeySet(keys) => {
                // TODO We don't handle more than 6 keys, which qwerty mode can do.  For now, just
                // report if we can.
                let (mods, keys) = keyset_to_hid(keys);
                usb.send_keyboard_report(mods.bits(), &keys);
            }
            KeyAction::ModOnly(mods) => {
                usb.send_keyboard_report(mods.bits(), &[]);
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
fn steno_thread(recv: Receiver<Stroke>, events: Sender<Event>) {
    printkln!("Steno thread running");
    let mut eq_send = SendWrap(events.clone());
    let mut dict = Dict::new();
    loop {
        let stroke = recv.recv().unwrap();
        for action in dict.handle_stroke(stroke, &mut eq_send, &WrapTimer) {
            // Enqueue the action, and the actual typing will be queued up by the main thread.
            events.send(Event::StenoText(action)).unwrap();
        }
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
pub fn rust_usb_status(state: u32) {
    let send = unsafe { USB_CB_MAIN_SEND.as_mut().unwrap() };

    let state = match state {
        0 => UsbDeviceState::Configured,
        1 => UsbDeviceState::Suspend,
        2 => UsbDeviceState::Resume,
        _ => unreachable!(),
    };
    send.send(Event::UsbState(state)).unwrap();
}

/// A reference into the main event loop for the heartbeat irq to use.
static mut HEARTBEAT_MAIN_SEND: Option<Sender<Event>> = None;

/// A semaphore so sync the heartbeat with the processing.
static mut HEARTBEAT_SEM: Option<Arc<Semaphore>> = None;
 
#[no_mangle]
extern "C" fn rust_heartbeat() {
    let send = unsafe { HEARTBEAT_MAIN_SEND.as_ref().unwrap() };

    // If we can get the sem, then it is safe to send another tick.
    // Otherwise, skip this tick.
    let sem = unsafe { HEARTBEAT_SEM.as_ref().unwrap() };
    if sem.take(NoWait).is_err() {
        return;
    }

    let _ = send.try_send(Event::Tick);
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

kobj_define! {
    // The steno thread.
    static STENO_THREAD: StaticThread;
    static STENO_STACK: ThreadStack<4096>;

    // The main loop thread.
    static MAIN_LOOP_STACK: ThreadStack<2048>;

    // A thread for the inter-worker.  Allows this to run at lower priority to prevent stalls.
    static INTER_STACK: ThreadStack<2048>;
}
