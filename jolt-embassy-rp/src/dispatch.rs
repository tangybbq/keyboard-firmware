//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use embassy_executor::SendSpawner;
use static_cell::StaticCell;

use crate::logging::{info, unwrap};
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

pub struct Dispatch {
}

impl Dispatch {
    pub fn new(spawn_high: SendSpawner, board: Board) -> &'static Dispatch {
        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
        });

        unwrap!(spawn_high.spawn(matrix_loop(this, board.matrix)));

        this
    }
}

#[embassy_executor::task]
async fn matrix_loop(dispatch: &'static Dispatch, mut matrix: Matrix) {
    matrix.scanner(dispatch).await;
}

impl MatrixAction for Dispatch {
    async fn handle_key(&self, event: bbq_keyboard::KeyEvent) {
        info!("Matrix Key: {:?}", event);
    }
}
