//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use bbq_keyboard::layout::{LayoutActions, LayoutManager};
use bbq_keyboard::usb_typer::{enqueue_action, ActionHandler};
use bbq_keyboard::{Event, KeyAction, Keyboard, LayoutMode, MinorMode, Mods};
use bbq_steno::dict::Joined;
use bbq_steno::Stroke;
use embassy_executor::SendSpawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use static_cell::StaticCell;

use crate::board::{KeyChannel, UsbHandler};
use crate::inter::InterPassive;
use crate::leds::manager::{self, Indication, LedManager};
use crate::logging::unwrap;
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

pub struct Dispatch {
    leds: Mutex<CriticalSectionRawMutex, LedManager>,
    layout: Option<Mutex<CriticalSectionRawMutex, LayoutManager>>,
    passive: Option<InterPassive>,
    usb: Option<UsbHandler>,
    stroke_sender: Sender<'static, CriticalSectionRawMutex, Stroke, 10>,
    event_receiver: Receiver<'static, CriticalSectionRawMutex, Event, 16>,
    typed_receiver: Receiver<'static, CriticalSectionRawMutex, Joined, 2>,

    current_mode: Mutex<CriticalSectionRawMutex, LayoutMode>,
    raw_mode: Mutex<CriticalSectionRawMutex, bool>,
}

impl Dispatch {
    pub fn new(
        spawn_high: SendSpawner,
        board: Board,
        event_receiver: Receiver<'static, CriticalSectionRawMutex, Event, 16>,
        stroke_sender: Sender<'static, CriticalSectionRawMutex, Stroke, 10>,
        typed_receiver: Receiver<'static, CriticalSectionRawMutex, Joined, 2>,
    ) -> &'static Dispatch {
        let mut leds = LedManager::new(board.leds);

        // TODO: This is a workaround until usb is present.  Until either USB connects, or the left
        // side connects to us, just disable the global state.
        leds.clear_global(0);

        let leds = Mutex::new(leds);

        // Hard code the "two row" parameter.  This will need to come from the board to add support
        // for 2 row keyboards.
        // The layout is present, as long as we aren't the passive side.
        let layout = if board.passive.is_none() {
            Some(Mutex::new(LayoutManager::new(false)))
        } else {
            None
        };

        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
            leds,
            layout,
            current_mode: Mutex::new(LayoutMode::Steno),
            raw_mode: Mutex::new(false),
            passive: board.passive,
            usb: board.usb,
            stroke_sender,
            event_receiver,
            typed_receiver,
        });

        unwrap!(spawn_high.spawn(matrix_loop(this, board.matrix)));
        unwrap!(spawn_high.spawn(led_loop(&this.leds)));
        if this.layout.is_some() {
            unwrap!(spawn_high.spawn(layout_loop(this)));
            unwrap!(spawn_high.spawn(event_loop(this)));
            unwrap!(spawn_high.spawn(typed_loop(this)));
        }
        if let Some(chan) = board.active_keys {
            unwrap!(spawn_high.spawn(active_task(this, chan)));
        }

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
    // The layout should always be set if we're runing.
    let layout = dispatch.layout.as_ref().unwrap();
    loop {
        ticker.next().await;
        layout.lock().await.tick(dispatch, 10).await;
    }
}

/// Legacy event loop handler.
#[embassy_executor::task]
async fn event_loop(dispatch: &'static Dispatch) -> ! {
    loop {
        let event = dispatch.event_receiver.receive().await;
        match event {
            Event::RawMode(raw) => {
                if *dispatch.current_mode.lock().await == LayoutMode::Steno {
                    *dispatch.raw_mode.lock().await = raw;
                    dispatch.leds.lock().await.set_base(0, get_steno_indicator(raw));
                }
            },
            _ => (),
        }
        // TODO: This brings in fmt, but this Event type should be going away soon anyway.
        // info!("Steno event: {}", &format!("{:?}", event));
    }
}

/// Event handler of steno actions.
#[embassy_executor::task]
async fn typed_loop(dispatch: &'static Dispatch) -> ! {
    let usb = &dispatch.usb.as_ref().unwrap();
    loop {
        match dispatch.typed_receiver.receive().await {
            Joined::Type { remove, append } => {
                for _ in 0..remove {
                    usb.keys.send(KeyAction::KeyPress(
                            Keyboard::DeleteBackspace,
                            Mods::empty())).await;
                    usb.keys.send(KeyAction::KeyRelease).await;
                }

                enqueue_action(&mut UsbAction(usb), &append).await;
            }
        }
    }
}

// The Actionhandler wants a mut ref, so give it one.
struct UsbAction(&'static UsbHandler);

impl ActionHandler for UsbAction {
    async fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for ev in events {
            // info!("USB send: {:?}", ev);
            self.0.keys.send(ev).await;
        }
    }
}

#[embassy_executor::task]
async fn active_task(dispatch: &'static Dispatch, chan: KeyChannel) -> ! {
    // The layout should always be set if we're running.
    let layout = dispatch.layout.as_ref().unwrap();
    loop {
        let event = chan.receive().await;
        layout.lock().await.handle_event(event, dispatch).await;
    }
}

impl MatrixAction for Dispatch {
    async fn handle_key(&self, event: bbq_keyboard::KeyEvent) {
        // info!("Matrix Key: {:?}", event);
        if let Some(layout) = &self.layout {
            layout.lock().await.handle_event(event, self).await
        } else if let Some(passive) = &self.passive {
            passive.update(event).await;
        } else {
            panic!("Matrix event with no destination");
        }
    }
}

impl LayoutActions for Dispatch {
    async fn set_mode(&self, mode: LayoutMode) {
        let next = match mode {
            LayoutMode::StenoDirect => todo!(),
            LayoutMode::Steno => get_steno_indicator(*self.raw_mode.lock().await),
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
            LayoutMode::Steno => get_steno_select_indicator(*self.raw_mode.lock().await),
            LayoutMode::Taipo => &manager::TAIPO_SELECT_INDICATOR,
            LayoutMode::Qwerty => &manager::QWERTY_SELECT_INDICATOR,
            _ => &manager::QWERTY_SELECT_INDICATOR,
        };
        self.leds.lock().await.set_base(0, next);
    }

    async fn send_key(&self, key: KeyAction) {
        // info!("Key: {:?}", key);
        self.usb.as_ref().unwrap().keys.send(key).await;
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        let _ = submode;
        // At this point, this doesn't do anything.
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        self.stroke_sender.send(stroke).await;
    }
}

fn get_steno_indicator(raw: bool) -> &'static Indication {
    if raw {
        &crate::leds::manager::STENO_RAW_INDICATOR
    } else {
        &crate::leds::manager::STENO_INDICATOR
    }
}

fn get_steno_select_indicator(raw: bool) -> &'static Indication {
    if raw {
        &crate::leds::manager::STENO_RAW_SELECT_INDICATOR
    } else {
        &crate::leds::manager::STENO_SELECT_INDICATOR
    }
}

// Wrapper around Dispatch because the usb typer wants.
