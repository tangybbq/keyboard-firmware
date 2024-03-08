#![no_std]

use core::{slice, cell::RefCell};

// use alloc::{vec::Vec, string::ToString, collections::VecDeque};
use alloc::collections::VecDeque;
use arraydeque::ArrayDeque;
use bbq_keyboard::{layout::LayoutManager, EventQueue, Event, KeyEvent, KeyAction};
use critical_section::Mutex;
use zephyr::struct_timer;

use crate::{matrix::Matrix, zephyr::Timer, devices::GpioFlags};

extern crate alloc;

mod devices;
mod matrix;
mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    critical_section::with(|_cs| {
        info!("Zephyr keyboard code");
    });
    let pins = devices::PinMatrix::get();
    let mut matrix = Matrix::new(pins).unwrap();

    let side_select = devices::get_side_select();
    side_select.pin_configure(GpioFlags::GPIO_INPUT).unwrap();
    info!("Side: {:?}", side_select.pin_get().unwrap());

    let mut heartbeat = unsafe {
        Timer::new_from_c(&mut heartbeat_timer)
    };

    let mut layout = LayoutManager::new();

    // Keys queued up to send to HID.
    let mut keys = VecDeque::new();

    heartbeat.start(1);
    loop {
        // Perform a single scan of the matrix.
        matrix.scan(|code, press| {
            // info!("Key {} {:?}", code, press);
            if press {
                EVENT_QUEUE.push(Event::Matrix(KeyEvent::Press(code)));
            } else {
                EVENT_QUEUE.push(Event::Matrix(KeyEvent::Release(code)));
                // events.push(KeyEvent::Release(code));
            }
            Ok(())
        }).unwrap();

        // Push off any usb-hid keypresses.
        usb_hid_push(&mut keys);

        // Dispatch any events.
        while let Some(event) = EVENT_QUEUE.pop() {
            match event {
                Event::Matrix(key) => {
                    // In the simple single-side case, matrix events are just
                    // passed to the layout manager.
                    layout.handle_event(key, &mut MutEventQueue);
                }
                Event::Key(key) => {
                    // Keypress are queued up, to be sent to the hid layer.
                    keys.push_back(key);
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

        heartbeat.wait();
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
            KeyAction::KeySet(_keys) => {
                info!("TODO: KeySet");
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

pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    GPIO,
}

extern "C" {
    static mut heartbeat_timer: struct_timer;
}

/// The global shared event queue.  Access is internally protected with a
/// critical section, so it is safe to enqueue things from callbacks, and the
/// likes.
pub static EVENT_QUEUE: SafeEventQueue = SafeEventQueue::new();

/// An event queue, built around an ArrayDeque that performs operations in a
/// critical section, so that it is possibly for interrupt handlers and
/// callbacks to register.
pub struct SafeEventQueue(Mutex<RefCell<ArrayDeque<Event, EVENT_QUEUE_SIZE>>>);

/// The number of elements that can be queued in the event queue. As long as
/// there is a generally small correspondence between the sizes of different
/// events, this shouldn't need to be too large. It is conceivable that all keys
/// are pressed at the same time, which would enqueue an event for every key
/// press. As such, we will make this a bit larger than the largest number of
/// keys we support. Longer strings to be typed will be a small number of
/// messages, and those will expand directly into the HID queue, and not to
/// events.
const EVENT_QUEUE_SIZE: usize = 64;

impl SafeEventQueue {
    pub const fn new() -> SafeEventQueue {
        SafeEventQueue(Mutex::new(RefCell::new(ArrayDeque::new())))
    }

    /// Push an event into the queue.  Even will be discarded if the queue
    /// overfills.
    pub fn push(&self, event: Event) {
        let mut failed = false;
        critical_section::with(|cs| {
            if self.0.borrow_ref_mut(cs).push_back(event).is_err() {
                failed = true;
            }
        });
        if failed {
            error!("Event queue overflow");
        }
    }

    /// Attempt to pop from the queue.
    pub fn pop(&self) -> Option<Event> {
        critical_section::with(|cs| {
            self.0.borrow_ref_mut(cs).pop_front()
        })
    }
}

// The keyboard code is expecting the event queue to be mutable.  To make this
// work, we just use this placeholder, which can readily be created, to pass
// around an empty instance.
struct MutEventQueue;

impl EventQueue for MutEventQueue {
    fn push(&mut self, val: Event) {
        EVENT_QUEUE.push(val);
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
