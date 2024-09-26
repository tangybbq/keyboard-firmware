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

use bbq_keyboard::{KeyEvent, Side};
use bbq_keyboard::serialize::{Decoder, Packet, PacketBuffer, EventVec};

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

    // Initialize USB HID.
    usb_setup();

    // Is this the best way to do this?  These aren't that big.
    let rows = zephyr::devicetree::aliases::matrix::get_rows();
    let cols = zephyr::devicetree::aliases::matrix::get_cols();

    // Build a Vec for these.
    let rows: Vec<_> = rows.into_iter().collect();
    let cols: Vec<_> = cols.into_iter().collect();

    let mut matrix = Matrix::new(rows, cols);
    // let mut matrix = Matrix::new(cols, rows);

    let mut uart = zephyr::devicetree::chosen::inter_board_uart::get_instance();
    let mut buffer = [0u8; 32];
    let mut seq = 0;

    let mut out_buffer = PacketBuffer::new();

    let delay = Duration::millis_at_least(1);
    let mut decode = Decoder::new();
    loop {
        let mut keys = EventVec::new();
        matrix.scan(|code, action| {
            let key = if action {
                KeyEvent::Press(code)
            } else {
                KeyEvent::Release(code)
            };
            // printkln!("{:?} {}", action, code);
            keys.push(key);
        });

        // Transmit to the uart.
        let packet = Packet::Secondary {
            side: side,
            keys: keys,
        };
        packet.encode(&mut out_buffer, &mut seq);

        let (a, b) = out_buffer.as_slices();
        let _ = uart.fifo_fill(a).unwrap();
        let _ = uart.fifo_fill(b).unwrap();
        out_buffer.clear();

        // buffer.iter_mut().for_each(|p| *p = 0xff);

        let num = uart.fifo_read(&mut buffer).unwrap();
        for ch in &buffer[0..num] {
            if let Some(packet) = decode.add_byte(*ch) {
                if let Packet::Secondary { keys, .. } = &packet {
                    if !keys.is_empty() {
                        printkln!("Packet: {:?}", packet);
                    }
                }
            }
        }

        sleep(delay);
    }
}

/// Rust USB callback.
#[no_mangle]
extern "C" fn rust_usb_status(state: u32) {
    printkln!("USB: {}", state);
}

/// Initialize the USB.
fn usb_setup() {
    unsafe {
        use core::ffi::c_int;

        extern "C" {
            fn usb_setup() -> c_int;
        }

        if usb_setup() != 0 {
            panic!("Unable to initialize USB");
        }
    }
}
