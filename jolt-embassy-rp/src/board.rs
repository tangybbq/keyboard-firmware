//! Board-specific initialization.
//!
//! This module initializes all of the various hardware devices used by the keyboard firmware, as
//! appropriate for the board information we have determined.

use bbq_keyboard::{boardinfo::BoardInfo, KeyAction, KeyEvent, Side};
use embassy_executor::SendSpawner;
use embassy_rp::Peripherals;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Receiver}};
use smart_leds::RGB8;

use crate::{inter::InterPassive, leds::LedSet, matrix::Matrix};

// Board specific for the jolt3.
mod jolt3 {
    use bbq_keyboard::{KeyAction, KeyEvent, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{
        gpio::{Input, Level, Output, Pin, Pull}, i2c, i2c_slave, peripherals::{self, PIO0}, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peripherals
    };
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::{Channel, Sender}};
    use embedded_resources::resource_group;
    use static_cell::StaticCell;

    use crate::{inter::{InterPassive, PassiveTask}, logging::unwrap};
    use crate::{
        leds::{
            led_strip::{LedStripGroup, LedStripHandle},
            LedSet,
        },
        matrix::Matrix,
        translate, Irqs,
    };

    use super::{Board, UsbHandler};

    // Split up the periperals for each init.
    #[resource_group]
    struct MatrixResources {
        pin_0: peripherals::PIN_0,
        pin_1: peripherals::PIN_1,
        pin_2: peripherals::PIN_2,
        pin_3: peripherals::PIN_3,
        pin_4: peripherals::PIN_4,
        pin_5: peripherals::PIN_5,
        pin_6: peripherals::PIN_6,
        pin_7: peripherals::PIN_7,
        pin_8: peripherals::PIN_8,
        pin_9: peripherals::PIN_9,
    }

    #[resource_group]
    struct RgbResources {
        pin_19: peripherals::PIN_19,
        pio0: peripherals::PIO0,
        dma_ch0: peripherals::DMA_CH0,
    }

    #[resource_group]
    struct I2cResources {
        pin_10: peripherals::PIN_10,
        pin_11: peripherals::PIN_11,
        pin_12: peripherals::PIN_12,
        pin_13: peripherals::PIN_13,
        i2c1: peripherals::I2C1,
    }

    #[resource_group]
    struct UsbResources {
        usb: peripherals::USB,
    }

    pub fn new_left(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(matrix_resources!(p), Side::Left);
        let leds = leds_init(rgb_resources!(p), spawner);

        let mut config = i2c::Config::default();
        config.frequency = 400_000;
        let i2c = i_2c_resources!(p);
        let bus = i2c::I2c::new_async(i2c.i2c1, i2c.pin_11, i2c.pin_10, Irqs, config);
        let irq = Input::new(i2c.pin_13, Pull::None);

        static CHAN: StaticCell<Channel<CriticalSectionRawMutex, KeyEvent, 1>> = StaticCell::new();
        let key_chan = CHAN.init(Channel::new());

        unwrap!(spawner.spawn(active_task(bus, irq, key_chan.sender())));

        let usb = usb_init(usb_resources!(p), spawner, unique);

        Board {
            matrix,
            leds,
            passive: None,
            active_keys: Some(key_chan.receiver()),
            usb: Some(usb),
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
        let matrix = matrix_init(matrix_resources!(p), Side::Right);
        let leds = leds_init(rgb_resources!(p), spawner);

        let mut config = i2c_slave::Config::default();
        config.addr = 0x42;
        let i2c = i_2c_resources!(p);
        let bus = i2c_slave::I2cSlave::new(i2c.i2c1, i2c.pin_11, i2c.pin_10, Irqs, config);
        let irq = Output::new(i2c.pin_12, Level::Low);

        let (passive, task_data) = InterPassive::new(bus, irq);

        unwrap!(spawner.spawn(passive_task(task_data)));

        Board {
            matrix,
            leds,
            passive: Some(passive),
            active_keys: None,
            usb: None,
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
                r.pin_6.degrade(),
                r.pin_7.degrade(),
                r.pin_8.degrade(),
                r.pin_9.degrade(),
            ]
            .map(|p| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.pin_0.degrade(),
                r.pin_2.degrade(),
                r.pin_1.degrade(),
                r.pin_3.degrade(),
                r.pin_5.degrade(),
                r.pin_4.degrade(),
            ]
            .map(|p| Input::new(p, Pull::Down)),
        );

        let xlate = translate::get_translation("jolt3");

        Matrix::new(cols, rows, xlate, side)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio0, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma_ch0, r.pin_19, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        unwrap! {spawner.spawn(led_task(leds))};

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

        unwrap!(spawner.spawn(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

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
    use embassy_rp::{gpio::{Input, Level, Output, Pin, Pull}, peripherals, Peripherals};
    use embedded_resources::resource_group;
    use static_cell::StaticCell;

    use crate::{matrix::{Matrix, MatrixAction}, translate};
    use crate::logging::{info, unwrap};

    use super::Board;

    // Split up the peripherals.
    #[resource_group]
    struct MatrixResources {
        pin_2: peripherals::PIN_2,
        pin_3: peripherals::PIN_3,
        pin_4: peripherals::PIN_4,
        pin_5: peripherals::PIN_5,
        pin_6: peripherals::PIN_6,
        pin_26: peripherals::PIN_26,
        pin_7: peripherals::PIN_7,
        pin_27: peripherals::PIN_27,
        pin_29: peripherals::PIN_29,
        pin_28: peripherals::PIN_28,
    }

    pub fn new_right(p: Peripherals, spawner: SendSpawner) -> Board {
        let _ = spawner;

        let matrix = matrix_init(matrix_resources!(p), Side::Right);

        // Testing, just scan the matrix.
        unwrap!(spawner.spawn(matrix_loop(matrix)));

        // We're at the lowest priority, so just spin.
        loop { }
    }

    // Testing/debugging of the matrix.
    #[embassy_executor::task]
    async fn matrix_loop(mut matrix: Matrix) -> ! {
        matrix.scanner(&MyAction).await
    }

    pub struct MyAction;

    impl MatrixAction for MyAction {
        async fn handle_key(&self, event: bbq_keyboard::KeyEvent) {
            info!("Matrix key: {:?}", event);
        }
    }

    fn matrix_init(r: MatrixResources, side: Side) -> Matrix {
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.pin_2.degrade(),
                r.pin_3.degrade(),
                r.pin_4.degrade(),
                r.pin_5.degrade(),
            ]
            .map(|p| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.pin_6.degrade(),
                r.pin_26.degrade(),
                r.pin_7.degrade(),
                r.pin_27.degrade(),
                r.pin_29.degrade(),
                r.pin_28.degrade(),
            ]
            .map(|p| Input::new(p, Pull::Down)),
        );

        let xlate = translate::get_translation("jolt2");

        Matrix::new(cols, rows, xlate, side)
    }
}

mod jolt2dir {
    //! The jolt2dir is a variant on the jolt2, where instead the Pimotoni Tiny 2040, the rp2040 and
    //! support circuitry is all directly made onto the board.

    use bbq_keyboard::{KeyAction, Side};
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{Input, Level, Output, Pin, Pull}, peripherals, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peripherals};
    use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
    use embedded_resources::resource_group;
    use static_cell::StaticCell;

    use crate::{leds::{led_strip::{LedStripGroup, LedStripHandle}, LedSet}, matrix::Matrix, translate, Irqs};
    use crate::logging::unwrap;

    use super::{Board, UsbHandler};

    // Split up the peripherals.
    #[resource_group]
    struct MatrixResources {
        row0: peripherals::PIN_4,
        row1: peripherals::PIN_6,
        row2: peripherals::PIN_5,
        row3: peripherals::PIN_7,
        row4: peripherals::PIN_9,
        row5: peripherals::PIN_8,
        col0: peripherals::PIN_2,
        col1: peripherals::PIN_1,
        col2: peripherals::PIN_0,
        col3: peripherals::PIN_3,
    }

    #[resource_group]
    struct RgbResources {
        rgb_pin: peripherals::PIN_13,
        #[alias = RgbPIO]
        pio: peripherals::PIO0,
        dma: peripherals::DMA_CH0,
    }

    #[resource_group]
    struct UsbResources {
        usb: peripherals::USB,
    }

    pub fn new_left(p: Peripherals, spawner: SendSpawner, unique: &'static str) -> Board {
        let matrix = matrix_init(matrix_resources!(p), Side::Left);
        let leds = leds_init(rgb_resources!(p), spawner);

        let usb = usb_init(usb_resources!(p), spawner, unique);

        Board {
            matrix,
            leds,
            passive: None,
            active_keys: None,
            usb: Some(usb),
        }
    }

    // Testing/debugging of the matrix.
    #[embassy_executor::task]
    async fn matrix_loop(mut matrix: Matrix) -> ! {
        matrix.scanner(&super::jolt2::MyAction).await
    }

    fn matrix_init(r: MatrixResources, side: Side) -> Matrix {
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init(
            [
                r.col0.degrade(),
                r.col1.degrade(),
                r.col2.degrade(),
                r.col3.degrade(),
            ]
            .map(|p| Output::new(p, Level::Low)),
        );

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init(
            [
                r.row0.degrade(),
                r.row1.degrade(),
                r.row2.degrade(),
                r.row3.degrade(),
                r.row4.degrade(),
                r.row5.degrade(),
            ]
            .map(|p| Input::new(p, Pull::Down)),
        );

        let xlate = translate::get_translation("jolt2");

        Matrix::new(cols, rows, xlate, side)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio {
            mut common, sm0, ..
        } = Pio::new(r.pio, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma, r.rgb_pin, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        unwrap! {spawner.spawn(led_task(leds))};

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

        unwrap!(spawner.spawn(crate::usb::setup_usb(r.usb, unique, usb.keys.receiver())));

        usb
    }
}

/// Channel type for key event messages.
pub type KeyChannel = Receiver<'static, CriticalSectionRawMutex, KeyEvent, 1>;

pub struct UsbHandler {
    /// Channel for handling keys.  The USB task listens to this.
    pub keys: &'static Channel<CriticalSectionRawMutex, KeyAction, 8>,
}

/// The Initialized board.  Some here are optional, as the different parts are not used in all
/// configurations.
pub struct Board {
    /// The keyboard matrix.  Always present.
    pub matrix: Matrix,
    /// The leds, always present
    pub leds: LedSet,
    /// The passive handler, if that is the side we are on.
    pub passive: Option<InterPassive>,
    /// The channel where Matrix events will come from the other side.
    pub active_keys: Option<KeyChannel>,
    /// The communication channels with the USB tasks
    pub usb: Option<UsbHandler>,
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
                let mut this = jolt2::new_right(p, spawner);
                this.leds.update(&[RGB8::new(8, 8, 0)]);
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
            info => {
                panic!("Unsupported board: {:?}", info);
            }
        }
    }
}
