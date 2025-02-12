//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use embassy_executor::SendSpawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use static_cell::StaticCell;

use crate::leds::manager::LedManager;
use crate::logging::{info, unwrap};
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

pub struct Dispatch {
    leds: Mutex<CriticalSectionRawMutex, LedManager>,
}

impl Dispatch {
    pub fn new(spawn_high: SendSpawner, board: Board) -> &'static Dispatch {
        let leds = Mutex::new(LedManager::new(board.leds));

        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
            leds,
        });

        unwrap!(spawn_high.spawn(matrix_loop(this, board.matrix)));
        unwrap!(spawn_high.spawn(led_loop(&this.leds)));

        this
    }
}

#[embassy_executor::task]
async fn led_loop(leds: &'static Mutex<CriticalSectionRawMutex, LedManager>) -> ! {
    let mut ticker = Ticker::every(Duration::from_millis(100));
    loop {
        ticker.next().await;
        leds.lock().await.tick();
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
