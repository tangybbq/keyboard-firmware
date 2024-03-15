#![no_std]

use core::mem::MaybeUninit;
use core::slice;

use alloc::vec::Vec;
use alloc::collections::VecDeque;
use bbq_keyboard::usb_typer::{enqueue_action, ActionHandler};
use bbq_keyboard::{Keyboard, Mods, LayoutMode, UsbDeviceState, Timable};
use bbq_keyboard::{layout::LayoutManager, EventQueue, Event, KeyEvent, KeyAction};
use bbq_keyboard::dict::Dict;
use bbq_steno::Stroke;
use zephyr::channel::Channel;
use zephyr::struct_timer;
use zephyr::sync::{k_mutex, k_condvar};

use crate::devices::acm::Uart;
use crate::devices::leds::LedStrip;
use crate::leds::LedManager;
use crate::{matrix::Matrix, zephyr::Timer, devices::GpioFlags};

extern crate alloc;

mod devices;
mod leds;
mod matrix;
mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    info!("Zephyr keyboard code");
    let pins = devices::PinMatrix::get();
    let reverse = devices::get_matrix_reverse();
    info!("Reverse scan?: {}", reverse);
    let mut matrix = Matrix::new(pins, reverse).unwrap();
    let mut leds = LedManager::new(LedStrip::get());

    let mut acm = Uart::get_gemini();

    let translate = devices::get_matrix_translate();
    info!("Matrix translation: {:?}", translate);
    let translate = get_translation(translate);

    if let Some(side_select) = devices::get_side_select() {
        side_select.pin_configure(GpioFlags::GPIO_INPUT).unwrap();
        info!("Side: {:?}", side_select.pin_get().unwrap());
    }

    let mut heartbeat = unsafe {
        Timer::new_from_c(&mut heartbeat_timer)
    };

    let mut layout = LayoutManager::new();

    // Keys queued up to send to HID.
    let mut keys = VecDeque::new();

    heartbeat.start(1);

    // Start with a global indicator, showing unconfigured USB.
    let mut has_global = true;
    let mut suspended = true;
    let mut woken = false;
    let mut current_mode = LayoutMode::Steno;
    loop {
        // Update the state of the Gemini indicator.
        leds.set_base(2, if acm.is_dtr() {
            &leds::GEMINI_INDICATOR
        } else {
            &leds::OFF_INDICATOR
        });

        // Perform a single scan of the matrix.
        matrix.scan(|code, press| {
            let code = translate(code);
            // info!("Key {} {:?}", code, press);
            if press {
                let _ = event_queue().try_send(Event::Matrix(KeyEvent::Press(code)));
            } else {
                let _ = event_queue().try_send(Event::Matrix(KeyEvent::Release(code)));
            }
            Ok(())
        }).unwrap();

        // Push off any usb-hid keypresses.
        usb_hid_push(&mut keys);

        // Dispatch any events.
        while let Ok(event) = event_queue().try_recv() {
            match event {
                Event::Matrix(key) => {
                    // If we get events, but are suspended, request a wakeup.
                    if suspended && !woken {
                        devices::usb_wakup();
                        woken = true;
                        // Ideally, we would discard keys until we wake up.
                        // But, that isn't really ideal.  Unsure if we need to
                        // be careful to only call this once per suspend.
                    }
                    // In the simple single-side case, matrix events are just
                    // passed to the layout manager.
                    layout.handle_event(key, &mut MutEventQueue);
                }
                Event::Key(key) => {
                    // Keypress are queued up, to be sent to the hid layer.
                    keys.push_back(key);
                }

                // For now, just show what steno strokes are.
                Event::RawSteno(stroke) => {
                    if current_mode == LayoutMode::Steno {
                        // Send the stroke off to the steno thread for processing.
                        let _ = steno_queue().try_send(stroke);
                    } else {
                        // In the raw steno mode, send via gemini.
                        let packet = stroke.to_gemini();
                        acm.write(&packet);
                    }
                }

                // Once the steno layer has translated the strokes, it gives us
                // a TypeAction to send of to HID.
                Event::StenoText(action) => {
                    // For each remove, press a backspace.
                    for _ in 0..action.remove {
                        keys.push_back(KeyAction::KeyPress(Keyboard::DeleteBackspace, Mods::empty()));
                        keys.push_back(KeyAction::KeyRelease);
                    }
                    // Then, just send the text.
                    enqueue_action(&mut KeyActionWrap(&mut keys), &action.text);
                }

                // Mode select and mode affect the LEDs.
                Event::ModeSelect(mode) => {
                    info!("modeselect: {:?}", mode);
                    let next = match mode {
                        LayoutMode::Steno => &leds::STENO_SELECT_INDICATOR,
                        LayoutMode::StenoRaw => &leds::STENO_RAW_SELECT_INDICATOR,
                        LayoutMode::Taipo => &leds::TAIPO_SELECT_INDICATOR,
                        LayoutMode::Qwerty => &leds::QWERTY_SELECT_INDICATOR,
                        _ => &leds::QWERTY_SELECT_INDICATOR,
                    };
                    leds.set_base(0, next);
                }

                // Mode select and mode affect the LEDs.
                Event::Mode(mode) => {
                    info!("modeselect: {:?}", mode);
                    let next = match mode {
                        LayoutMode::Steno => &leds::STENO_INDICATOR,
                        LayoutMode::StenoRaw => &leds::STENO_RAW_INDICATOR,
                        LayoutMode::Taipo => &leds::TAIPO_INDICATOR,
                        LayoutMode::Qwerty => &leds::QWERTY_INDICATOR,
                        _ => &leds::QWERTY_INDICATOR,
                    };
                    leds.set_base(0, next);
                    current_mode = mode;
                }

                // When the USB is configured, turn off the global indicator.
                Event::UsbState(UsbDeviceState::Configured) | Event::UsbState(UsbDeviceState::Resume) => {
                    if has_global {
                        leds.clear_global(0);
                        has_global = false;
                        suspended = false;
                    }
                }

                Event::UsbState(UsbDeviceState::Suspend) => {
                    leds.set_global(0, &leds::SLEEP_INDICATOR);
                    has_global = true;
                    suspended = true;
                    woken = false;
                }

                // The USB state isn't meaningful here.
                Event::UsbState(_) => {
                    /*
                    if has_global {
                        // leds.clear_global();
                        has_global = false;
                    }
                    */
                }

                // Catch all for the rest.
                event => info!("event: {:?}", event),
            }
        }

        // Pass the keys off to the layout manager.
        // for event in events {
        //     layout.handle_event(event, &mut silly);
        // }

        layout.tick(&mut MutEventQueue);
        leds.tick();

        heartbeat.wait();
    }
}

/// The lower priority steno lookup thread.
#[no_mangle]
extern "C" fn steno_thread_main() -> ! {
    info!("Steno thread running");
    let mut dict = Dict::new();
    loop {
        let stroke = steno_queue().recv().unwrap();
        // info!("Stroke: {}", stroke);
        for action in dict.handle_stroke(stroke, &WrapTimer) {
            // Enqueue the action, and the actual typing will be queued up by
            // the main thread.  In this case, it is ok to block.
            // TODO: implement the blocking send.
            let _ = event_queue().try_send(Event::StenoText(action));
            // info!("Action: {:?}", action);
        }
    }
}

/// Push a usb-hid event off to the USB stack, if that makes sense.
fn usb_hid_push(keys: &mut VecDeque<KeyAction>) {
    // If a report is pending, do nothing.
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
                // TODO: We don't handle more than 6 keys, which qwerty mode can
                // do.  For now just report if we can.
                let (mods, keys) = keyset_to_hid(keys);
                devices::hid_send_keyboard_report(mods.bits(), &keys);
                info!("TODO: KeySet: {:?}", keys);
            }
            KeyAction::ModOnly(mods) => {
                devices::hid_send_keyboard_report(mods.bits(), &[]);
            }
            KeyAction::Stall => {
                // Not sure what this means with this interface.  For now, just
                // go on a 1 ms tick.
            }
        }
    }
}

// Qwerty mode just sends scan codes, but not the mod bits as expected by the
// HID Layer.  To fix this, convert the codes from QWERTY into a proper
// formatted data for a report.
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

// Matrix translation simplifies some other parts of the code.
fn translate_id(code: u8) -> u8 {
    code
}

static HIGHBOARD: [u8; 24] = [
    // 0
    255,
    255,
    255,
    15,
    19,
    // 5
    23,
    2,
    6,
    10,
    14,
    // 10
    18,
    22,
    1,
    5,
    9,
    // 15
    13,
    17,
    21,
    0,
    4,
    // 20
    8,
    12,
    16,
    20,
];

fn translate_highboard(code: u8) -> u8 {
    *HIGHBOARD.get(code as usize).unwrap_or(&255)
}

static PROTO4: [u8; 30] = [
    // 0
    13,    // L-F1
    14,    // L-F2
    11,    // L-Star
    11+15, // R-T
    14+15, // R-Z
    // 5
    13+15, // R-D
    12,    // L-S
    9,     // L-T
    10,    // L-K
    10+15, // R-G
    // 10
    9+15,  // R-L
    12+15, // R-S
    8,     // L-P
    7,     // L-W
    6,     // L-H
    // 15
    6+15,  // R-F
    7+15,  // R-B
    8+15,  // R-P
    5,     // L-R
    3,     // L-S1
    // 20
    4,     // L-S2
    4+15,  // R-S4
    3+15,  // R-S3
    5+15,  // R-R
    2,     // L-num
    // 25
    1,     // L-A
    0,     // L-O
    0+15,  // R-E
    1+15,  // R-U
    2+15,  // R-Num
];

fn translate_proto4(code: u8) -> u8 {
    *PROTO4.get(code as usize).unwrap_or(&255)
}

fn get_translation(translate: Option<&'static str>) -> fn (u8) -> u8 {
    match translate {
        Some("proto4") => translate_proto4,
        Some("highboard") => translate_highboard,
        None => translate_id,
        Some(name) => {
            panic!("Unexpected translation in DT: {}", name);
        }
    }
}

struct KeyActionWrap<'a>(&'a mut VecDeque<KeyAction>);

impl<'a> ActionHandler for KeyActionWrap<'a> {
    fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for act in events {
            self.0.push_back(act);
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    GPIO,
    LED,
}

extern "C" {
    static mut heartbeat_timer: struct_timer;
}

type SafeEventQueue = Channel<Event, EVENT_QUEUE_SIZE>;

/// The global shared event queue.  This is a mutable static to be initialized (unsafely).
static mut EVENT_QUEUE: MaybeUninit<SafeEventQueue> = MaybeUninit::uninit();

/// Wrapper to get the current event_queue.
pub fn event_queue() -> &'static SafeEventQueue {
    // We assume the init happens early.
    unsafe {
        &*EVENT_QUEUE.as_ptr()
    }
}

/// The number of elements that can be queued in the event queue. As long as
/// there is a generally small correspondence between the sizes of different
/// events, this shouldn't need to be too large. It is conceivable that all keys
/// are pressed at the same time, which would enqueue an event for every key
/// press. As such, we will make this a bit larger than the largest number of
/// keys we support. Longer strings to be typed will be a small number of
/// messages, and those will expand directly into the HID queue, and not to
/// events.
const EVENT_QUEUE_SIZE: usize = 64;

type StenoQueue = Channel<Stroke, STENO_QUEUE_SIZE>;
const STENO_QUEUE_SIZE: usize = 16;

static mut STENO_QUEUE: MaybeUninit<StenoQueue> = MaybeUninit::uninit();

pub fn steno_queue() -> &'static StenoQueue {
    unsafe {
        &*STENO_QUEUE.as_ptr()
    }
}

extern "C" {
    static mut event_queue_mutex: k_mutex;
    static mut event_queue_condvar: k_condvar;
    static mut steno_queue_mutex: k_mutex;
    static mut steno_queue_condvar: k_condvar;
}

// The keyboard code is expecting the event queue to be mutable.  To make this
// work, we just use this placeholder, which can readily be created, to pass
// around an empty instance.
struct MutEventQueue;

impl EventQueue for MutEventQueue {
    fn push(&mut self, val: Event) {
        let _ = event_queue().try_send(val);
        // match val {
        //     Event::RawSteno(stroke) => {
        //         // let text = stroke.to_string();
        //         // info!("stroke: {}", text);
        //         info!("stroke: {}", stroke.to_string());
        //     }
        //     ev => info!("event: {:?}", ev),
        // }
    }
}

#[no_mangle]
extern "C" fn init_queues() {
    // Initialize the static event queue.
    unsafe {
        EVENT_QUEUE.write(Channel::new(&mut event_queue_mutex, &mut event_queue_condvar));
        STENO_QUEUE.write(Channel::new(&mut steno_queue_mutex, &mut steno_queue_condvar));
    }
}

// Hack to get time from the os.
struct WrapTimer;

impl Timable for WrapTimer {
    fn get_ticks(&self) -> u64 {
        unsafe { sys_cycle_get_64() }
    }
}

extern "C" {
    fn sys_cycle_get_64() -> u64;
}
