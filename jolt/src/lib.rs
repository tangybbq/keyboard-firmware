#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use bbq_keyboard::{KeyAction, KeyEvent, Keyboard, LayoutMode, MinorMode, Mods, layout::{LayoutActions, LayoutManager}};
use bbq_steno::Stroke;
use embassy_executor::Spawner;
use embassy_time::Ticker;
use static_cell::StaticCell;
use zephyr::{device::gpio::GpioPin, devicetree::Value, embassy::Executor, printkln};

mod mapping;
mod matrix;

unsafe extern "C" {
    fn usb_setup() -> i32;
    fn usb_send_report(report: *const u8, len: u16) -> i32;
}

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    printkln!("Jolt keyboard firmware");
    printkln!("Time tick: {}", zephyr::time::SYS_FREQUENCY);

    let ret = unsafe { usb_setup() };
    if ret != 0 {
        panic!("usb_setup failed: {}", ret);
    }

    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(main(spawner)).unwrap();
    })
}

#[embassy_executor::task]
async fn main(spawner: Spawner) -> () {
    let mut cols = Vec::new();
    extract_gpios(zephyr::devicetree::aliases::matrix::RAW_COL_GPIOS, &mut cols);
    printkln!("n columns: {}", cols.len());

    let mut rows = Vec::new();
    extract_gpios(zephyr::devicetree::aliases::matrix::RAW_ROW_GPIOS, &mut rows);
    printkln!("n rows: {}", rows.len());

    // Find the keyboard matrix definitions.
    printkln!("Cols: {:?}", rows);
    printkln!("Rows: {:?}", cols);

    // Spawn a task to manage the keyboard matrix.
    spawner.spawn(keyboard_task(rows, cols)).unwrap();
}

/// A single executor to run most of the system. Runs in the main thread.
static EXECUTOR: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::task]
async fn keyboard_task(rows: Vec<GpioPin>, cols: Vec<GpioPin>) -> () {
    let mut count = 0u64;
    let mut matrix = matrix::Matrix::new(rows, cols);
    let mut ticker = Ticker::every(embassy_time::Duration::from_millis(1));
    let mut manager = LayoutManager::new(true);
    let action = Action;
    loop {
        let mut events = Vec::new();
        matrix.scan(|code, pressed| {
            let code = mapping::PROTO4_MAPPING.get(code as usize).copied().unwrap_or_else(|| {
                panic!("Invalid code from matrix: {}", code);
            });
            let ev = if pressed {
                KeyEvent::Press(code)
            } else {
                KeyEvent::Release(code)
            };
            events.push(ev);
            // printkln!("Key {} {}", code, if pressed { "pressed" } else { "released" });
        });
        for ev in events {
            printkln!("Event: {:?}", ev);
            manager.handle_event(ev, &action).await;
        }
        manager.tick(&action, 1).await;
        manager.poll();
        count += 1;
        if count % 30000 == 0 {
            printkln!("Keyboard task running: count={}", count);
        }
        ticker.next().await;
    }
}

struct Action;

impl LayoutActions for Action {
    async fn set_mode(&self, mode: LayoutMode) {
        printkln!("Set mode: {:?}", mode);
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        printkln!("Set mode select: {:?}", mode);
    }

    async fn send_key(&self, key: KeyAction) {
        match key {
            KeyAction::KeyPress(key, mods) => {
                submit_report(keypress_report(key, mods));
            }
            KeyAction::ModOnly(mods) => {
                submit_report([modifier_bits(mods), 0, 0, 0, 0, 0, 0, 0]);
            }
            KeyAction::KeyRelease => {
                submit_report([0; 8]);
            }
            KeyAction::KeySet(keys) => {
                submit_report(keyset_report(&keys));
            }
            KeyAction::Stall => {
                printkln!("USB stall action");
            }
        }
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        printkln!("Set submode: {:?}", submode);
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        printkln!("Clear submode: {:?}", submode);
    }

    async fn send_raw_steno(&self, steno: Stroke) {
        printkln!("Send raw steno: {}", steno);
    }
}

fn submit_report(report: [u8; 8]) {
    let ret = unsafe { usb_send_report(report.as_ptr(), report.len() as u16) };
    if ret != 0 {
        printkln!("usb_send_report failed: {}", ret);
    }
}

fn keypress_report(key: Keyboard, mods: Mods) -> [u8; 8] {
    [modifier_bits(mods), 0, key as u8, 0, 0, 0, 0, 0]
}

fn keyset_report(keys: &[Keyboard]) -> [u8; 8] {
    let mut report = [0u8; 8];
    for (index, key) in keys.iter().take(6).enumerate() {
        report[index + 2] = *key as u8;
    }
    report
}

fn modifier_bits(mods: Mods) -> u8 {
    let mut bits = 0u8;
    if mods.contains(Mods::CONTROL) {
        bits |= 0x01;
    }
    if mods.contains(Mods::SHIFT) {
        bits |= 0x02;
    }
    if mods.contains(Mods::ALT) {
        bits |= 0x04;
    }
    if mods.contains(Mods::GUI) {
        bits |= 0x08;
    }
    bits
}

/// Extract GPIO from the devicetree data.
///
/// As we don't have support yet for exporting the pins directly from the
/// devicetree, extrat them from the raw data.
fn extract_gpios(values: &[Value], out: &mut Vec<GpioPin>) {
    for value in values {
        if let Value::Words(words) = value {
            for elt in *words {
                if let zephyr::devicetree::Word::Gpio(name, args) = elt {
                    printkln!("GPIO: {} {:?}", name, args);
                    if *name != "gpio0" {
                        panic!("Unexpected GPIO controller name: {}", name);
                    }
                    out.push(unsafe {
                        GpioPin::raw_new(
                            zephyr::devicetree::labels::gpio0::get_instance_raw(),
                            zephyr::devicetree::labels::gpio0::get_static_raw(),
                            args[0],
                            args[1],
                        )
                    });
                }
            }
        }
    }
}
