//! Board-specific initialization.
//!
//! This module initializes all of the various hardware devices used by the keyboard firmware, as
//! appropriate for the board information we have determined.

use bbq_keyboard::{boardinfo::BoardInfo, Side};
use embassy_executor::SendSpawner;
use embassy_rp::Peripherals;
use smart_leds::RGB8;

use crate::{leds::LedSet, matrix::Matrix};

// Board specific for the jolt3.
mod jolt3 {
    use assign_resources::assign_resources;
    use embassy_executor::SendSpawner;
    use embassy_rp::{gpio::{Input, Level, Output, Pin, Pull}, peripherals::{self, PIO0}, pio::Pio, pio_programs::ws2812::{PioWs2812, PioWs2812Program}, Peripherals};
    use static_cell::StaticCell;

    use crate::{leds::{led_strip::{LedStripGroup, LedStripHandle}, LedSet}, matrix::Matrix, translate, Irqs};
    use crate::logging::unwrap;

    use super::Board;

    // Split up the periperals for each init.
    assign_resources! {
        matrix: MatrixResources {
            pin_0: PIN_0,
            pin_1: PIN_1,
            pin_2: PIN_2,
            pin_3: PIN_3,
            pin_4: PIN_4,
            pin_5: PIN_5,
            pin_6: PIN_6,
            pin_7: PIN_7,
            pin_8: PIN_8,
            pin_9: PIN_9,
        }
        rgb: RgbResources {
            pin_19: PIN_19,
            pio0: PIO0,
            dma_ch0: DMA_CH0,
        }
    }

    pub fn new_left(p: Peripherals, spawner: SendSpawner) -> Board {
        let r = split_resources!(p);

        let matrix = matrix_init(r.matrix);
        let leds = leds_init(r.rgb, spawner);

        Board { matrix, leds }
    }

    pub fn new_right(p: Peripherals, spawner: SendSpawner) -> Board {
        let r = split_resources!(p);

        let matrix = matrix_init(r.matrix);
        let leds = leds_init(r.rgb, spawner);

        Board { matrix, leds }
    }

    fn matrix_init(r: MatrixResources) -> Matrix {
        // The keyboard matrix.
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init([
            r.pin_6.degrade(),
            r.pin_7.degrade(),
            r.pin_8.degrade(),
            r.pin_9.degrade(),
        ]
        .map(|p| Output::new(p, Level::Low)));

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init([
            r.pin_0.degrade(),
            r.pin_2.degrade(),
            r.pin_1.degrade(),
            r.pin_3.degrade(),
            r.pin_5.degrade(),
            r.pin_4.degrade(),
        ]
        .map(|p| Input::new(p, Pull::Down)));

        let xlate = translate::get_translation("jolt3");

        Matrix::new(cols, rows, xlate)
    }

    fn leds_init(r: RgbResources, spawner: SendSpawner) -> LedSet {
        // The PIO and DMA are used for the LED driver.
        let Pio { mut common, sm0, .. } = Pio::new(r.pio0, Irqs);
        let program = PioWs2812Program::new(&mut common);
        let ws2812 = PioWs2812::new(&mut common, sm0, r.dma_ch0, r.pin_19, &program);

        let leds = LedStripGroup::new(ws2812);

        static STRIP: StaticCell<LedStripHandle> = StaticCell::new();
        let strip = STRIP.init(leds.get_handle());
        unwrap!{spawner.spawn(led_task(leds))};

        LedSet::new([strip])
    }

    #[embassy_executor::task]
    async fn led_task(leds: LedStripGroup<'static, PIO0, 0, 2>) {
        leds.update_task().await;
    }
}

/// The Initialized board.  Some here are optional, as the different parts are not used in all
/// configurations.
pub struct Board {
    /// The keyboard matrix.  Always present.
    pub matrix: Matrix,
    /// The leds, always present
    pub leds: LedSet,
}

impl Board {
    pub fn new(p: Peripherals, spawner: SendSpawner, info: &BoardInfo) -> Board {
        match info {
            BoardInfo { name, side: Some(Side::Left) } if name == "jolt3" => {
                let mut this = jolt3::new_left(p, spawner);
                this.leds.update(&[RGB8::new(0, 8, 8), RGB8::new(8, 8, 0)]);
                this
            }
            BoardInfo { name, side: Some(Side::Right) } if name == "jolt3" => {
                let mut this = jolt3::new_right(p, spawner);
                this.leds.update(&[RGB8::new(0, 8, 8), RGB8::new(8, 8, 0)]);
                this
            }
            info => {
                panic!("Unsupported board: {:?}", info);
            }
        }
    }
}
