//! Board-specific initialization.
//!
//! This module initializes all of the various hardware devices used by the keyboard firmware, as
//! appropriate for the board information we have determined.

use bbq_keyboard::{boardinfo::BoardInfo, KeyAction, KeyEvent, Side};
use embassy_executor::SendSpawner;
use embassy_rp::Peripherals;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Receiver}};
use smart_leds::RGB8;

use crate::{inter::InterPassive, inter_uart::InterActive, leds::LedSet, matrix::Matrix};

// Board specific for the jolt3.
mod jolt3 {
    use bbq_keyboard::{KeyAction, KeyEvent, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{
        gpio::{AnyPin, Input, Level, Output, Pull}, i2c, i2c_slave, peripherals::{self, I2C1, PIO0}, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peri, Peripherals
    };
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Sender}};
    use static_cell::StaticCell;

    use crate::{board::Inter, inter::{InterPassive, PassiveTask}, logging::unwrap};
    use crate::{
        leds::{
            led_strip::{LedStripGroup, LedStripHandle},
            LedSet,
        },
        matrix::Matrix,
        Irqs,
    };

    use super::{Board, UsbHandler};

    // Split up the periperals for each init.
    struct MatrixResources {
        pin_0: Peri<'static, peripherals::PIN_0>,
        pin_1: Peri<'static, peripherals::PIN_1>,
        pin_2: Peri<'static, peripherals::PIN_2>,
        pin_3: Peri<'static, peripherals::PIN_3>,
        pin_4: Peri<'static, peripherals::PIN_4>,
        pin_5: Peri<'static, peripherals::PIN_5>,
        pin_6: Peri<'static, peripherals::PIN_6>,
        pin_7: Peri<'static, peripherals::PIN_7>,
        pin_8: Peri<'static, peripherals::PIN_8>,
        pin_9: Peri<'static, peripherals::PIN_9>,
    }

    struct RgbResources {
        pin_19: Peri<'static, peripherals::PIN_19>,
        pio0: Peri<'static, peripherals::PIO0>,
        dma_ch0: Peri<'static, peripherals::DMA_CH0>,
    }

    struct I2cResources {
        pin_10: Peri<'static, peripherals::PIN_10>,
        pin_11: Peri<'static, peripherals::PIN_11>,
        pin_12: Peri<'static, peripherals::PIN_12>,
        pin_13: Peri<'static, peripherals::PIN_13>,
        i2c1: Peri<'static, peripherals::I2C1>,
    }

    struct UsbResources {
        usb: Peri<'static, peripherals::USB>,
    }

    pub fn new_left(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(MatrixResources {
            pin_0: p.PIN_0, pin_1: p.PIN_1, pin_2: p.PIN_2, pin_3: p.PIN_3, pin_4: p.PIN_4,
            pin_5: p.PIN_5, pin_6: p.PIN_6, pin_7: p.PIN_7, pin_8: p.PIN_8, pin_9: p.PIN_9,
        }, Side::Left);
        let leds = leds_init(RgbResources {
            pin_19: p.PIN_19, pio0: p.PIO0, dma_ch0: p.DMA_CH0,
        }, spawner);

        let mut config = i2c::Config::default();
        config.frequency = 400_000;
        let i2c = I2cResources {
            pin_10: p.PIN_10, pin_11: p.PIN_11, pin_12: p.PIN_12, pin_13: p.PIN_13, i2c1: p.I2C1,
        };
        let bus = i2c::I2c::new_async(i2c.i2c1, i2c.pin_11, i2c.pin_10, Irqs, config);
        let irq = Input::new(i2c.pin_13, Pull::None);

        static CHAN: StaticCell<Channel<CriticalSectionRawMutex, KeyEvent, 1>> = StaticCell::new();
        let key_chan = CHAN.init(Channel::new());

        spawner.spawn(unwrap!(active_task(bus, irq, key_chan.sender())));

        let usb = usb_init(UsbResources { usb: p.USB }, spawner, unique);

        Board {
            matrix,
            leds,
            inter: Inter::ActiveI2C(key_chan.receiver()),
            usb: Some(usb),
            two_row: false,
        }
    }

    #[embassy_executor::task]
    async fn active_task(
        bus: i2c::I2c<'static, I2C1, i2c::Async>,
        irq: Input<'static>,
        sender: Sender<'static, CriticalSectionRawMutex, KeyEvent, 1>,
    ) -> ! {
        crate::inter::active_task(irq, bus, sender).await;
    }

    pub fn new_right(p: Peripherals, spawner: SendSpawner) -> Board {
        let matrix = matrix_init(MatrixResources {
            pin_0: p.PIN_0, pin_1: p.PIN_1, pin_2: p.PIN_2, pin_3: p.PIN_3, pin_4: p.PIN_4,
            pin_5: p.PIN_5, pin_6: p.PIN_6, pin_7: p.PIN_7, pin_8: p.PIN_8, pin_9: p.PIN_9,
        }, Side::Right);
        let leds = leds_init(RgbResources {
            pin_19: p.PIN_19, pio0: p.PIO0, dma_ch0: p.DMA_CH0,
        }, spawner);

        let mut config = i2c_slave::Config::default();
        config.addr = 0x42;
        let i2c = I2cResources {
            pin_10: p.PIN_10, pin_11: p.PIN_11, pin_12: p.PIN_12, pin_13: p.PIN_13, i2c1: p.I2C1,
        };
        let bus = i2c_slave::I2cSlave::new(i2c.i2c1, i2c.pin_11, i2c.pin_10, Irqs, config);
        let irq = Output::new(i2c.pin_12, Level::Low);

        let (passive, task_data) = InterPassive::new(bus, irq);

        spawner.spawn(unwrap!(passive_task(task_data)));

        Board {
            matrix,
            leds,
            inter: Inter::PassiveI2C(passive),
            usb: None,
            two_row: false,
        }
    }

    #[embassy_executor::task]
    async fn passive_task(task: PassiveTask<I2C1>) {
        task.handler().await
    }


    fn matrix_init(r: MatrixResources, side: Side) -> Matrix {
        // The keyboard matrix.
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.pin_6.into(),
                r.pin_7.into(),
                r.pin_8.into(),
                r.pin_9.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.pin_0.into(),
                r.pin_2.into(),
                r.pin_1.into(),
                r.pin_3.into(),
                r.pin_5.into(),
                r.pin_4.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Input::new(p, Pull::Down)),
        );

        let xlate = bbq_keyboard::translate::get_translation("jolt3");

        Matrix::new(cols, rows, xlate, side)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio0, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma_ch0, Irqs, r.pin_19, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        spawner.spawn(unwrap!(led_task(leds)));

        LedSet::new([strip])
    }

    #[embassy_executor::task]
    async fn led_task(leds: LedStripGroup<'static, PIO0, 0, 2>) {
        leds.update_task().await;
    }

    fn usb_init(r: UsbResources, spawner: SendSpawner, unique: &'static str) -> UsbHandler {
        static KEYS: StaticCell<Channel<CriticalSectionRawMutex, KeyAction, 8>> = StaticCell::new();

        let usb = UsbHandler {
            keys: KEYS.init(Channel::new()),
        };

        spawner.spawn(unwrap!(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

        usb
    }
}

mod jolt2 {
    //! The jolt2 is the first tiered keyboard, built around the pimoroni Tiny 2040.  (Note, this is
    //! distinct from the jolt2dir, which is effectively the same as the jolt2, but the rp2040 is
    //! directly on the board.  This was only ever made in the left-side variant, so it is common to
    //! combine with the jolt2 as the right side.  The Zephyr-based firmware expects 'jolt2' for
    //! both, and they are distinguished at build time.  Instead, we expect the jolt2dir to identify
    //! itself as such).

    use bbq_keyboard::Side;
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{AnyPin, Input, Level, Output, Pull}, peripherals, uart::{BufferedUart, BufferedUartRx, BufferedUartTx, DataBits, Parity, StopBits}, Peri, Peripherals};
    use static_cell::StaticCell;

    use crate::{inter_uart::InterPassive, leds::LedSet, matrix::Matrix, Irqs};
    use crate::logging::unwrap;

    use super::{Board, Inter};

    // Split up the peripherals.
    struct MatrixResources {
        pin_2: Peri<'static, peripherals::PIN_2>,
        pin_3: Peri<'static, peripherals::PIN_3>,
        pin_4: Peri<'static, peripherals::PIN_4>,
        pin_5: Peri<'static, peripherals::PIN_5>,
        pin_6: Peri<'static, peripherals::PIN_6>,
        pin_26: Peri<'static, peripherals::PIN_26>,
        pin_7: Peri<'static, peripherals::PIN_7>,
        pin_27: Peri<'static, peripherals::PIN_27>,
        pin_29: Peri<'static, peripherals::PIN_29>,
        pin_28: Peri<'static, peripherals::PIN_28>,
    }

    struct UartResources {
        uart: Peri<'static, peripherals::UART0>,
        tx: Peri<'static, peripherals::PIN_0>,
        rx: Peri<'static, peripherals::PIN_1>,
    }

    pub fn new_right(p: Peripherals, spawner: SendSpawner) -> Board {
        let _ = spawner;

        // For now, construct an empty led, until we have something to write to the led.
        let leds = LedSet::new([]);
        let matrix = matrix_init(MatrixResources {
            pin_2: p.PIN_2, pin_3: p.PIN_3, pin_4: p.PIN_4, pin_5: p.PIN_5, pin_6: p.PIN_6,
            pin_26: p.PIN_26, pin_7: p.PIN_7, pin_27: p.PIN_27, pin_29: p.PIN_29, pin_28: p.PIN_28,
        }, Side::Right);
        let uart = uart_init(UartResources {
            uart: p.UART0, tx: p.PIN_0, rx: p.PIN_1,
        }, spawner);

        Board {
            matrix,
            leds,
            inter: Inter::PassiveUart(uart),
            usb: None,
            two_row: false,
        }
    }

    fn matrix_init(r: MatrixResources, side: Side) -> Matrix {
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.pin_2.into(),
                r.pin_3.into(),
                r.pin_4.into(),
                r.pin_5.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.pin_6.into(),
                r.pin_26.into(),
                r.pin_7.into(),
                r.pin_27.into(),
                r.pin_29.into(),
                r.pin_28.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Input::new(p, Pull::Down)),
        );

        let xlate = bbq_keyboard::translate::get_translation("jolt2");

        Matrix::new(cols, rows, xlate, side)
    }

    fn uart_init(r: UartResources, spawner: SendSpawner) -> &'static InterPassive {
        // TODO: This is shared, don't duplicate.
        let mut config = embassy_rp::uart::Config::default();
        config.baudrate = 460800;
        config.stop_bits = StopBits::STOP1;
        config.data_bits = DataBits::DataBits8;
        config.parity = Parity::ParityNone;

        static TX_BUF: StaticCell<[u8; 64]> = StaticCell::new();
        let tx_buf = &mut TX_BUF.init([0; 64])[..];
        static RX_BUF: StaticCell<[u8; 64]> = StaticCell::new();
        let rx_buf = &mut RX_BUF.init([0; 64])[..];

        static UART: StaticCell<BufferedUart> = StaticCell::new();
        let uart = UART.init(BufferedUart::new(
            r.uart,
            r.tx,
            r.rx,
            Irqs,
            tx_buf,
            rx_buf,
            config,
        ));

        let (tx, rx) = uart.split_ref();

        static PASSIVE: StaticCell<InterPassive> = StaticCell::new();
        let passive = PASSIVE.init(InterPassive::new());
        spawner.spawn(unwrap!(passive_tx_task(passive, tx)));
        spawner.spawn(unwrap!(passive_rx_task(passive, rx)));

        passive
    }

    #[embassy_executor::task]
    async fn passive_tx_task(passive: &'static InterPassive, tx: &'static mut BufferedUartTx) -> ! {
        passive.tx_task(tx).await
    }

    #[embassy_executor::task]
    async fn passive_rx_task(passive: &'static InterPassive, rx: &'static mut BufferedUartRx) -> ! {
        passive.rx_task(rx).await
    }
}

mod jolt2dir {
    //! The jolt2dir is a variant on the jolt2, where instead the Pimotoni Tiny 2040, the rp2040 and
    //! support circuitry is all directly made onto the board.

    use bbq_keyboard::{KeyAction, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{AnyPin, Input, Level, Output, Pull}, peripherals, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, uart::{BufferedUart, BufferedUartRx, BufferedUartTx, DataBits, Parity, StopBits}, Peri, Peripherals};
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
    use static_cell::StaticCell;

    use crate::{inter_uart::InterActive, leds::{led_strip::{LedStripGroup, LedStripHandle}, LedSet}, matrix::Matrix, Irqs};
    use crate::logging::unwrap;

    use super::{Board, Inter, UsbHandler};

    /// The PIO instance that drives the RGB LEDs.
    type RgbPIO = peripherals::PIO0;

    // Split up the peripherals.
    struct MatrixResources {
        row0: Peri<'static, peripherals::PIN_4>,
        row1: Peri<'static, peripherals::PIN_6>,
        row2: Peri<'static, peripherals::PIN_5>,
        row3: Peri<'static, peripherals::PIN_7>,
        row4: Peri<'static, peripherals::PIN_9>,
        row5: Peri<'static, peripherals::PIN_8>,
        col0: Peri<'static, peripherals::PIN_2>,
        col1: Peri<'static, peripherals::PIN_1>,
        col2: Peri<'static, peripherals::PIN_0>,
        col3: Peri<'static, peripherals::PIN_3>,
    }

    struct RgbResources {
        rgb_pin: Peri<'static, peripherals::PIN_13>,
        pio: Peri<'static, RgbPIO>,
        dma: Peri<'static, peripherals::DMA_CH0>,
    }

    struct UsbResources {
        usb: Peri<'static, peripherals::USB>,
    }

    struct UartResources {
        uart: Peri<'static, peripherals::UART0>,
        tx: Peri<'static, peripherals::PIN_28>,
        rx: Peri<'static, peripherals::PIN_29>,
    }

    pub fn new_left(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(MatrixResources {
            row0: p.PIN_4, row1: p.PIN_6, row2: p.PIN_5, row3: p.PIN_7, row4: p.PIN_9, row5: p.PIN_8,
            col0: p.PIN_2, col1: p.PIN_1, col2: p.PIN_0, col3: p.PIN_3,
        }, Side::Left);
        let leds = leds_init(RgbResources {
            rgb_pin: p.PIN_13, pio: p.PIO0, dma: p.DMA_CH0,
        }, spawner);

        let usb = usb_init(UsbResources { usb: p.USB }, spawner, unique);
        let uart = uart_init(UartResources {
            uart: p.UART0, tx: p.PIN_28, rx: p.PIN_29,
        }, spawner);

        Board {
            matrix,
            leds,
            inter: Inter::ActiveUart(uart),
            usb: Some(usb),
            two_row: false,
        }
    }

    fn matrix_init(r: MatrixResources, side: Side) -> Matrix {
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.col0.into(),
                r.col1.into(),
                r.col2.into(),
                r.col3.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.row0.into(),
                r.row1.into(),
                r.row2.into(),
                r.row3.into(),
                r.row4.into(),
                r.row5.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Input::new(p, Pull::Down)),
        );

        let xlate = bbq_keyboard::translate::get_translation("jolt2");

        Matrix::new(cols, rows, xlate, side)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma, Irqs, r.rgb_pin, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        spawner.spawn(unwrap!(led_task(leds)));

        LedSet::new([strip])
    }

    #[embassy_executor::task]
    async fn led_task(leds: LedStripGroup<'static, RgbPIO, 0, 2>) {
        leds.update_task().await;
    }

    fn usb_init(r: UsbResources, spawner: SendSpawner, unique: &'static str) -> UsbHandler {
        static KEYS: StaticCell<Channel<CriticalSectionRawMutex, KeyAction, 8>> = StaticCell::new();

        let usb = UsbHandler {
            keys: KEYS.init(Channel::new()),
        };

        spawner.spawn(unwrap!(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

        usb
    }

    fn uart_init(r: UartResources, spawner: SendSpawner) -> &'static InterActive {
        let mut config = embassy_rp::uart::Config::default();
        config.baudrate = 460800;
        config.stop_bits = StopBits::STOP1;
        config.data_bits = DataBits::DataBits8;
        config.parity = Parity::ParityNone;

        static TX_BUF: StaticCell<[u8; 64]> = StaticCell::new();
        let tx_buf = &mut TX_BUF.init([0; 64])[..];
        static RX_BUF: StaticCell<[u8; 64]> = StaticCell::new();
        let rx_buf = &mut RX_BUF.init([0; 64])[..];

        static UART: StaticCell<BufferedUart> = StaticCell::new();
        let uart = UART.init(BufferedUart::new(
            r.uart,
            r.tx,
            r.rx,
            Irqs,
            tx_buf,
            rx_buf,
            config,
        ));

        let (tx, rx) = uart.split_ref();

        static ACTIVE: StaticCell<InterActive> = StaticCell::new();
        let active = ACTIVE.init(InterActive::new());
        spawner.spawn(unwrap!(active_tx_task(active, tx)));
        spawner.spawn(unwrap!(active_rx_task(active, rx)));

        active
    }

    #[embassy_executor::task]
    async fn active_tx_task(active: &'static InterActive, tx: &'static mut BufferedUartTx) -> ! {
        active.tx_task(tx).await
    }

    #[embassy_executor::task]
    async fn active_rx_task(active: &'static InterActive, rx: &'static mut BufferedUartRx) -> ! {
        active.rx_task(rx).await
    }
}

mod proto4 {
    //! The proto4 is a 30-key, 2-row keyboard built around the Pimoroni Tiny 2040.  The connector
    //! between the halves carries the full matrix, so the single MCU scans all 30 keys and there is
    //! no inter-board protocol at all.
    //!
    //! In addition to the 4 ws2812 LEDs on the keyboard itself, the Tiny 2040 has an RGB LED driven
    //! by PWM.  That one is not yet supported, as there is no PWM `LedGroup` implementation.

    use bbq_keyboard::{KeyAction, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{AnyPin, Input, Level, Output, Pull}, peripherals, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peri, Peripherals};
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
    use static_cell::StaticCell;

    use crate::{leds::{led_strip::{LedStripGroup, LedStripHandle}, LedSet}, matrix::Matrix, Irqs};
    use crate::logging::unwrap;

    use super::{Board, Inter, UsbHandler};

    /// The PIO instance that drives the RGB LEDs.
    type RgbPIO = peripherals::PIO0;

    /// The number of ws2812 LEDs on the keyboard.
    const NUM_LEDS: usize = 4;

    // Split up the peripherals.
    struct MatrixResources {
        row0: Peri<'static, peripherals::PIN_0>,
        row1: Peri<'static, peripherals::PIN_1>,
        row2: Peri<'static, peripherals::PIN_2>,
        row3: Peri<'static, peripherals::PIN_3>,
        row4: Peri<'static, peripherals::PIN_4>,
        row5: Peri<'static, peripherals::PIN_5>,
        col0: Peri<'static, peripherals::PIN_6>,
        col1: Peri<'static, peripherals::PIN_7>,
        col2: Peri<'static, peripherals::PIN_26>,
        col3: Peri<'static, peripherals::PIN_27>,
        col4: Peri<'static, peripherals::PIN_28>,
    }

    struct RgbResources {
        rgb_pin: Peri<'static, peripherals::PIN_29>,
        pio: Peri<'static, RgbPIO>,
        dma: Peri<'static, peripherals::DMA_CH0>,
    }

    struct UsbResources {
        usb: Peri<'static, peripherals::USB>,
    }

    pub fn new(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(MatrixResources {
            row0: p.PIN_0, row1: p.PIN_1, row2: p.PIN_2, row3: p.PIN_3, row4: p.PIN_4, row5: p.PIN_5,
            col0: p.PIN_6, col1: p.PIN_7, col2: p.PIN_26, col3: p.PIN_27, col4: p.PIN_28,
        });
        let leds = leds_init(RgbResources {
            rgb_pin: p.PIN_29, pio: p.PIO0, dma: p.DMA_CH0,
        }, spawner);

        let usb = usb_init(UsbResources { usb: p.USB }, spawner, unique);

        Board {
            matrix,
            leds,
            inter: Inter::None,
            usb: Some(usb),
            two_row: true,
        }
    }

    fn matrix_init(r: MatrixResources) -> Matrix {
        static COLS: StaticCell<[Output<'static>; 5]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.col0.into(),
                r.col1.into(),
                r.col2.into(),
                r.col3.into(),
                r.col4.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.row0.into(),
                r.row1.into(),
                r.row2.into(),
                r.row3.into(),
                r.row4.into(),
                r.row5.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Input::new(p, Pull::Down)),
        );

        let xlate = bbq_keyboard::translate::get_translation("proto4");

        // A single MCU scans the whole matrix, so there is no bias to apply to the scan codes.
        Matrix::new(cols, rows, xlate, Side::Left)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma, Irqs, r.rgb_pin, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        spawner.spawn(unwrap!(led_task(leds)));

        LedSet::new([strip])
    }

    #[embassy_executor::task]
    async fn led_task(leds: LedStripGroup<'static, RgbPIO, 0, NUM_LEDS>) {
        leds.update_task().await;
    }

    fn usb_init(r: UsbResources, spawner: SendSpawner, unique: &'static str) -> UsbHandler {
        static KEYS: StaticCell<Channel<CriticalSectionRawMutex, KeyAction, 8>> = StaticCell::new();

        let usb = UsbHandler {
            keys: KEYS.init(Channel::new()),
        };

        spawner.spawn(unwrap!(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

        usb
    }
}

mod mesa1 {
    //! The mesa1 is a 30-key, 2-row keyboard built around the Pimoroni Tiny 2040.  The keys are the
    //! proto4's, in the same places and with the same meanings, but the matrix is wired for routing
    //! rather than for compatibility: five driven lines are shared between the halves, and each half
    //! has three sensed lines, so the RJ-45 between the halves carries only 8 conductors.  As with
    //! the proto4, the single MCU scans all 30 keys and there is no inter-board protocol.
    //!
    //! The mesa1 documentation calls the shared lines "rows" (`ROW_A`..`ROW_E`) and the per-half
    //! lines "columns" (`COL_1`..`COL_6`).  The diodes conduct from column to row, and the scanner
    //! drives its columns while sensing its rows, so the two roles are swapped here: the scanner's
    //! columns are the mesa1's `COL_1`..`COL_6`, and the scanner's rows are its `ROW_A`..`ROW_E`.
    //!
    //! As on the proto4, the Tiny 2040's PWM RGB LED is not yet supported, only the 4 ws2812 LEDs
    //! on the keyboard itself.

    use bbq_keyboard::{KeyAction, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{AnyPin, Input, Level, Output, Pull}, peripherals, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peri, Peripherals};
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
    use static_cell::StaticCell;

    use crate::{leds::{led_strip::{LedStripGroup, LedStripHandle}, LedSet}, matrix::Matrix, Irqs};
    use crate::logging::unwrap;

    use super::{Board, Inter, UsbHandler};

    /// The PIO instance that drives the RGB LEDs.
    type RgbPIO = peripherals::PIO0;

    /// The number of ws2812 LEDs on the keyboard.
    const NUM_LEDS: usize = 4;

    // Split up the peripherals.  Named for the mesa1 nets, not for the scanner's roles.
    struct MatrixResources {
        row_a: Peri<'static, peripherals::PIN_4>,
        row_b: Peri<'static, peripherals::PIN_3>,
        row_c: Peri<'static, peripherals::PIN_2>,
        row_d: Peri<'static, peripherals::PIN_1>,
        row_e: Peri<'static, peripherals::PIN_0>,
        col_1: Peri<'static, peripherals::PIN_27>,
        col_2: Peri<'static, peripherals::PIN_28>,
        col_3: Peri<'static, peripherals::PIN_29>,
        col_4: Peri<'static, peripherals::PIN_7>,
        col_5: Peri<'static, peripherals::PIN_6>,
        col_6: Peri<'static, peripherals::PIN_5>,
    }

    struct RgbResources {
        rgb_pin: Peri<'static, peripherals::PIN_26>,
        pio: Peri<'static, RgbPIO>,
        dma: Peri<'static, peripherals::DMA_CH0>,
    }

    struct UsbResources {
        usb: Peri<'static, peripherals::USB>,
    }

    pub fn new(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(MatrixResources {
            row_a: p.PIN_4, row_b: p.PIN_3, row_c: p.PIN_2, row_d: p.PIN_1, row_e: p.PIN_0,
            col_1: p.PIN_27, col_2: p.PIN_28, col_3: p.PIN_29,
            col_4: p.PIN_7, col_5: p.PIN_6, col_6: p.PIN_5,
        });
        let leds = leds_init(RgbResources {
            rgb_pin: p.PIN_26, pio: p.PIO0, dma: p.DMA_CH0,
        }, spawner);

        let usb = usb_init(UsbResources { usb: p.USB }, spawner, unique);

        Board {
            matrix,
            leds,
            inter: Inter::None,
            usb: Some(usb),
            two_row: true,
        }
    }

    fn matrix_init(r: MatrixResources) -> Matrix {
        // Driven: the mesa1's columns, which the diodes let feed the rows.
        static COLS: StaticCell<[Output<'static>; 6]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.col_1.into(),
                r.col_2.into(),
                r.col_3.into(),
                r.col_4.into(),
                r.col_5.into(),
                r.col_6.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Output::new(p, Level::Low)),
        );

        // Sensed: the mesa1's rows, shared by both halves.
        static ROWS: StaticCell<[Input<'static>; 5]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.row_a.into(),
                r.row_b.into(),
                r.row_c.into(),
                r.row_d.into(),
                r.row_e.into(),
            ]
            .map(|p: Peri<'static, AnyPin>| Input::new(p, Pull::Down)),
        );

        let xlate = bbq_keyboard::translate::get_translation("mesa1");

        // A single MCU scans the whole matrix, so there is no bias to apply to the scan codes.
        Matrix::new(cols, rows, xlate, Side::Left)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma, Irqs, r.rgb_pin, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        spawner.spawn(unwrap!(led_task(leds)));

        LedSet::new([strip])
    }

    #[embassy_executor::task]
    async fn led_task(leds: LedStripGroup<'static, RgbPIO, 0, NUM_LEDS>) {
        leds.update_task().await;
    }

    fn usb_init(r: UsbResources, spawner: SendSpawner, unique: &'static str) -> UsbHandler {
        static KEYS: StaticCell<Channel<CriticalSectionRawMutex, KeyAction, 8>> = StaticCell::new();

        let usb = UsbHandler {
            keys: KEYS.init(Channel::new()),
        };

        spawner.spawn(unwrap!(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

        usb
    }
}

/// Channel type for key event messages.
pub type KeyChannel = Receiver<'static, CriticalSectionRawMutex, KeyEvent, 1>;

pub struct UsbHandler {
    /// Channel for handling keys.  The USB task listens to this.
    pub keys: &'static Channel<CriticalSectionRawMutex, KeyAction, 8>,
}

/// The inter-board connection should be one of these.
pub enum Inter {
    PassiveI2C(InterPassive),
    ActiveI2C(KeyChannel),
    PassiveUart(&'static crate::inter_uart::InterPassive),
    ActiveUart(&'static InterActive),
    /// There is no other half at all.  Either a non-split board, or one where a single MCU scans
    /// the entire matrix.
    None,
}

impl Inter {
    /// Does this side run the layout engine?
    ///
    /// True for the active side of a split board, and for boards that have no other half.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::ActiveI2C(_) | Self::ActiveUart(_) | Self::None)
    }
}

/// The Initialized board.  Some here are optional, as the different parts are not used in all
/// configurations.
pub struct Board {
    /// The keyboard matrix.  Always present.
    pub matrix: Matrix,
    /// The leds, always present
    pub leds: LedSet,
    /// How the inter-board handler is implemented.
    pub inter: Inter,
    /// The communication channels with the USB tasks
    pub usb: Option<UsbHandler>,
    /// Is this a 2-row keyboard?  These have fewer keys, and select modes differently, with the
    /// upper-left key acting as the "Fn" key.
    pub two_row: bool,
}

impl Board {
    pub fn new(p: Peripherals, spawner: SendSpawner, info: &BoardInfo, unique: &'static str) -> Board {
        match info {
            BoardInfo {
                name,
                side: Some(Side::Left),
            } if name == "jolt3" => {
                let mut this = jolt3::new_left(p, spawner, unique);
                this.leds.update(&[RGB8::new(0, 8, 8), RGB8::new(8, 8, 0)]);
                this
            }
            BoardInfo {
                name,
                side: Some(Side::Right),
            } if name == "jolt3" => {
                let mut this = jolt3::new_right(p, spawner);
                this.leds.update(&[RGB8::new(0, 8, 8), RGB8::new(8, 8, 0)]);
                this
            }
            BoardInfo {
                name,
                side: Some(Side::Right),
            } if name == "jolt2" => {
                let this = jolt2::new_right(p, spawner);
                // this.leds.update(&[RGB8::new(8, 8, 0)]);
                this
            }
            BoardInfo {
                name,
                side: Some(Side::Left),
            } if name == "jolt2dir" => {
                let mut this = jolt2dir::new_left(p, spawner, unique);
                this.leds.update(&[RGB8::new(8, 8, 0), RGB8::new(8, 8, 0)]);
                this
            }
            BoardInfo { name, side: None } if name == "proto4" => {
                let mut this = proto4::new(p, spawner, unique);
                this.leds.update(&[
                    RGB8::new(8, 0, 0),
                    RGB8::new(0, 8, 0),
                    RGB8::new(0, 0, 8),
                    RGB8::new(8, 8, 0),
                ]);
                this
            }
            BoardInfo { name, side: None } if name == "mesa1" => {
                let mut this = mesa1::new(p, spawner, unique);
                this.leds.update(&[
                    RGB8::new(8, 0, 0),
                    RGB8::new(0, 8, 0),
                    RGB8::new(0, 0, 8),
                    RGB8::new(8, 8, 0),
                ]);
                this
            }
            info => {
                panic!("Unsupported board: {:?}", info);
            }
        }
    }
}
