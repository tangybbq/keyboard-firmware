// Copyright (c) 2024 Linaro LTD
// SPDX-License-Identifier: Apache-2.0

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use matrix::Matrix;
use zephyr::printkln;
use zephyr::time::{Duration, sleep};

mod matrix;

#[no_mangle]
extern "C" fn rust_main() {
    printkln!("Hello world from Rust on {}",
              zephyr::kconfig::CONFIG_BOARD);

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
