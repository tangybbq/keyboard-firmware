// Copyright (c) 2024 Linaro LTD
// SPDX-License-Identifier: Apache-2.0

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

use core::cell::RefCell;
use core::slice;

use matrix::Matrix;
use zephyr::{kobj_define, printkln};
use zephyr::object::KobjInit;
use zephyr::sync::channel::{
    self,
    Sender,
    Message,
};

use bbq_keyboard::{
    Event,
    EventQueue,
    Keyboard,
    KeyAction,
    KeyEvent,
    layout::LayoutManager,
    Mods,
    UsbDeviceState,
};
use bbq_keyboard::serialize::{Decoder, Packet, PacketBuffer};

mod devices;
mod matrix;

#[no_mangle]
extern "C" fn rust_main() {
    printkln!("Hello world from Rust on {}",
              zephyr::kconfig::CONFIG_BOARD);

    // Initialize the main event queue.
    EVENT_QUEUE_STATIC.init();
    let equeue = EVENT_QUEUE_STATIC.get();
    let (equeue_send, equeue_recv) = channel::unbounded_from::<Event>(equeue);

    unsafe {
        // Store a sender for the USB callback.
        USB_CB_MAIN_SEND = Some(equeue_send.clone());
        // Store a sender for the Heartbeat callback.
        HEARTBEAT_MAIN_SEND = Some(equeue_send.clone());
    }

    add_heartbeat_box();

    // After the callbacks have the queue handles, we can start the heartbeat.
    setup_heartbeat();

    // Retrieve the side select.
    let side = devices::get_side();
    printkln!("Our side: {:?}", side);

    // Initialize USB HID.
    usb_setup();

    // Is this the best way to do this?  These aren't that big.
    let rows = zephyr::devicetree::aliases::matrix::get_rows();
    let cols = zephyr::devicetree::aliases::matrix::get_cols();

    // Build a Vec for these.
    let rows: Vec<_> = rows.into_iter().collect();
    let cols: Vec<_> = cols.into_iter().collect();

    let matrix = Matrix::new(rows, cols);
    let mut scanner = Scanner::new(matrix, equeue_send.clone());

    let mut layout = LayoutManager::new();

    let mut uart = zephyr::devicetree::chosen::inter_board_uart::get_instance();
    let mut buffer = [0u8; 32];
    let mut seq = 0;

    let mut out_buffer = PacketBuffer::new();

    let mut decode = Decoder::new();

    let mut eq_send = SendWrap(equeue_send.clone());
    let mut keys = VecDeque::new();

    loop {
        let ev = equeue_recv.recv().unwrap();

        let mut is_tick = false;
        match ev {
            Event::Tick => is_tick = true,
            Event::Matrix(key) => {
                // TODO: State determines what we do with this.
                layout.handle_event(key, &mut eq_send);
            }
            Event::Key(key) => {
                // Keypresses are queued up, to be sent to the hid layer.
                keys.push_back(key);
            }
            ev => {
                printkln!("Event: {:?}", ev);
            }
        }

        // Only continue when the tick is received.
        if !is_tick {
            continue;
        }

        scanner.scan();

        // Transmit to the uart.
        let packet = Packet::Secondary {
            side: side,
            keys: Vec::new(),
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

        layout.tick(&mut eq_send);
        usb_hid_push(&mut keys);

        // After processing the main loop, generate a message for the tick irq handler.  This will
        // allow ticks to be missed if processing takes too long.
        add_heartbeat_box();
    }
}

/// Scanner.
///
/// The scanner is responsible for receiving scan events from the keyboard, as well as from the
/// other side.
struct Scanner {
    matrix: Matrix,
    events: Sender<Event>,
    translate: fn (u8) -> u8,
}

impl Scanner {
    fn new(matrix: Matrix, events: Sender<Event>) -> Scanner {
        let translate = devices::get_translation();
        Scanner { matrix, events, translate }
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
}

/// Push usb-hid events to the USB stack, when possible.
fn usb_hid_push(keys: &mut VecDeque<KeyAction>) {
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
                // TODO We don't handle more than 6 keys, which qwerty mode can do.  For now, just
                // report if we can.
                let (mods, keys) = keyset_to_hid(keys);
                devices::hid_send_keyboard_report(mods.bits(), &keys);
            }
            KeyAction::ModOnly(mods) => {
                devices::hid_send_keyboard_report(mods.bits(), &[]);
            }
            KeyAction::Stall => (),
        }
    }
}

// Qwerty mode just sends scan codes, but not the mod bits as expected by the HID layer.  To fix
// this, convert the codes from QWERTY into a proper formatted data for a report.
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
#[no_mangle]
extern "C" fn rust_usb_status(state: u32) {
    let send = unsafe { USB_CB_MAIN_SEND.as_mut().unwrap() };

    let state = match state {
        0 => UsbDeviceState::Configured,
        1 => UsbDeviceState::Suspend,
        2 => UsbDeviceState::Resume,
        _ => unreachable!(),
    };
    send.send(Event::UsbState(state)).unwrap();
}

static mut HEARTBEAT_MAIN_SEND: Option<Sender<Event>> = None;
static HEARTBEAT_BOX: critical_section::Mutex<RefCell<Option<Box<Message<Event>>>>> =
    critical_section::Mutex::new(RefCell::new(None));

#[no_mangle]
extern "C" fn rust_heartbeat() {
    let send = unsafe { HEARTBEAT_MAIN_SEND.as_mut().unwrap() };
    let boxed = critical_section::with(|cs| {
        HEARTBEAT_BOX.borrow_ref_mut(cs).take()
    });
    // Send it, if there was one there to send.
    if let Some(boxed) = boxed {
        send.send_boxed(boxed).unwrap();
    }
}

/// Give the heartbeat IRQ a box holding a message it can send.
fn add_heartbeat_box() {
    let tick = Box::new(Message::new(Event::Tick));
    critical_section::with(|cs| {
        HEARTBEAT_BOX.borrow(cs).replace(Some(tick));
    });
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

/// Initialize the heartbeat.
fn setup_heartbeat() {
    unsafe {
        extern "C" {
            fn setup_heartbeat();
        }

        setup_heartbeat();
    }
}

kobj_define! {
    // The main event queue.
    static EVENT_QUEUE_STATIC: StaticQueue;
}
