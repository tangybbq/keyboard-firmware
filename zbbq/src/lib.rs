#![no_std]

use crate::matrix::Matrix;

extern crate alloc;

mod devices;
mod matrix;
mod zephyr;

#[no_mangle]
extern "C" fn rust_main () {
    info!("Zephyr keyboard code");
    let pins = devices::PinMatrix::get();
    let mut matrix = Matrix::new(pins).unwrap();

    matrix.scan(|code, press| {
        info!("Key {} {:?}", code, press);
        Ok(())
    }).unwrap();
}

pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    GPIO,
}
