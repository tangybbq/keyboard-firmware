//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.

// This is needed by RTIC.
#![feature(type_alias_impl_trait)]
#![no_std]
#![no_main]

extern crate alloc;

use core::{
    convert::Infallible,
    mem::MaybeUninit,
    sync::atomic::{AtomicU8, Ordering},
};

use bsp::hal::gpio::{DynPinId, FunctionSio, Pin, PullDown, SioInput, SioOutput};
use defmt_rtt as _;
use embedded_alloc::Heap;
use matrix::Matrix;
use panic_probe as _;

use sparkfun_pro_micro_rp2040 as bsp;

mod board;
mod inter;
mod leds;
mod matrix;
mod usb;

#[global_allocator]
static HEAP: Heap = Heap::empty();
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
const HEAP_SIZE: usize = 8192;

type MatrixType = Matrix<
    Infallible,
    Pin<DynPinId, FunctionSio<SioInput>, PullDown>,
    Pin<DynPinId, FunctionSio<SioOutput>, PullDown>,
    { board::NCOLS },
    { board::NROWS },
    { board::NKEYS },
>;

#[rtic::app(
    device = crate::bsp::pac,
    dispatchers = [SW0_IRQ, SW1_IRQ],
    // dispatchers = [TIMER_IRQ_1],
)]
mod app {
    use crate::bsp;
    use crate::inter;
    use crate::leds;
    use crate::matrix::Matrix;
    use crate::usb;
    use crate::MatrixType;
    use crate::HEAP;
    use crate::HEAP_MEM;
    use crate::HEAP_SIZE;
    use arrayvec::ArrayString;
    use bbq_keyboard::layout::LayoutManager;
    use bbq_keyboard::usb_typer::enqueue_action;
    use bbq_keyboard::dict::Dict;
    use bbq_keyboard::Event;
    use bbq_keyboard::EventQueue;
    use bbq_keyboard::InterState;
    use bbq_keyboard::KeyAction;
    use bbq_keyboard::LayoutMode;
    use bbq_keyboard::MinorMode;
    use bbq_keyboard::Side;
    use bbq_keyboard::Timable;
    use bbq_steno::Stroke;
    use bsp::hal::clocks::init_clocks_and_plls;
    use bsp::hal::gpio::bank0::Gpio8;
    use bsp::hal::gpio::bank0::Gpio9;
    use bsp::hal::gpio::DynPinId;
    use bsp::hal::gpio::FunctionPio0;
    use bsp::hal::gpio::FunctionUart;
    use bsp::hal::gpio::Pin;
    use bsp::hal::gpio::PinState;
    use bsp::hal::gpio::PullDown;
    use bsp::hal::pac::PIO0;
    use bsp::hal::pac::UART1;
    use bsp::hal::pio::{PIOExt, SM0};
    use bsp::hal::uart::DataBits;
    use bsp::hal::uart::StopBits;
    use bsp::hal::uart::UartConfig;
    use bsp::hal::usb::UsbBus;
    use bsp::hal::Clock;
    use bsp::hal::Sio;
    use bsp::{hal, XOSC_CRYSTAL_FREQ};
    use core::iter::once;
    use core::mem::MaybeUninit;
    use defmt::info;
    use defmt::warn;
    use embedded_hal::digital::v2::InputPin;
    use fugit::RateExtU32;
    use rtic_monotonics::rp2040::Timer;
    use rtic_monotonics::Monotonic;
    // use fugit::RateExtU32;
    use rtic_monotonics::rp2040::ExtU64;
    use rtic_sync::channel::Receiver;
    use rtic_sync::channel::Sender;
    use rtic_sync::make_channel;
    use usb_device::class_prelude::UsbBusAllocator;
    use usb_device::prelude::UsbDeviceState;
    use ws2812_pio::Ws2812Direct;

    pub const EVENT_CAPACITY: usize = 200;
    pub const STENO_CAPACITY: usize = 8;

    type UartPinout = (
        Pin<Gpio8, FunctionUart, PullDown>,
        Pin<Gpio9, FunctionUart, PullDown>,
    );

    #[shared]
    struct Shared {
        inter_handler: inter::InterHandler<UART1, UartPinout>,
        layout_manager: LayoutManager,
        led_manager:
            leds::LedManager<Ws2812Direct<PIO0, SM0, Pin<DynPinId, FunctionPio0, PullDown>>>,
        usb_handler: usb::UsbHandler<'static, UsbBus>,
    }

    #[local]
    struct Local {
        matrix: MatrixType,
        usb_event: Sender<'static, Event, EVENT_CAPACITY>,
        inter_event: Sender<'static, Event, EVENT_CAPACITY>,
        event_event: Sender<'static, Event, EVENT_CAPACITY>,
        periodic_event: Sender<'static, Event, EVENT_CAPACITY>,
        dict: Dict,
    }

    #[init(local=[
        usb_bus: MaybeUninit<UsbBusAllocator<UsbBus>> = MaybeUninit::uninit()
    ])]
    fn init(mut ctx: init::Context) -> (Shared, Local) {
        // When using the picoprobe, it only resets the core and not any
        // peripherals. This causes it sometimes to get into a state where the
        // hardware spinlock is locked, and the first critical section will
        // deadlock. Work around this by resetting the spinlock.
        //
        // The rp2040 hal has a workaround in its `entry` macro. However, when
        // going through rtic, the general cortex-m `entry` is used, which
        // doesn't use the workaround, so we need to do it ourselves.
        unsafe {
            bsp::hal::sio::spinlock_reset();
        }

        // Initialize the heap.
        unsafe {
            HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE);
        }

        info!("Init running");

        let rp2040_timer_token = rtic_monotonics::create_rp2040_monotonic_token!();
        Timer::start(ctx.device.TIMER, &mut ctx.device.RESETS, rp2040_timer_token);

        let mut watchdog = hal::Watchdog::new(ctx.device.WATCHDOG);

        // External high-speed crystal on the pico board is 12Mhz
        let clocks = init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            ctx.device.XOSC,
            ctx.device.CLOCKS,
            ctx.device.PLL_SYS,
            ctx.device.PLL_USB,
            &mut ctx.device.RESETS,
            &mut watchdog,
        )
        .ok()
        .unwrap();

        let sio = Sio::new(ctx.device.SIO);

        let pins = bsp::Pins::new(
            ctx.device.IO_BANK0,
            ctx.device.PADS_BANK0,
            sio.gpio_bank0,
            &mut ctx.device.RESETS,
        );

        // Determine which side of the keyboard we are, based on a GPIO.
        let side_select = crate::board::side_pin!(pins);
        let side = if side_select.is_high().unwrap() {
            Side::Right
        } else {
            Side::Left
        };

        let (mut pio, sm0, _, _, _) = ctx.device.PIO0.split(&mut ctx.device.RESETS);
        let ws = Ws2812Direct::new(
            pins.led.into_function().into_dyn_pin(),
            &mut pio,
            sm0,
            clocks.peripheral_clock.freq(),
        );
        let led_manager = leds::LedManager::new(ws);

        // Build handler for the matrix handler.
        let matrix = {
            let cols = crate::board::cols!(pins);
            let rows = crate::board::rows!(pins);
            // TODO: Calculate side.
            Matrix::new(cols, rows, side)
        };

        let uart_pins = (
            pins.tx1.into_function::<hal::gpio::FunctionUart>(),
            pins.rx1.into_function::<hal::gpio::FunctionUart>(),
        );
        let uart =
            hal::uart::UartPeripheral::new(ctx.device.UART1, uart_pins, &mut ctx.device.RESETS)
                .enable(
                    // Ideally, being above 320k will allow full frames to be sent
                    // each tick. This number is chosen to be an exact divisor of
                    // the clock rate.
                    UartConfig::new(390625.Hz(), DataBits::Eight, None, StopBits::One),
                    clocks.peripheral_clock.freq(),
                )
                .unwrap();

        let inter_handler = inter::InterHandler::new(uart, side);

        let layout_manager = LayoutManager::new();

        let dict = Dict::new();

        let usb_bus: &'static _ =
            ctx.local
                .usb_bus
                .write(UsbBusAllocator::new(hal::usb::UsbBus::new(
                    ctx.device.USBCTRL_REGS,
                    ctx.device.USBCTRL_DPRAM,
                    clocks.usb_clock,
                    true,
                    &mut ctx.device.RESETS,
                )));
        let usb_handler = usb::UsbHandler::new(&usb_bus);

        let (event_send, event_receive) = make_channel!(Event, EVENT_CAPACITY);
        let (steno_send, steno_receive) = make_channel!(Stroke, STENO_CAPACITY);

        let usb_event = event_send.clone();
        let inter_event = event_send.clone();
        let event_event = event_send.clone();
        let periodic_event = event_send.clone();

        periodic_task::spawn().unwrap();
        event_task::spawn(event_receive, steno_send).unwrap();
        steno_task::spawn(steno_receive).unwrap();

        // let _timer = Timer::new(ctx.device.TIMER, &mut ctx.device.RESETS, &clocks);

        (
            Shared {
                inter_handler,
                layout_manager,
                led_manager,
                usb_handler,
            },
            Local {
                matrix,
                usb_event,
                inter_event,
                event_event,
                periodic_event,
                dict,
            },
        )
    }

    // USB normally doesn't have significant deadlines, as the controller will
    // happily NAK for us, and the deadlines to respond with data are on the
    // order of 100s of ms. However, the hal docs at
    // https://docs.rs/rp2040-hal/0.9.0/rp2040_hal/usb/index.html suggest that
    // there might be a race with enumeration on Windows. It says we _should_ be
    // able to avoid the issue by increasing the maximum endpoint-0 packet size.
    // Regardless, give the USB IRQ the highest priority to be able soon for the
    // initial packet.
    #[task(binds = USBCTRL_IRQ, shared = [usb_handler], local = [usb_event], priority = 4)]
    fn usbctrl_irq(mut cx: usbctrl_irq::Context) {
        cx.shared.usb_handler.lock(|usb_handler| {
            usb_handler.poll(cx.local.usb_event);
        });
    }

    // The UART task needs to be able to drain the FIFO before it fills at
    // 32-bytes. At 400-kbps, that gives us around 800us.
    #[task(binds = UART1_IRQ, shared = [inter_handler], local = [inter_event], priority = 3)]
    fn uart1_irq(mut cx: uart1_irq::Context) {
        cx.shared.inter_handler.lock(|inter_handler| {
            inter_handler.poll(cx.local.inter_event);
        });
    }

    /// Macro to assist with locking.
    macro_rules! lock {
        // Match the simple case.  Doesn't work.
        // ($ctx:ident.$var:ident, $body:expr) => {
        //     $ctx.shared.$var.lock(|$var| {
        //         $var.$body
        //     });
        // };
        ($ctx: ident, $var:ident, $body:expr) => {
            $ctx.shared.$var.lock(|$var| $body);
        };
    }

    /// The periodic task. This calls 'tick' on various manager subsystems, once
    /// every ms.
    #[task(shared = [usb_handler, inter_handler, layout_manager, led_manager],
           local = [periodic_event, matrix],
           priority = 2
    )]
    async fn periodic_task(mut ctx: periodic_task::Context) {
        let mut next = Timer::now();
        loop {
            next += 1.millis();
            Timer::delay_until(next).await;

            lock!(ctx, usb_handler, usb_handler.tick());
            lock!(ctx, inter_handler, inter_handler.tick());
            lock!(ctx, layout_manager, {
                layout_manager.tick(&mut EventWrapper(ctx.local.periodic_event));
            });
            lock!(ctx, led_manager, led_manager.tick());
            ctx.local.matrix.tick(ctx.local.periodic_event).await;
        }
    }

    /// The main event processor. This is responsible for receiving events, and
    /// dispatching them to appropriate other parts of the system.
    #[task(shared = [layout_manager, led_manager, inter_handler, usb_handler],
           local = [event_event],
           priority = 2)]
    async fn event_task(
        mut ctx: event_task::Context,
        mut recv: Receiver<'static, Event, EVENT_CAPACITY>,
        mut steno: Sender<'static, Stroke, STENO_CAPACITY>,
    ) {
        let mut last_size = 0;
        let mut state = InterState::Idle;
        let mut flashing = true;
        let mut usb_suspended = true;
        while let Ok(event) = recv.recv().await {
            match event {
                Event::Matrix(key) => {
                    match state {
                        InterState::Primary | InterState::Idle => {
                            lock!(ctx, layout_manager, {
                                layout_manager.handle_event(
                                    key,
                                    &mut EventWrapper(ctx.local.event_event),
                                );
                            });
                        }
                        InterState::Secondary => {
                            lock!(ctx, inter_handler, {
                                inter_handler.add_key(key);
                            });
                        }
                    }
                    if usb_suspended {
                        // This is specific to our implementation.
                        // TODO: Only do this if remote wakeup enabled.
                        lock!(ctx, usb_handler, {
                            usb_handler.wakeup();
                        });
                    }
                }
                Event::InterKey(key) => {
                    if state == InterState::Primary {
                        lock!(ctx, layout_manager, {
                            layout_manager.handle_event(
                                key,
                                &mut EventWrapper(ctx.local.event_event),
                            );
                        });
                    }
                }
                Event::Key(action) => {
                    lock!(ctx, usb_handler, usb_handler.enqueue(once(action)));
                }
                Event::RawSteno(stroke) => {
                    let mut buffer = ArrayString::<24>::new();
                    stroke.to_arraystring(&mut buffer);
                    let _ = steno.try_send(stroke);

                    // Enqueue with USB to send.
                    lock!(ctx, usb_handler, {
                        enqueue_action(usb_handler, buffer.as_str());
                        enqueue_action(usb_handler, " ");
                        usb_handler.enqueue(once(KeyAction::KeyRelease));
                    });
                }
                Event::UsbState(UsbDeviceState::Configured) => {
                    // TODO: Unclear how to handle suspend, but once we are
                    // configured, we need to start figuring out which side
                    // we are so we can communicate between the halves.
                    lock!(ctx, led_manager, {
                        led_manager.set_global(&leds::USB_PRIMARY);
                    });
                    flashing = true;
                    lock!(ctx, inter_handler, {
                        inter_handler.set_state(InterState::Primary, ctx.local.event_event);
                    });
                    usb_suspended = false;
                }
                Event::UsbState(UsbDeviceState::Suspend) => {
                    // This indicates the host has gone to sleep.
                    lock!(
                        ctx,
                        led_manager,
                        led_manager.set_global(&leds::SLEEP_INDICATOR)
                    );
                    // flashing = true;
                    usb_suspended = true;
                }
                Event::UsbState(_) => (),
                Event::BecomeState(new_state) => {
                    if state != new_state {
                        if new_state == InterState::Secondary {
                            info!("Secondary");
                            // We've gone into secondary state, stop blinking the LEDs.
                            lock!(ctx, led_manager, led_manager.clear_global());
                        } else if new_state == InterState::Idle {
                            info!("Idle");
                            lock!(ctx, led_manager, led_manager.clear_global());
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
                        lock!(ctx, led_manager, led_manager.clear_global());
                        flashing = false;
                    }
                }
                Event::Mode(mode) => {
                    let visible = match mode {
                        LayoutMode::Steno => &leds::STENO_INDICATOR,
                        LayoutMode::Artsey => &leds::ARTSEY_INDICATOR,
                        LayoutMode::Qwerty => &leds::QWERTY_INDICATOR,
                        LayoutMode::NKRO => &leds::NKRO_INDICATOR,
                    };
                    lock!(ctx, led_manager, led_manager.set_base(visible));
                }
                Event::ModeSelect(mode) => {
                    let visible = match mode {
                        LayoutMode::Steno => &leds::STENO_SELECT_INDICATOR,
                        LayoutMode::Artsey => &leds::ARTSEY_SELECT_INDICATOR,
                        LayoutMode::Qwerty => &leds::QWERTY_SELECT_INDICATOR,
                        LayoutMode::NKRO => &leds::NKRO_SELECT_INDICATOR,
                    };
                    lock!(ctx, led_manager, led_manager.set_base(visible));
                }
                Event::Indicator(mode) => {
                    let visible = match mode {
                        MinorMode::ArtseyMain => &leds::ARTSEY_INDICATOR,
                        MinorMode::ArtseyNav => &leds::ARTSEY_NAV_INDICATOR,
                    };
                    lock!(ctx, led_manager, led_manager.set_base(visible));
                }
                Event::Heartbeat => {
                    if flashing {
                        lock!(ctx, led_manager, {
                            led_manager.clear_global();
                        });
                        flashing = false;
                    }
                }
            }

            // Heap debugging is useful.
            let new_used = HEAP.used();
            if new_used > last_size {
                let free = HEAP.free();
                info!("Heap: {} used, {} free", new_used, free);
                last_size = new_used;
            }
        }
    }

    #[task(
        local = [dict],
        priority = 1,
    )]
    async fn steno_task(
        ctx: steno_task::Context,
        mut steno: Receiver<'static, Stroke, STENO_CAPACITY>
    ) {
        while let Ok(stroke) = steno.recv().await {
            for action in ctx.local.dict.handle_stroke(stroke, &WrapTimer) {
                info!("type action: {} del, {} add", action.remove, action.text.len());
            }
        }
    }

    /// Wrap the event queue in a way so that the bbq-keyboard package doesn't need
    /// to know how it is implemented.
    struct EventWrapper<'a>(&'a mut Sender<'static, Event, EVENT_CAPACITY>);

    impl<'a> EventQueue for EventWrapper<'a> {
        fn push(&mut self, event: Event) {
            if self.0.try_send(event).is_err() {
                warn!("Unable to queue event");
            }
        }
    }

    /// Placeholder for the timer, until we implement a real one.
    struct WrapTimer;

    impl Timable for WrapTimer {
        fn get_ticks(&self) -> u64 {
            Timer::now().ticks()
        }
    }
}

// This starts as zero, and can be set to various gate values to allow execution to continue.
#[used]
static GATE: AtomicU8 = AtomicU8::new(0);

// For the debugger.  Still until the debugger sets the GATE to at least this value.
#[inline(never)]
#[allow(dead_code)]
fn stall(gate: u8) {
    while GATE.load(Ordering::Acquire) < gate {}
}
