#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use bbq_keyboard::{KeyAction, KeyEvent, LayoutMode, MinorMode, layout::{LayoutActions, LayoutManager}};
use bbq_steno::Stroke;
use embassy_executor::Spawner;
use embassy_time::Ticker;
use static_cell::StaticCell;
use zephyr::{device::gpio::GpioPin, devicetree::Value, embassy::Executor, printkln};

mod mapping;
mod matrix;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    printkln!("Jolt keyboard firmware");
    printkln!("Time tick: {}", zephyr::time::SYS_FREQUENCY);

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
        printkln!("Send key: {:?}", key);
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
