//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate alloc;

use core::convert::Infallible;

// use alloc::collections::BTreeSet;
use bsp::{entry, XOSC_CRYSTAL_FREQ};
use defmt::*;
use defmt_rtt as _;
use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use panic_probe as _;

use embedded_alloc::Heap;

#[global_allocator]
static HEAP: Heap = Heap::empty();

// Provide an alias for our BSP so we can switch targets quickly.
// Uncomment the BSP you included in Cargo.toml, the rest of the code does not need to change.
// use rp_pico as bsp;
use sparkfun_pro_micro_rp2040 as bsp;

use bsp::hal::{
    clocks::{init_clocks_and_plls, Clock},
    pac,
    sio::Sio,
    watchdog::Watchdog,
};

#[entry]
fn main() -> ! {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 4096;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    info!("Program start");
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let clocks = init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut col_a = pins.gpio2.into_push_pull_output_in_state(PinState::Low);
    let mut col_b = pins.gpio3.into_push_pull_output_in_state(PinState::Low);
    let mut col_c = pins.gpio4.into_push_pull_output_in_state(PinState::Low);
    let mut col_d = pins.gpio5.into_push_pull_output_in_state(PinState::Low);
    let mut col_e = pins.gpio6.into_push_pull_output_in_state(PinState::Low);
    let cols = [
        &mut col_a as &mut dyn OutputPin<Error = Infallible>,
        &mut col_b as &mut dyn OutputPin<Error = Infallible>,
        &mut col_c as &mut dyn OutputPin<Error = Infallible>,
        &mut col_d as &mut dyn OutputPin<Error = Infallible>,
        &mut col_e as &mut dyn OutputPin<Error = Infallible>,
    ];
    let row_1 = pins.gpio7.into_pull_down_input();
    let row_2 = pins.adc0.into_pull_down_input();
    let row_3 = pins.sck.into_pull_down_input();
    let rows = [
        &row_1 as &dyn InputPin<Error = Infallible>,
        &row_2 as &dyn InputPin<Error = Infallible>,
        &row_3 as &dyn InputPin<Error = Infallible>,
    ];
    const KEYS: usize = 5 * 3;

    let mut keys = [Debouncer::new(); KEYS];

    // This is actually a terrible choice here, so move to something better.  But, this is fast.
    // let mut pressed = BTreeSet::new();
    // let mut released = BTreeSet::new();

    loop {
        for col in 0..cols.len() {
            cols[col].set_high().unwrap();
            for row in 0..rows.len() {
                let key = col * 3 + row;
                let action = keys[key].react(rows[row].is_high().unwrap());
                match action {
                    KeyAction::Press => info!("press: {}", key),
                    KeyAction::Release => info!("release: {}", key),
                    _ => (),
                }
            }
            cols[col].set_low().unwrap();
            delay.delay_us(5);
        }

        // Check if everything pressed got released.
        // if !released.is_empty() && pressed == released {
        //     for key in &released {
        //         info!("press: {}", key);
        //     }
        //     info!("Up");

        //     pressed.clear();
        //     released.clear();
        // }

        // This should be timer triggered so actually 1ms, not just after 1ms.
        delay.delay_ms(1);
    }
}

/// Individual state tracking.
#[derive(Clone, Copy)]
enum KeyState {
    /// Key is in released state.
    Released,
    /// Key is in pressed state.
    Pressed,
    /// We've seen a release edge, and will consider it released when consistent.
    DebounceRelease,
    /// We've seen a press edge, and will consider it pressed when consistent.
    DebouncePress,
}

#[derive(Clone, Copy)]
enum KeyAction {
    None,
    Press,
    Release,
}

// Don't really want Copy, but needed for init.
#[derive(Clone, Copy)]
struct Debouncer {
    /// State for this key.
    state: KeyState,
    /// Count how many times we've seen a given debounce state.
    counter: usize,
}

const DEBOUNCE_COUNT: usize = 20;

impl Debouncer {
    fn new() -> Debouncer {
        Debouncer {
            state: KeyState::Released,
            counter: 0,
        }
    }

    fn react(&mut self, pressed: bool) -> KeyAction {
        match self.state {
            KeyState::Released => {
                if pressed {
                    self.state = KeyState::DebouncePress;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::Pressed => {
                if !pressed {
                    self.state = KeyState::DebounceRelease;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::DebounceRelease => {
                if pressed {
                    // Reset the counter any time we see a press state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Released;
                        KeyAction::Release
                    } else {
                        KeyAction::None
                    }
                }
            }
            // TODO: We could probably just do two states, and a press/released flag.
            KeyState::DebouncePress => {
                if !pressed {
                    // Reset the counter any time we see a released state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Pressed;
                        KeyAction::Press
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }
}
