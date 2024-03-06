#![no_std]

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

    heartbeat.start(1);
    loop {
        matrix.scan(|code, press| {
            info!("Key {} {:?}", code, press);
            Ok(())
        }).unwrap();

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
