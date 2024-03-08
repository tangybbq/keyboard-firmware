#![no_std]

use core::slice;

// use alloc::{vec::Vec, string::ToString, collections::VecDeque};
use alloc::collections::VecDeque;
use bbq_keyboard::{layout::LayoutManager, EventQueue, Event, KeyEvent, KeyAction};
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
    let mut events = VecDeque::new();

    // Keys queued up to send to HID.
    let mut keys = VecDeque::new();

    heartbeat.start(1);
    loop {
        // Perform a single scan of the matrix.
        matrix.scan(|code, press| {
            // info!("Key {} {:?}", code, press);
            if press {
                events.push_back(Event::Matrix(KeyEvent::Press(code)));
                // events.push(KeyEvent::Press(code));
            } else {
                events.push_back(Event::Matrix(KeyEvent::Release(code)));
                // events.push(KeyEvent::Release(code));
            }
            Ok(())
        }).unwrap();

        // Push off any usb-hid keypresses.
        usb_hid_push(&mut keys);

        // Dispatch any events.
        while let Some(event) = events.pop_front() {
            match event {
                Event::Matrix(key) => {
                    // In the simple single-side case, matrix events are just
                    // passed to the layout manager.
                    layout.handle_event(key, &mut VecDeqEvents(&mut events));
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

        layout.tick(&mut VecDeqEvents(&mut events));

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

// An even queue built around a VecDeque
struct VecDeqEvents<'a>(&'a mut VecDeque<Event>);

impl<'a> EventQueue for VecDeqEvents<'a> {
    fn push(&mut self, val: Event) {
        self.0.push_back(val);

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
