// Copyright (c) 2024 Linaro LTD
// SPDX-License-Identifier: Apache-2.0

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use matrix::Matrix;
use zephyr::printkln;
use zephyr::sys::busy_wait;
use zephyr::time::{Duration, sleep};
use zephyr::raw::{
    GPIO_INPUT,
    GPIO_PULL_UP,
};

use bbq_keyboard::Side;

mod matrix;

#[no_mangle]
extern "C" fn rust_main() {
    printkln!("Hello world from Rust on {}",
              zephyr::kconfig::CONFIG_BOARD);

    // Retrieve the side select.
    let side_select = zephyr::devicetree::side_select::get_gpios();
    let mut side_select = match side_select {
        [pin] => pin,
        // Compile error here means more than one pin is defined in DT.
    };

    side_select.configure(GPIO_INPUT | GPIO_PULL_UP);
    busy_wait(5);
    let side = if side_select.get() { Side::Left } else { Side::Right };

    printkln!("Our side: {:?}", side);

    // Is this the best way to do this?  These aren't that big.
    let rows = zephyr::devicetree::aliases::matrix::get_rows();
    let cols = zephyr::devicetree::aliases::matrix::get_cols();

    // Build a Vec for these.
    let rows: Vec<_> = rows.into_iter().collect();
    let cols: Vec<_> = cols.into_iter().collect();

    let mut matrix = Matrix::new(rows, cols);
    // let mut matrix = Matrix::new(cols, rows);

    let delay = Duration::millis_at_least(1);
    loop {
        matrix.scan(|code, action| {
            printkln!("{:?} {}", action, code);
        });

        sleep(delay);
    }
}
