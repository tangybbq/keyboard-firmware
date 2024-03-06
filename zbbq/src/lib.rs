#![no_std]

use alloc::{vec::Vec, string::ToString};
use bbq_keyboard::{layout::LayoutManager, EventQueue, Event, KeyEvent};
use zephyr::struct_timer;

use crate::{matrix::Matrix, zephyr::Timer, devices::GpioFlags};

extern crate alloc;

mod devices;
mod matrix;
mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    info!("Zephyr keyboard code");
    let pins = devices::PinMatrix::get();
    let mut matrix = Matrix::new(pins).unwrap();

    let side_select = devices::get_side_select();
    side_select.pin_configure(GpioFlags::GPIO_INPUT).unwrap();
    info!("Side: {:?}", side_select.pin_get().unwrap());

    let mut heartbeat = unsafe {
        Timer::new_from_c(&mut heartbeat_timer)
    };

    let mut layout = LayoutManager::new();
    let mut silly = SillyQueue;

    heartbeat.start(1);
    loop {
        let mut events = Vec::new();

        matrix.scan(|code, press| {
            // info!("Key {} {:?}", code, press);
            if press {
                events.push(KeyEvent::Press(code));
            } else {
                events.push(KeyEvent::Release(code));
            }
            Ok(())
        }).unwrap();

        // Pass the keys off to the layout manager.
        for event in events {
            layout.handle_event(event, &mut silly);
        }

        layout.tick(&mut silly);

        heartbeat.wait();
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

// Silly event queue.
struct SillyQueue;

impl EventQueue for SillyQueue {
    fn push(&mut self, val: Event) {
        match val {
            Event::RawSteno(stroke) => {
                // let text = stroke.to_string();
                // info!("stroke: {}", text);
                info!("stroke: {}", stroke.to_string());
            }
            ev => info!("event: {:?}", ev),
        }
    }
}
