//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate alloc;

use arraydeque::ArrayDeque;
use arrayvec::ArrayString;
use matrix::KeyEvent;
use steno::Stroke;
use usb::UsbHandler;
use ws2812_pio::Ws2812Direct;

use core::convert::Infallible;
use core::iter::once;

// use alloc::collections::BTreeSet;
use bsp::{entry, XOSC_CRYSTAL_FREQ, hal::{uart::{UartConfig, StopBits, DataBits}, Timer}};
use defmt::*;
use defmt_rtt as _;
use embedded_hal::{digital::v2::{InputPin, OutputPin, PinState}, timer::CountDown};
use fugit::{ExtU32, RateExtU32};
use panic_probe as _;
use usb_device::{class_prelude::{UsbBusAllocator, UsbBus}, prelude::UsbDeviceState};

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

mod matrix;
mod usb;
mod steno;
mod inter;
mod leds;

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
    // let side_select = pins.adc1.into_pull_down_input();
    // let idle_color = if side_select.is_high().unwrap() {
    //     RGB8::new(0, 15, 0)
    // } else {
    //     RGB8::new(0, 0, 15)
    // };

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
    let mut led_manager = leds::LedManager::new(&mut ws);

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    let mut usb_handler = usb::UsbHandler::new(&usb_bus);

    // ws.write(once(RGB8::new(4, 16, 4))).unwrap();

    // Let's see if we can use the UART1.
    let uart_pins = (
        pins.tx1.into_function::<hal::gpio::FunctionUart>(),
        pins.rx1.into_function::<hal::gpio::FunctionUart>(),
    );
    // info!("Uart clk: {}", clocks.peripheral_clock.freq().raw());
    let uart = hal::uart::UartPeripheral::new(pac.UART1, uart_pins, &mut pac.RESETS)
        .enable(
            // Ideally, being above 320k will allow full frames to be sent each
            // tick.  This number is chosen to be an exact divisor of the clock rate.
            UartConfig::new(390625.Hz(), DataBits::Eight, None, StopBits::One),
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

    let mut inter_handler = inter::InterHandler::new(uart);

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
    let cols = &mut [
        &mut col_a as &mut dyn OutputPin<Error = Infallible>,
        &mut col_b as &mut dyn OutputPin<Error = Infallible>,
        &mut col_c as &mut dyn OutputPin<Error = Infallible>,
        &mut col_d as &mut dyn OutputPin<Error = Infallible>,
        &mut col_e as &mut dyn OutputPin<Error = Infallible>,
    ];

    let row_1 = pins.gpio7.into_pull_down_input();
    let row_2 = pins.adc0.into_pull_down_input();
    let row_3 = pins.sck.into_pull_down_input();
    let rows = &[
        &row_1 as &dyn InputPin<Error = Infallible>,
        &row_2 as &dyn InputPin<Error = Infallible>,
        &row_3 as &dyn InputPin<Error = Infallible>,
    ];

    let mut matrix_handler: matrix::Matrix<'_, '_, Infallible, 15> = matrix::Matrix::new(
        cols,
        rows,
    );

    let mut steno_raw_handler = steno::RawStenoHandler::new();

    // TODO: Use the fugit values, and actual intervals.
    let mut next_1ms = timer.get_counter().ticks() + 1_000;
    let mut next_10us = timer.get_counter().ticks() + 10;

    let mut events = EventQueue::new();
    loop {
        let now = timer.get_counter().ticks();

        // Rapid poll first.
        if now > next_10us {
            // Ideall this would be periodic, but it is also possible we never
            // keep up.
            usb_handler.poll(&mut events);
            matrix_handler.poll();
            steno_raw_handler.poll();
            inter_handler.poll();
            next_10us = now + 10;
        }

        // Slow poll next.
        if now > next_1ms {
            usb_handler.tick();
            matrix_handler.tick(&mut delay, &mut events);
            steno_raw_handler.tick();

            // Handle the event queue.
            while let Some(event) = events.pop() {
                match event {
                    Event::Matrix(key) => {
                        steno_raw_handler.handle_event(key, &mut events);
                    }
                    Event::RawSteno(stroke) => {
                        let mut buffer = ArrayString::<24>::new();
                        stroke.to_arraystring(&mut buffer);

                        // Enqueue with USB to send.
                        enqueue_action(&mut usb_handler, buffer.as_str());
                        enqueue_action(&mut usb_handler, " ");
                        usb_handler.enqueue(once(KeyAction::KeyRelease));
                    }
                    Event::UsbState(UsbDeviceState::Configured) => {
                        // TODO: Unclear how to handle suspend, but once we are
                        // configured, we need to start figuring out which side
                        // we are so we can communicate between the halves.
                        led_manager.set_global(&leds::USB_PRIMARY);
                    }
                    Event::UsbState(_) => (),
                }
            }
            inter_handler.tick();
            led_manager.tick();

            next_1ms = now + 1_000;
        }
    }
}

/// An event is something that happens in a handler to indicate some action
/// likely needs to be performed on it.
pub(crate) enum Event {
    /// Events from the Matrix layer indicating changes in key actions.
    Matrix(KeyEvent),

    /// Indication of a "raw" steno stroke from the steno layer.  This is
    /// untranslated and should just be typed.
    RawSteno(Stroke),

    /// Change in USB status.
    UsbState(UsbDeviceState),
}

pub(crate) struct EventQueue(ArrayDeque<Event, 256>);

impl EventQueue {
    pub fn new() -> Self {
        EventQueue(ArrayDeque::new())
    }

    pub fn push(&mut self, event: Event) {
        if let Err(_) = self.0.push_back(event) {
            warn!("Internal event queue overflow");
        }
    }

    pub fn pop(&mut self) -> Option<Event> {
        self.0.pop_front()
    }
}

/// Indicates keypress that should be sent to the host.
#[derive(Clone)]
pub(crate) enum KeyAction {
    KeyPress(Keyboard),
    ShiftedKeyPress(Keyboard),
    KeyRelease,
}

fn enqueue_action<Bus: UsbBus>(usb: &mut UsbHandler<Bus>, text: &str) {
    for ch in text.chars() {
        let keys = match ch {
            'A' => KeyAction::ShiftedKeyPress(Keyboard::A),
            'B' => KeyAction::ShiftedKeyPress(Keyboard::B),
            'C' => KeyAction::ShiftedKeyPress(Keyboard::C),
            'D' => KeyAction::ShiftedKeyPress(Keyboard::D),
            'E' => KeyAction::ShiftedKeyPress(Keyboard::E),
            'F' => KeyAction::ShiftedKeyPress(Keyboard::F),
            'G' => KeyAction::ShiftedKeyPress(Keyboard::G),
            'H' => KeyAction::ShiftedKeyPress(Keyboard::H),
            'I' => KeyAction::ShiftedKeyPress(Keyboard::I),
            'J' => KeyAction::ShiftedKeyPress(Keyboard::J),
            'K' => KeyAction::ShiftedKeyPress(Keyboard::K),
            'L' => KeyAction::ShiftedKeyPress(Keyboard::L),
            'M' => KeyAction::ShiftedKeyPress(Keyboard::M),
            'N' => KeyAction::ShiftedKeyPress(Keyboard::N),
            'O' => KeyAction::ShiftedKeyPress(Keyboard::O),
            'P' => KeyAction::ShiftedKeyPress(Keyboard::P),
            'Q' => KeyAction::ShiftedKeyPress(Keyboard::Q),
            'R' => KeyAction::ShiftedKeyPress(Keyboard::R),
            'S' => KeyAction::ShiftedKeyPress(Keyboard::S),
            'T' => KeyAction::ShiftedKeyPress(Keyboard::T),
            'U' => KeyAction::ShiftedKeyPress(Keyboard::U),
            'V' => KeyAction::ShiftedKeyPress(Keyboard::V),
            'W' => KeyAction::ShiftedKeyPress(Keyboard::W),
            'X' => KeyAction::ShiftedKeyPress(Keyboard::X),
            'Y' => KeyAction::ShiftedKeyPress(Keyboard::Y),
            'Z' => KeyAction::ShiftedKeyPress(Keyboard::Z),
            '0' => KeyAction::KeyPress(Keyboard::Keyboard0),
            '1' => KeyAction::KeyPress(Keyboard::Keyboard1),
            '2' => KeyAction::KeyPress(Keyboard::Keyboard2),
            '3' => KeyAction::KeyPress(Keyboard::Keyboard3),
            '4' => KeyAction::KeyPress(Keyboard::Keyboard4),
            '5' => KeyAction::KeyPress(Keyboard::Keyboard5),
            '6' => KeyAction::KeyPress(Keyboard::Keyboard6),
            '7' => KeyAction::KeyPress(Keyboard::Keyboard7),
            '8' => KeyAction::KeyPress(Keyboard::Keyboard8),
            '9' => KeyAction::KeyPress(Keyboard::Keyboard9),
            '-' => KeyAction::KeyPress(Keyboard::Minus),
            ' ' => KeyAction::KeyPress(Keyboard::Space),
            '#' => KeyAction::ShiftedKeyPress(Keyboard::Keyboard3),
            '*' => KeyAction::ShiftedKeyPress(Keyboard::Keyboard8),
            '^' => KeyAction::ShiftedKeyPress(Keyboard::Keyboard6),
            '+' => KeyAction::ShiftedKeyPress(Keyboard::Minus),
            ch => {
                warn!("Unhandled character: {}", ch);
                KeyAction::ShiftedKeyPress(Keyboard::ForwardSlash)
            }
        };
        usb.enqueue([
            keys,
        ].iter().cloned());
    }
}
