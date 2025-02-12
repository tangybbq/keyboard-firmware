//! Board-specific initialization.
//!
//! This module initializes all of the various hardware devices used by the keyboard firmware, as
//! appropriate for the board information we have determined.

use bbq_keyboard::{boardinfo::BoardInfo, Side};
use defmt::panic;
use embassy_rp::{gpio::{Input, Level, Output, Pin, Pull}, Peripherals};
use static_cell::StaticCell;

use crate::{matrix::Matrix, translate};

/// The Initialized board.  Some here are optional, as the different parts are not used in all
/// configurations.
pub struct Board {
    /// The keyboard matrix.  Always present.
    pub matrix: Matrix,
}

impl Board {
    pub fn new(p: Peripherals, info: &BoardInfo) -> Board {
        match info {
            BoardInfo { name, side: Some(Side::Left) } if name == "jolt3" => {
                Self::new_jolt3_left(p)
            }
            BoardInfo { name, side: Some(Side::Right) } if name == "jolt3" => {
                Self::new_jolt3_right(p)
            }
            info => {
                panic!("Unsupported board: {:?}", info);
            }
        }
    }

    fn new_jolt3_left(p: Peripherals) -> Board {
        // The keyboard matrix.
        static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
        let cols = COLS.init([
            p.PIN_6.degrade(),
            p.PIN_7.degrade(),
            p.PIN_8.degrade(),
            p.PIN_9.degrade(),
        ]
        .map(|p| Output::new(p, Level::Low)));

        static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
        let rows = ROWS.init([
            p.PIN_0.degrade(),
            p.PIN_2.degrade(),
            p.PIN_1.degrade(),
            p.PIN_3.degrade(),
            p.PIN_5.degrade(),
            p.PIN_4.degrade(),
        ]
        .map(|p| Input::new(p, Pull::Down)));

        let xlate = translate::get_translation("jolt3");

        let matrix = Matrix::new(cols, rows, xlate);

        Board { matrix }
    }

    fn new_jolt3_right(p: Peripherals) -> Board {
        let _ = p;
        todo!()
    }
}
