//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate alloc;

use core::convert::Infallible;
use core::iter::once;

// use alloc::collections::BTreeSet;
use bsp::{entry, XOSC_CRYSTAL_FREQ, hal::{uart::{UartConfig, StopBits, DataBits}, Timer}};
use defmt::*;
use defmt_rtt as _;
use embedded_hal::{digital::v2::{InputPin, OutputPin, PinState}, timer::CountDown};
use fugit::{ExtU32, RateExtU32};
use panic_probe as _;
use usb_device::class_prelude::UsbBusAllocator;
use ws2812_pio::Ws2812Direct;
use smart_leds::{SmartLedsWrite, RGB8};

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
    pio::PIOExt,
    watchdog::Watchdog,
};

use bsp::hal as hal;

use usbd_human_interface_device::page::Keyboard;

mod usb;

// use usbd_hid::descriptor::{generator_prelude::*, KeyboardReport};
// use usbd_hid::hid_class::HIDClass;

#[entry]
fn main() -> ! {
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 4096;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    info!("Program start");
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

    // The ACD1/GPIO27 gpio pin will be pulled up or down to indicate if this is
    // the left or right half of the keyboard. High indicates the right side,
    // and low is the left side.
    let side_select = pins.adc1.into_pull_down_input();
    let idle_color = if side_select.is_high().unwrap() {
        RGB8::new(0, 15, 0)
    } else {
        RGB8::new(0, 0, 15)
    };

    // let defmt_uart_pins = (
    //     pins.tx0.into_mode::<hal::gpio::FunctionUart>(),
    //     pins.rx0.into_mode::<hal::gpio::FunctionUart>(),
    // );
    // let defmt_uart = hal::uart::UartPeripheral::new(pac.UART0, defmt_uart_pins, &mut pac.RESETS)
    //     .enable(
    //         UartConfig::new(115200.Hz(), DataBits::Eight, None, StopBits::One),
    //         clocks.peripheral_clock.freq(),
    //     )
    //     .unwrap();
    // defmt_uart.write_full_blocking(b"Test hello\r\n");
    // defmt_serial::defmt_serial(defmt_uart);
    info!("Defmt working");

    let timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);
    let mut ticker = timer.count_down();
    ticker.start(1.millis());
    // ticker.start(250.micros());

    let (mut pio, sm0, _, _, _) = pac.PIO0.split(&mut pac.RESETS);
    let mut ws = Ws2812Direct::new(
        pins.led.into_function(),
        &mut pio,
        sm0,
        clocks.peripheral_clock.freq(),
        );

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    let mut usb_handler = usb::UsbHandler::new(&usb_bus);

    // let mut usb_hid = HIDClass::new(&usb_bus, KeyboardReport::desc(), 60);
    // let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0xfeed, 0xbee0))
    //     .manufacturer("David Brown")
    //     .product("Proto2 Keyboard")
    //     .serial_number("1234")
    //     .device_class(0)
    //     .build();

    /*
    delay.delay_ms(100);
    let report = KeyboardReport {
        modifier: 0,
        reserved: 0,
        leds: 0,
        keycodes: [0x04, 0x00, 0x00, 0x00, 0x00, 0x00],
    };
    usb_hid.push_input(&report).unwrap();
    usb_dev.poll(&mut [&mut usb_hid]);
    delay.delay_ms(100);
    usb_dev.poll(&mut [&mut usb_hid]);
    let report = KeyboardReport {
        modifier: 0,
        reserved: 0,
        leds: 0,
        keycodes: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    };
    usb_hid.push_input(&report).unwrap();
    usb_dev.poll(&mut [&mut usb_hid]);
    delay.delay_ms(100);
    usb_dev.poll(&mut [&mut usb_hid]);
    */

    // ws.write(once(RGB8::new(4, 16, 4))).unwrap();

    // Let's see if we can use the UART1.
    let uart_pins = (
        pins.tx1.into_function::<hal::gpio::FunctionUart>(),
        pins.rx1.into_function::<hal::gpio::FunctionUart>(),
    );
    let _uart = hal::uart::UartPeripheral::new(pac.UART1, uart_pins, &mut pac.RESETS)
        .enable(
            UartConfig::new(9600.Hz(), DataBits::Eight, None, StopBits::One),
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

    // let mut gotten = false;
    // while !gotten {
    //     let mut buf = [0u8];
    //     if let Ok(count) = uart.read_raw(&mut buf) {
    //         info!("count = {}", count);
    //         info!("Read byte: {}", buf[0]);
    //         if buf[0] == b'\n' {
    //             gotten = true;
    //         }
    //     }
    //     delay.delay_ms(1);
    // }

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

    let mut reported: bool = false;
    // TODO: Use the fugit values, and actual intervals.
    let mut next_1ms = timer.get_counter().ticks() + 1_000;
    let mut next_10us = timer.get_counter().ticks() + 10;
    loop {
        let now = timer.get_counter().ticks();

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

        // For debugging, turn on the red LED in the case where we have keys down.
        let color = if keys.iter().any(|k| k.is_pressed()) {
            RGB8::new(15, 15, 0)
        } else {
            idle_color
        };
        ws.write(once(color)).unwrap();

        let this_reported = keys[0].is_pressed();
        if this_reported != reported {
            reported = this_reported;
            if reported {
                usb_handler.enqueue([
                    Event::KeyPress(Keyboard::H),
                    Event::KeyRelease(Keyboard::H),
                    Event::KeyPress(Keyboard::E),
                    Event::KeyRelease(Keyboard::E),
                    Event::KeyPress(Keyboard::L),
                    Event::KeyRelease(Keyboard::L),
                    Event::KeyPress(Keyboard::L),
                    Event::KeyRelease(Keyboard::L),
                    Event::KeyPress(Keyboard::O),
                    Event::KeyRelease(Keyboard::O),
                ].iter().cloned());
            }
        }

        // Rapid poll first.
        if now > next_10us {
            // Ideall this would be periodic, but it is also possible we never
            // keep up.
            usb_handler.poll();
            next_10us = now + 10;
        }

        // Slow poll next.
        if now > next_1ms {
            usb_handler.tick();
            next_1ms = now + 1_000;
        }
    }
}

/// Individual state tracking.
#[derive(Clone, Copy, Eq, PartialEq)]
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

    fn is_pressed(&self) -> bool {
        self.state == KeyState::Pressed || self.state == KeyState::DebounceRelease
    }
}

#[derive(Clone)]
pub(crate) enum Event {
    KeyPress(Keyboard),
    KeyRelease(Keyboard),
}
