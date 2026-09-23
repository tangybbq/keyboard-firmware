//! Keyboard event dispatch.
//!
//! Dispatch is shared across the system via immutable reference, so data within will need to be
//! protected using Atomic or Mutexes.

use bbq_keyboard::layout::taipo::TaipoVariant;
use bbq_keyboard::layout::{LayoutActions, LayoutManager, TimedLayout};
#[cfg(feature = "steno")]
use bbq_keyboard::steno_delay::StenoDelay;
#[cfg(feature = "steno")]
use bbq_keyboard::usb_typer::{enqueue_action, ActionHandler};
#[cfg(feature = "steno")]
use bbq_keyboard::{Event, Keyboard};
use bbq_keyboard::{KeyAction, KeyEvent, LayoutMode, MinorMode, Mods};
#[cfg(feature = "steno")]
use bbq_steno::dict::Joined;
#[cfg(feature = "steno")]
use bbq_steno::Stroke;
use embassy_executor::SendSpawner;
use embassy_futures::select::{select, Either};
#[cfg(feature = "steno")]
use embassy_futures::select::{select3, Either3};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
#[cfg(feature = "steno")]
use embassy_sync::channel::{Receiver, Sender};
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Instant, Timer};
use minder::keylog::Marker;
use static_cell::StaticCell;

use crate::board::{Inter, KeyChannel, UsbHandler};
use crate::leds::manager::LedManager;
use crate::keylog;
use crate::logging::unwrap;
use crate::matrix::Matrix;
use crate::{board::Board, matrix::MatrixAction};

pub struct Dispatch {
    leds: Mutex<CriticalSectionRawMutex, LedManager>,
    layout: Option<Mutex<CriticalSectionRawMutex, TimedLayout>>,
    /// Tells `layout_loop` that a key event may have moved the layout's
    /// deadline.
    layout_wake: Signal<CriticalSectionRawMutex, ()>,
    inter: Inter,
    usb: Option<UsbHandler>,
    #[cfg(feature = "steno")]
    stroke_sender: Sender<'static, CriticalSectionRawMutex, Stroke, 10>,
    #[cfg(feature = "steno")]
    event_receiver: Receiver<'static, CriticalSectionRawMutex, Event, 16>,
    #[cfg(feature = "steno")]
    typed_receiver: Receiver<'static, CriticalSectionRawMutex, Joined, 2>,

    /// Asks `typed_loop` to type everything it still has buffered, right now.
    #[cfg(feature = "steno")]
    flush_signal: Signal<CriticalSectionRawMutex, ()>,
}

impl Dispatch {
    pub fn new(
        spawn_high: SendSpawner,
        board: Board,
        #[cfg(feature = "steno")] event_receiver: Receiver<'static, CriticalSectionRawMutex, Event, 16>,
        #[cfg(feature = "steno")] stroke_sender: Sender<'static, CriticalSectionRawMutex, Stroke, 10>,
        #[cfg(feature = "steno")] typed_receiver: Receiver<'static, CriticalSectionRawMutex, Joined, 2>,
    ) -> &'static Dispatch {
        // The LEDs come up dark, and stay that way until Taipo reports a
        // modifier being held.
        let leds = Mutex::new(LedManager::new(board.leds));

        // The layout is present, as long as we aren't the passive side.
        let layout = if board.inter.is_active() {
            Some(Mutex::new(TimedLayout::new(
                LayoutManager::new(board.two_row),
                Instant::now().as_millis(),
            )))
        } else {
            None
        };

        static THIS: StaticCell<Dispatch> = StaticCell::new();
        let this = THIS.init(Dispatch {
            leds,
            layout,
            layout_wake: Signal::new(),
            inter: board.inter,
            usb: board.usb,
            #[cfg(feature = "steno")]
            stroke_sender,
            #[cfg(feature = "steno")]
            event_receiver,
            #[cfg(feature = "steno")]
            typed_receiver,
            #[cfg(feature = "steno")]
            flush_signal: Signal::new(),
        });

        spawn_high.spawn(unwrap!(matrix_loop(this, board.matrix)));
        if this.layout.is_some() {
            spawn_high.spawn(unwrap!(layout_loop(this)));
            #[cfg(feature = "steno")]
            spawn_high.spawn(unwrap!(event_loop(this)));
            #[cfg(feature = "steno")]
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

/// Run the layout's timers.
///
/// The layout only needs time to pass while a timer is running, which is only
/// while a chord is being built, so this sleeps until the layout's deadline,
/// or indefinitely if it has none.  A key event can create, move or remove the
/// deadline, so every event signals `layout_wake`, and the loop throws away
/// what it was waiting on and asks again.  The deadline is only ever read under
/// the lock, never carried across a wait, so the loop can't act on an old one.
/// A spurious wake does no harm: the layout's timers only fire once their time
/// is up.
#[embassy_executor::task]
async fn layout_loop(dispatch: &'static Dispatch) -> ! {
    // The layout should always be set if we're runing.
    let layout = dispatch.layout.as_ref().unwrap();

    // Announces the initial mode.
    layout.lock().await.wake(Instant::now().as_millis(), dispatch).await;

    loop {
        let deadline = layout.lock().await.deadline();
        let due = async {
            match deadline {
                Some(deadline) => Timer::at(Instant::from_millis(deadline)).await,
                None => core::future::pending().await,
            }
        };

        match select(due, dispatch.layout_wake.wait()).await {
            Either::First(()) => {
                layout.lock().await.wake(Instant::now().as_millis(), dispatch).await;
            }
            // An event may have moved the deadline; go round and read it again.
            Either::Second(()) => (),
        }
    }
}

/// Legacy event loop handler.
///
/// Everything this used to do was an LED: the raw/cooked steno mode and the
/// dictionary's cap and space state each had an indicator, and the LEDs now
/// belong to the modifiers.  The events still have to be taken off the channel,
/// or the sender blocks.
#[cfg(feature = "steno")]
#[embassy_executor::task]
async fn event_loop(dispatch: &'static Dispatch) -> ! {
    loop {
        let _ = dispatch.event_receiver.receive().await;
    }
}

/// Event handler of steno actions.
///
/// Dictionary results are not typed as they arrive, but held in a [`StenoDelay`] buffer for a
/// short while, so that a following stroke's corrections can quietly consume text that hasn't
/// been typed yet.  See `bbq_keyboard::steno_delay`.
#[cfg(feature = "steno")]
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
#[cfg(feature = "steno")]
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
#[cfg(feature = "steno")]
struct UsbAction(&'static UsbHandler);

#[cfg(feature = "steno")]
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
        // The remote half's keys arrive here rather than through `handle_key`.
        keylog::log_key(event.key(), event.is_press());
        dispatch.refresh_leds(event).await;
        dispatch.layout_event(layout, event).await;
    }
}

impl Dispatch {
    /// Rewrite the LEDs on a key press, in case one has taken a bad bit.  See
    /// `LedManager::refresh`.
    async fn refresh_leds(&self, event: KeyEvent) {
        if event.is_press() {
            self.leds.lock().await.refresh();
        }
    }

    /// Give a key event to the layout, and let `layout_loop` know that its
    /// deadline may have moved.
    ///
    /// Every event has to go through here: an event that doesn't signal can
    /// leave `layout_loop` asleep past a chord's deadline.
    async fn layout_event(&self, layout: &Mutex<CriticalSectionRawMutex, TimedLayout>, event: KeyEvent) {
        layout
            .lock()
            .await
            .handle_event(event, Instant::now().as_millis(), self)
            .await;
        self.layout_wake.signal(());
    }
}

impl MatrixAction for Dispatch {
    async fn handle_key(&self, event: KeyEvent) {
        // info!("Matrix Key: {:?}", event);
        self.refresh_leds(event).await;
        if let Some(layout) = &self.layout {
            // Logged before the layout sees it, and timestamped here rather than in
            // `bbq-keyboard`, which stays time-free and no_std.  This is the local half; the
            // remote half's keys come through `active_task`.
            keylog::log_key(event.key(), event.is_press());
            self.layout_event(layout, event).await
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
        keylog::log_marker(Marker::Mode, mode.marker());

        self.leds.lock().await.set_mode(mode);

        // Steno output is buffered briefly before being typed.  Leaving steno mode, get it out
        // now, rather than having it appear in the middle of what is typed next.  Signalling with
        // nothing buffered is harmless.
        #[cfg(feature = "steno")]
        if mode != LayoutMode::Steno {
            self.flush_signal.signal(());
        }
    }

    /// Nothing to show.  Mode select flashed the mode LED, and there is no
    /// longer a mode LED to flash.
    async fn set_mode_select(&self, mode: LayoutMode) {
        let _ = mode;
    }

    async fn send_key(&self, key: KeyAction) {
        // info!("Key: {:?}", key);
        self.usb.as_ref().unwrap().keys.send(key).await;
    }

    async fn set_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Dosh => {
                keylog::log_marker(Marker::Variant, TaipoVariant::Dosh.marker());
            }
        }
    }

    async fn clear_sub_mode(&self, submode: MinorMode) {
        match submode {
            MinorMode::Dosh => {
                keylog::log_marker(Marker::Variant, TaipoVariant::Taipo.marker());
            }
        }
    }

    #[cfg(feature = "steno")]
    async fn send_raw_steno(&self, stroke: Stroke) {
        self.stroke_sender.send(stroke).await;
    }

    async fn set_row_position(&self, lower: bool) {
        keylog::log_marker(Marker::RowShift, lower as u8);
    }

    async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
        self.leds.lock().await.set_mods(oneshot, sticky);
    }
}

// Wrapper around Dispatch because the usb typer wants.
