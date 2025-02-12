//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use bbq_keyboard::layout::{LayoutActions, LayoutManager};
use bbq_keyboard::{KeyAction, LayoutMode, MinorMode};
use bbq_steno::Stroke;
use embassy_executor::SendSpawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use static_cell::StaticCell;

use crate::leds::manager::{self, LedManager};
use crate::logging::{info, unwrap};
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

pub struct Dispatch {
    leds: Mutex<CriticalSectionRawMutex, LedManager>,
    layout: Mutex<CriticalSectionRawMutex, LayoutManager>,

    current_mode: Mutex<CriticalSectionRawMutex, LayoutMode>,
}

impl Dispatch {
    pub fn new(spawn_high: SendSpawner, board: Board) -> &'static Dispatch {
        let mut leds = LedManager::new(board.leds);

        // TODO: This is a workaround until usb is present.  Until either USB connects, or the left
        // side connects to us, just disable the global state.
        leds.clear_global(0);

        let leds = Mutex::new(leds);

        // Hard code the "two row" parameter.  This will need to come from the board to add support
        // for 2 row keyboards.
        let layout = Mutex::new(LayoutManager::new(false));

        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
            leds,
            layout,
            current_mode: Mutex::new(LayoutMode::Steno),
        });

        unwrap!(spawn_high.spawn(matrix_loop(this, board.matrix)));
        unwrap!(spawn_high.spawn(led_loop(&this.leds)));
        unwrap!(spawn_high.spawn(layout_loop(this)));

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

#[embassy_executor::task]
async fn layout_loop(dispatch: &'static Dispatch) -> ! {
    let mut ticker = Ticker::every(Duration::from_millis(10));
    loop {
        ticker.next().await;
        dispatch.layout.lock().await.tick(dispatch, 10).await;
    }
}

impl MatrixAction for Dispatch {
    async fn handle_key(&self, event: bbq_keyboard::KeyEvent) {
        self.layout.lock().await.handle_event(event, self).await
        // info!("Matrix Key: {:?}", event);
    }
}

impl LayoutActions for Dispatch {
    async fn set_mode(&self, mode: LayoutMode) {
        let next = match mode {
            LayoutMode::StenoDirect => todo!(),
            LayoutMode::Steno => &manager::STENO_DIRECT_INDICATOR,
            LayoutMode::Taipo => &manager::TAIPO_INDICATOR,
            LayoutMode::Qwerty => &manager::QWERTY_INDICATOR,
            _ => &manager::QWERTY_INDICATOR,
        };
        self.leds.lock().await.set_base(0, next);
        *self.current_mode.lock().await = mode;
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        let next = match mode {
            LayoutMode::StenoDirect => todo!(),
            LayoutMode::Steno => &manager::STENO_DIRECT_SELECT_INDICATOR,
            LayoutMode::Taipo => &manager::TAIPO_SELECT_INDICATOR,
            LayoutMode::Qwerty => &manager::QWERTY_SELECT_INDICATOR,
            _ => &manager::QWERTY_SELECT_INDICATOR,
        };
        self.leds.lock().await.set_base(0, next);
    }

    async fn send_key(&self, key: KeyAction) {
        info!("Key: {:?}", key);
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        let _ = submode;
        // At this point, this doesn't do anything.
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        info!("raw steno: {:?}", stroke);
    }
}
