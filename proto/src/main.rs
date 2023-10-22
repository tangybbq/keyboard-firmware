//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

extern crate alloc;

use arrayvec::ArrayString;
use cortex_m::delay::Delay;
use smart_leds::{SmartLedsWrite, RGB8};
use usb::UsbHandler;
use ws2812_pio::Ws2812Direct;

use core::convert::Infallible;
use core::iter::once;

// use alloc::collections::BTreeSet;
use bsp::{entry, XOSC_CRYSTAL_FREQ, hal::{uart::{UartConfig, StopBits, DataBits, UartDevice, ValidUartPinout}, Timer}};
use defmt::*;
use defmt_rtt as _;
use embedded_hal::{digital::v2::{InputPin, OutputPin, PinState}, timer::CountDown};
use fugit::{ExtU32, RateExtU32};
use panic_probe as _;
use usb_device::{class_prelude::UsbBusAllocator, prelude::UsbDeviceState};

use embedded_alloc::Heap;

use bbq_keyboard::{KeyAction, Side, EventQueue, InterState, Event, LayoutMode, MinorMode};
use bbq_keyboard::usb_typer::enqueue_action;
use bbq_keyboard::layout::LayoutManager;

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

mod matrix;
mod usb;
mod inter;
mod leds;

// Bring in the bootloader.
#[link_section = ".boot_loader"]
#[used]
pub static BOOT_LOADER: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

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

    let delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

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
    let side = if side_select.is_high().unwrap() {
        info!("Right side");
        Side::Right
    } else {
        info!("Left side");
        Side::Left
    };
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
    let led_manager = leds::LedManager::new(&mut ws);

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    let usb_handler = usb::UsbHandler::new(&usb_bus);

    // Let's see if we can use the UART1.
    let uart_pins = (
        pins.tx1.into_function::<hal::gpio::FunctionUart>(),
        pins.rx1.into_function::<hal::gpio::FunctionUart>(),
    );
    let uart = hal::uart::UartPeripheral::new(pac.UART1, uart_pins, &mut pac.RESETS)
        .enable(
            // Ideally, being above 320k will allow full frames to be sent each
            // tick.  This number is chosen to be an exact divisor of the clock rate.
            UartConfig::new(390625.Hz(), DataBits::Eight, None, StopBits::One),
            clocks.peripheral_clock.freq(),
        )
        .unwrap();

    let inter_handler = inter::InterHandler::new(uart, side);

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

    let matrix_handler: matrix::Matrix<'_, '_, Infallible, 15> = matrix::Matrix::new(
        cols,
        rows,
        side,
    );

    let layout_manager = LayoutManager::new();
    main_loop(
        timer,
        delay,
        usb_handler,
        matrix_handler,
        layout_manager,
        inter_handler,
        led_manager,
    );
}

fn main_loop<
    'r, 'c, 'a,
    E: core::fmt::Debug,
    D: UartDevice,
    P: ValidUartPinout<D>,
    L: SmartLedsWrite<Color = RGB8>,
    const NKEYS: usize
>(
    timer: Timer,
    mut delay: Delay,
    mut usb_handler: UsbHandler<hal::usb::UsbBus>,
    mut matrix_handler: matrix::Matrix<'r, 'c, E, NKEYS>,
    mut layout_manager: LayoutManager,
    mut inter_handler: inter::InterHandler<D, P>,
    mut led_manager: leds::LedManager<'a, L>,
) -> ! {
    // TODO: Use the fugit values, and actual intervals.
    let mut next_1ms = timer.get_counter().ticks() + 1_000;
    let mut next_10us = timer.get_counter().ticks() + 10;

    let mut events = EventQueue::new();
    let mut state = InterState::Idle;
    let mut flashing = true;
    let mut usb_suspended = true;
    loop {
        let now = timer.get_counter().ticks();

        // Rapid poll first.
        if now > next_10us {
            // Ideall this would be periodic, but it is also possible we never
            // keep up.
            usb_handler.poll(&mut events);
            matrix_handler.poll();
            layout_manager.poll();
            inter_handler.poll(&mut events);
            next_10us = now + 10;
        }

        // Slow poll next.
        if now > next_1ms {
            usb_handler.tick();
            matrix_handler.tick(&mut delay, &mut events);
            layout_manager.tick(&mut events);

            // Handle the event queue.
            while let Some(event) = events.pop() {
                match event {
                    Event::Matrix(key) => {
                        match state {
                            InterState::Primary | InterState::Idle =>
                                layout_manager.handle_event(key, &mut events),
                            InterState::Secondary =>
                                inter_handler.add_key(key),
                        }
                        if usb_suspended {
                            // This is specific to our implementation.
                            // TODO: Only do this if remote wakeup enabled.
                            usb_handler.wakeup();
                        }
                    }
                    Event::InterKey(key) => {
                        if state == InterState::Primary {
                            layout_manager.handle_event(key, &mut events)
                        }
                    }
                    Event::Key(action) => {
                        usb_handler.enqueue(once(action));
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
                        flashing = true;

                        // Indicate to the inter channel that we are now primary.
                        inter_handler.set_state(InterState::Primary, &mut events);

                        usb_suspended = false;
                    }
                    Event::UsbState(UsbDeviceState::Suspend) => {
                        // This indicates the host has gone to sleep.
                        led_manager.set_global(&leds::SLEEP_INDICATOR);
                        // flashing = true;
                        usb_suspended = true;
                    }
                    Event::UsbState(_) => (),
                    Event::BecomeState(new_state) => {
                        if state != new_state {
                            if new_state == InterState::Secondary  {
                                info!("Secondary");
                                // We've gone into secondary state, stop blinking the LEDs.
                                led_manager.clear_global();
                            } else if new_state == InterState::Idle {
                                info!("Idle");
                                led_manager.clear_global();
                            } else {
                                info!("Primary");
                            }
                            state = new_state;
                        } else if new_state == InterState::Secondary && flashing {
                            // This happens if the secondary side is running,
                            // and the primary side is reset (common when
                            // programming firmware, or waking from sleep).
                            // Detect that we are flashing the lights, and turn
                            // them off.
                            led_manager.clear_global();
                            flashing = false;
                        }
                    }
                    Event::Mode(mode) => {
                        let visible = match mode {
                            LayoutMode::Steno => &leds::STENO_INDICATOR,
                            LayoutMode::Artsey => &leds::ARTSEY_INDICATOR,
                        };
                        led_manager.set_base(visible);
                    }
                    Event::Indicator(mode) => {
                        let visible = match mode {
                            MinorMode::ArtseyMain => &leds::ARTSEY_INDICATOR,
                            MinorMode::ArtseyNav => &leds::ARTSEY_NAV_INDICATOR,
                        };
                        led_manager.set_base(visible);
                    }
                    Event::Heartbeat => {
                        if flashing {
                            led_manager.clear_global();
                            flashing = false;
                        }
                    }
                }
            }
            inter_handler.tick();
            led_manager.tick();

            next_1ms = now + 1_000;
        }
    }
}
