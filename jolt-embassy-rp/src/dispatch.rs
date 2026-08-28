//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use bbq_keyboard::layout::{LayoutActions, LayoutManager};
use bbq_keyboard::steno_delay::StenoDelay;
use bbq_keyboard::usb_typer::{enqueue_action, ActionHandler};
use bbq_keyboard::{Event, KeyAction, Keyboard, LayoutMode, MinorMode, Mods};
use bbq_steno::dict::Joined;
use bbq_steno::Stroke;
use embassy_executor::SendSpawner;
use embassy_futures::select::{select3, Either3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Ticker, Timer};
use static_cell::StaticCell;

use crate::board::{Inter, KeyChannel, UsbHandler};
use crate::leds::manager::{self, get_mods_color, get_steno_state, Indication, LedManager};
use crate::logging::unwrap;
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

/// The LED showing the Taipo modifier state.
///
/// This is the 4th LED, which only some boards have; the manager ignores
/// updates to LEDs that aren't there.
const MODS_LED: usize = 3;

/// The LED showing which chord table the Taipo engine is using.
///
/// This is the 3rd LED, which only some boards have, and which is dark outside
/// of Taipo mode.
const VARIANT_LED: usize = 2;

pub struct Dispatch {
    leds: Mutex<CriticalSectionRawMutex, LedManager>,
    layout: Option<Mutex<CriticalSectionRawMutex, LayoutManager>>,
    inter: Inter,
    usb: Option<UsbHandler>,
    stroke_sender: Sender<'static, CriticalSectionRawMutex, Stroke, 10>,
    event_receiver: Receiver<'static, CriticalSectionRawMutex, Event, 16>,
    typed_receiver: Receiver<'static, CriticalSectionRawMutex, Joined, 2>,

    /// Asks `typed_loop` to type everything it still has buffered, right now.
    flush_signal: Signal<CriticalSectionRawMutex, ()>,

    current_mode: Mutex<CriticalSectionRawMutex, LayoutMode>,
    raw_mode: Mutex<CriticalSectionRawMutex, bool>,

    /// Whether the Taipo engine has the Posh chord table selected.  Only
    /// meaningful in Taipo mode, but the engine keeps the setting across mode
    /// changes, so this does too.
    posh: Mutex<CriticalSectionRawMutex, bool>,
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

        // The modifier indicator is dark until Taipo reports a modifier being held.
        leds.set_base(MODS_LED, &manager::OFF_INDICATOR);

        // The variant indicator is dark until we enter Taipo mode.
        leds.set_base(VARIANT_LED, &manager::OFF_INDICATOR);

        let leds = Mutex::new(leds);

        // The layout is present, as long as we aren't the passive side.
        let layout = if board.inter.is_active() {
            Some(Mutex::new(LayoutManager::new(board.two_row)))
        } else {
            None
        };

        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
            leds,
            layout,
            current_mode: Mutex::new(LayoutMode::Steno),
            raw_mode: Mutex::new(false),
            posh: Mutex::new(false),
            inter: board.inter,
            usb: board.usb,
            stroke_sender,
            event_receiver,
            typed_receiver,
            flush_signal: Signal::new(),
        });

        spawn_high.spawn(unwrap!(matrix_loop(this, board.matrix)));
        spawn_high.spawn(unwrap!(led_loop(&this.leds)));
        if this.layout.is_some() {
            spawn_high.spawn(unwrap!(layout_loop(this)));
            spawn_high.spawn(unwrap!(event_loop(this)));
            spawn_high.spawn(unwrap!(typed_loop(this)));
        }
        if let Inter::ActiveI2C(chan) = this.inter {
            spawn_high.spawn(unwrap!(active_task(this, chan)));
        }
        if let Inter::ActiveUart(active_uart) = this.inter {
            spawn_high.spawn(unwrap!(active_uart_task(this, active_uart)));
        }

        this
    }

    /// Update the Taipo variant indicator to match the current mode and
    /// variant.  It is dark outside of Taipo mode, since the variant means
    /// nothing there.
    ///
    /// The mutexes are taken one after another, never nested.
    async fn update_variant_led(&self) {
        let mode = *self.current_mode.lock().await;
        let posh = *self.posh.lock().await;

        let next = if mode != LayoutMode::Taipo {
            &manager::OFF_INDICATOR
        } else if posh {
            &manager::VARIANT_POSH_INDICATOR
        } else {
            &manager::VARIANT_TAIPO_INDICATOR
        };
        self.leds.lock().await.set_base(VARIANT_LED, next);
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
async fn active_uart_task(dispatch: &'static Dispatch, act: &'static crate::inter_uart::InterActive) -> ! {
    loop {
        let event = act.get_key().await;
        dispatch.handle_key(event).await;
    }
}

#[embassy_executor::task]
async fn layout_loop(dispatch: &'static Dispatch) -> ! {
    // The layout timeouts are all in milliseconds, so tick at that rate to keep
    // them from being quantized.
    let mut ticker = Ticker::every(Duration::from_millis(1));
    // The layout should always be set if we're runing.
    let layout = dispatch.layout.as_ref().unwrap();
    loop {
        ticker.next().await;
        layout.lock().await.tick(dispatch, 1).await;
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
            Event::StenoState(state) => {
                dispatch.leds.lock().await.set_base(1, get_steno_state(&state));
            }
            _ => (),
        }
        // TODO: This brings in fmt, but this Event type should be going away soon anyway.
        // info!("Steno event: {}", &format!("{:?}", event));
    }
}

/// Event handler of steno actions.
///
/// Dictionary results are not typed as they arrive, but held in a [`StenoDelay`] buffer for a
/// short while, so that a following stroke's corrections can quietly consume text that hasn't
/// been typed yet.  See `bbq_keyboard::steno_delay`.
#[embassy_executor::task]
async fn typed_loop(dispatch: &'static Dispatch) -> ! {
    let usb = dispatch.usb.as_ref().unwrap();
    let mut delay = StenoDelay::new();
    loop {
        // Wait until the buffer's front entry is due.  With nothing buffered, there is no
        // deadline, and only a new result or a flush can wake us.
        let due = async {
            match delay.next_deadline() {
                Some(deadline) => Timer::at(Instant::from_millis(deadline)).await,
                None => core::future::pending().await,
            }
        };

        match select3(
            dispatch.typed_receiver.receive(),
            due,
            dispatch.flush_signal.wait(),
        )
        .await
        {
            Either3::First(action) => delay.push(action, Instant::now().as_millis()),
            Either3::Second(()) => {
                if let Some(action) = delay.take_ready(Instant::now().as_millis()) {
                    type_action(usb, action).await;
                }
            }
            // Leaving steno mode: don't leave text sitting in the buffer while the user types
            // with another layout.
            Either3::Third(()) => {
                if let Some(action) = delay.take_all() {
                    type_action(usb, action).await;
                }
            }
        }
    }
}

/// Send a dictionary result to the host as USB key events.
async fn type_action(usb: &'static UsbHandler, action: Joined) {
    let Joined::Type { remove, append } = action;

    for _ in 0..remove {
        usb.keys
            .send(KeyAction::KeyPress(
                Keyboard::DeleteBackspace,
                Mods::empty(),
            ))
            .await;
        usb.keys.send(KeyAction::KeyRelease).await;
    }

    enqueue_action(&mut UsbAction(usb), &append).await;
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
        } else if let Inter::PassiveI2C(passive) = &self.inter {
            passive.update(event).await;
        } else if let Inter::PassiveUart(passive_uart) = &self.inter {
            passive_uart.update_keys(event).await;
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

        // Steno output is buffered briefly before being typed.  Leaving steno mode, get it out
        // now, rather than having it appear in the middle of what is typed next.  Signalling with
        // nothing buffered is harmless.
        if mode != LayoutMode::Steno {
            self.flush_signal.signal(());
        }

        self.update_variant_led().await;
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
        match submode {
            MinorMode::Posh => {
                *self.posh.lock().await = true;
                self.update_variant_led().await;
            }
        }
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Posh => {
                *self.posh.lock().await = false;
                self.update_variant_led().await;
            }
        }
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        self.stroke_sender.send(stroke).await;
    }

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        let color = get_mods_color(oneshot, sticky);
        self.leds.lock().await.set_solid(MODS_LED, Some(color));
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
