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

use embassy_executor::Spawner;
use static_cell::StaticCell;
use zephyr::embassy::Executor;

use log::info;

extern crate alloc;

// use logging::Logger;

// use log::{info, warn};

/*
#[allow(unused_imports)]
use crate::inter::{InterHandler, InterUpdate};
*/

/*
mod devices;
mod dispatch;
mod inter;
mod keyminder;
mod leds;
mod logging;
mod matrix;
mod translate;
*/

static EXECUTOR_MAIN: StaticCell<Executor> = StaticCell::new();

#[no_mangle]
extern "C" fn rust_main() {
    // TODO: Logging through USB.
    unsafe {
        zephyr::set_logger().unwrap();
    }

    info!("Hello world from Rust on {}", zephyr::kconfig::CONFIG_BOARD);

    // Become our executor.
    info!(
        "Starting Embassy executor on {}",
        zephyr::kconfig::CONFIG_BOARD,
    );
    let executor = EXECUTOR_MAIN.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(main(spawner)).unwrap();
    })

    // let logger = Logger::new();

    /*
    // Initialize the main event queue.
    let (equeue_send, equeue_recv) = channel::bounded::<Event>(32);

    // The heartbeat semaphore.
    let heart = Arc::new(Semaphore::new(1, 1).unwrap());

    unsafe {
        HEARTBEAT_SEM = Some(heart.clone());
    }

    // The 'inter' worker runs on its own thread, lower priority than the main loop.  As this
    // computation takes longer than a frame, this allows the regular periodic work to continue to
    // run, as the inter work takes several frames.
    let inter_worker = Box::new(
        WorkQueueBuilder::new()
            .set_priority(3)
            .set_name(c"interwork")
            .set_no_yield(true)
            .start(INTER_STACK.init_once(()).unwrap()),
    );

    unsafe {
        // Store a sender for the USB callback.
        USB_CB_MAIN_SEND = Some(equeue_send.clone());
        // Store a sender for the Heartbeat callback.
        HEARTBEAT_MAIN_SEND = Some(equeue_send.clone());
    }

    // After the callbacks have the queue handles, we can start the heartbeat.
    setup_heartbeat();

    // Retrieve our information.
    let side_data =
        (zephyr::kconfig::CONFIG_FLASH_BASE_ADDRESS + 2 * 1024 * 1024 - 256) as *const u8;
    let info = unsafe { BoardInfo::decode_from_memory(side_data) }.expect("Board info not present");

    // Retrieve the side select.
    // For now, if we are a single setup, consider that the "left" side,
    // which will avoid any bias of the scancodes.
    let side = info.side.unwrap_or(Side::Left);
    info!("Our side: {:?}, name: {:?}", side, info.name);

    // Initialize USB HID.
    let usb = devices::usb::Usb::new().unwrap();

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

    let leds = LedSet::get_all();
    let leds = LedManager::new(leds);

    let dispatch = DispatchBuilder {
        equeue_send: equeue_send.clone(),
        usb,
        leds,
    }
    .build();

    // Queue for layout events.  These should be processed readily, so this doesn't need to be
    // large.
    let (lm_send, lm_recv) = channel::bounded(32);

    let _ = zephyr::kio::spawn(
        layout_task(layout, lm_recv, dispatch.clone()),
        &dispatch.main_worker,
        c"w:layout",
    );

    let (inter_task, inter) = get_inter(side, equeue_send.clone()).unzip();

    /*
    let mut acm = zephyr::devicetree::labels::acm_uart_0::get_instance().unwrap();
    */

    let minder_uart = zephyr::devicetree::labels::acm_uart_1::get_instance().unwrap();

    let minder_uart = unsafe { minder_uart.into_irq().unwrap() };

    let _minder = Minder::new(minder_uart, logger);

    // TODO: We should really ask for the current mode, instead of hoping to align them.
    let mut state = InterState::Idle;
    // let mut suspended = true;
    // let mut woken = false;
    let mut has_global = true;

    let mut heap_counter = 0;

    let mut led_counter = 0;

    // The scanner just runs periodically to scan the matrix.
    let _ = zephyr::kio::spawn(scanner.run(), &dispatch.main_worker, c"w:scanner");

    // Startup the inter-update, if it exists.
    let _ = inter_task
        .map(|inter_task| zephyr::kio::spawn(inter_task.run(), &inter_worker, c"w:inter"));

    // Temp, need a copy to spawn this main loop.
    let dispatch2 = dispatch.clone();

    let main_loop = async move {
        // let mut acm_active;
        loop {
            /*
            // TODO: We currently aren't using Gemini, but this would be nice to have if it ever
            // returns.
            // Update the state of the Gemini indicator.
            {
                let mut leds = leds.lock().unwrap();
                if let Ok(1) = unsafe { acm.line_ctrl_get(LineControl::DTR) } {
                    leds.set_base(2, &leds::manager::GEMINI_INDICATOR);
                    acm_active = true;
                } else {
                    leds.set_base(2, &leds::manager::OFF_INDICATOR);
                    acm_active = false;
                }
            }
            */

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

                Event::InterKey(key) => {
                    if state == InterState::Primary {
                        if lm_send.try_send(key).is_err() {
                            warn!("Key even dropped {:?}", key);
                        }
                    }
                }

                Event::RawMode(raw) => {
                    info!("Switch raw: {:?}", raw);
                    *dispatch.raw_mode.lock().unwrap() = raw;
                    if *dispatch.current_mode.lock().unwrap() == LayoutMode::Steno {
                        dispatch.leds.lock()
                            .unwrap()
                            .set_base(0, get_steno_indicator(raw))
                    }
                }

                // Handle the USB becoming configured.
                Event::UsbState(UsbDeviceState::Configured)
                | Event::UsbState(UsbDeviceState::Resume) => {
                    if has_global {
                        dispatch.leds.lock().unwrap().clear_global(0);
                        has_global = false;
                    }
                    // suspended = false;
                    if let Some(inter) = &inter {
                        inter
                            .send(InterUpdate::SetState(bbq_keyboard::InterState::Primary))
                            .unwrap();
                    }
                }

                Event::UsbState(UsbDeviceState::Suspend) => {
                    dispatch.leds.lock()
                        .unwrap()
                        .set_global(0, &leds::manager::SLEEP_INDICATOR);
                    has_global = true;
                    // suspended = true;
                    // woken = false;
                }

                Event::BecomeState(new_state) => {
                    if state != new_state {
                        if new_state == InterState::Secondary {
                            dispatch.leds.lock().unwrap().clear_global(0);
                        } else if new_state == InterState::Idle {
                            dispatch.leds.lock().unwrap().clear_global(0);
                        }
                    }
                    state = new_state;
                }

                Event::Heartbeat => {}

                ev => {
                    printkln!("Event: {:?}", ev);
                }
            }

            yield_now().await;

            // Only continue when the tick is received.
            if !is_tick {
                continue;
            }

            // Update the LEDs every 100ms.
            led_counter += 1;
            if led_counter >= 100 {
                led_counter = 0;
                dispatch.leds.lock().unwrap().tick();
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

    let main_loop = zephyr::kio::spawn(main_loop, &dispatch2.main_worker, c"w:main");

    // Wait for the main loop.  This should never happen.
    let () = main_loop.join();
    */
}

#[embassy_executor::task]
async fn main(spawner: Spawner) {
    let _ = spawner;
    info!("Main thread running");
}

/*
// TODO: Does this move to Dispatch?
fn get_steno_indicator(raw: bool) -> &'static Indication {
    if raw {
        &leds::manager::STENO_RAW_INDICATOR
    } else {
        &leds::manager::STENO_INDICATOR
    }
}

// TODO: Does this move to Dispatch?
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
    // The dispatcher, for sending events to.
    dispatch: Arc<Dispatch>,
) {
    const PERIOD_MS: usize = 10;
    zephyr::event_loop!(keys, Duration::millis_at_least(PERIOD_MS as Tick),
                        Some(ev) => {
                            layout.handle_event(ev, dispatch.as_ref()).await;
                        },
                        None => {
                            layout.tick(dispatch.as_ref(), PERIOD_MS).await;
                        },
    );
}

/// Conditionally return the inter-board code.
#[cfg(dt = "chosen::inter_board_uart")]
fn get_inter(
    side: Side,
    equeue_send: Sender<Event>,
) -> Option<(InterHandler, Sender<InterUpdate>)> {
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
    translate: fn(u8) -> u8,
}

impl Scanner {
    fn new(matrix: Matrix, events: Sender<Event>, info: &BoardInfo) -> Scanner {
        let translate = translate::get_translation(&info.name);
        Scanner {
            matrix,
            events,
            translate,
        }
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
            static mut z_malloc_heap: sys_heap;
        }

        let mut stats: sys_memory_stats = mem::zeroed();
        match sys_heap_runtime_stats_get(&mut z_malloc_heap, &mut stats) {
            0 => (),
            n => {
                warn!("Unable to collect heap stats: {}", n);
                return;
            }
        }

        info!("Heap free: {}", stats.free_bytes);
        info!("    alloc: {}", stats.allocated_bytes);
        info!("max alloc: {}", stats.max_allocated_bytes);

    }
}

kobj_define! {
    // The steno thread.
    static STENO_THREAD: StaticThread;
    static STENO_STACK: ThreadStack<4096>;

    // A thread for the inter-worker.  Allows this to run at lower priority to prevent stalls.
    static INTER_STACK: ThreadStack<2048>;
}
*/
