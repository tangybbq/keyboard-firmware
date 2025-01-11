//! Keyboard event dispatch
//!
//! In transitioning from having a single large event enum to have actions generally tend to
//! directly call their handlers, this Dispatch type handles the notion of global state (which
//! affects where some messages go), as well, as implementing the handlers (with simple wrappers for
//! the Mutex).
//!
//! When something needs to be done, it should be done directly if that if possible.
//!
//! Dispatch will be shared, via `Arc<Dispatch>`.
//!
//! The various handlers will be held within Dispatch, protected as appropriate for the priorities
//! throughout the system.  For example, when a task only communicates to itself or lower priority
//! threads, it can directly aquire some kind of mutex and call the handler.  But, if a lower
//! priority task needs to have something handled by a higher priority, we will need a channel to
//! hold the message.
//!
//! As such, Dispatch will generally have no methods that take `&mut self`, and will instead use
//! internal locking as appropriate.
//!
//! As Dispatch holds the handles for the worker threads, it must not be dropped.  Main can exit,
//! but must leak a reference to the Dispatch to prevent it from being freed.

use core::{ffi::c_int, slice};

use alloc::vec::Vec;
use bbq_keyboard::{dict::Dict, layout::LayoutActions, usb_typer::{enqueue_action, ActionHandler}, Event, KeyAction, Keyboard, LayoutMode, MinorMode, Mods};
use bbq_steno::{dict::Joined, Stroke};
use log::{info, warn};
use zephyr::{
    kio::{self, sync::Mutex}, kobj_define, printkln,
    sync::{
        channel::{self, Receiver, Sender},
        Arc, SpinMutex,
    },
    sys::sync::Semaphore,
    time::{self, Duration},
    work::{WorkQueue, WorkQueueBuilder},
};

use crate::{devices::usb::Usb, get_steno_indicator, get_steno_select_indicator, leds::manager::{self, LedManager}, SendWrap, WrapTimer};

/// Priority of main work queue.
const MAIN_PRIORITY: c_int = 2;

/// The Steno thread runs at the lowest priority as these lookups can often take dozens of ms.
const STENO_PRIORITY: c_int = 5;

/// For initialization, the main thread will build this struct, and invoke 'build'.  The use of
/// build is mainly to avoid having a large number of unnamed arguments.
pub struct DispatchBuilder {
    /// For now, pass in a sender for the main event queue.
    ///
    /// This should eventually go away.
    pub equeue_send: Sender<Event>,

    /// The USB manager.
    pub usb: Usb,

    /// The LED manager.
    pub leds: LedManager,
}

impl DispatchBuilder {
    /// Create the Dispatch.
    ///
    /// This constructor intentionally leaks a reference to this so that it will never be freed.  It
    /// contains WorkQueues, which can never be dropped.
    pub fn build(self) -> Arc<Dispatch> {
        let this = Dispatch::build(self);
        let _ = Arc::into_raw(this.clone());
        this
    }
}

/// The main Dispatch for the keyboard.
pub struct Dispatch {
    /// The main work queue.
    ///
    /// Everything that is "fast" will be run within this thread.  Fast generally means within a few
    /// hundred us.  If something is slow enough to prevent things on main that need to run within a
    /// 1ms tick, they should be moved to another, lower priority thread.
    // TODO: shouldn't be pub, but needs to be as we transition to this.
    pub main_worker: WorkQueue,

    /// The steno lookup thread.
    ///
    /// This is a computational worker, and and runs lower priority than most other threads.
    pub steno_worker: WorkQueue,

    /// Work to be sent to the steno worker.
    steno_send: Sender<Stroke>,

    /// The main event queue.
    ///
    /// For transition, this queue still exists, but over time, various things that do need queues
    /// should use more specific types, or, when running on the same worker, just call what needs
    /// the work.
    pub equeue_send: Sender<Event>,

    /// Mode and raw mode.
    ///
    /// For transition, accessed outside.
    pub raw_mode: SpinMutex<bool>,
    pub current_mode: SpinMutex<LayoutMode>,

    /// The USB handler.
    usb: Usb,

    /// The LED manager.
    ///
    /// TODO: pub is for transition.
    pub leds: Mutex<LedManager>,
}

impl Dispatch {
    /// Build a Dispatch out of the builder.
    fn build(builder: DispatchBuilder) -> Arc<Dispatch> {
        let main_worker = WorkQueueBuilder::new()
            .set_priority(MAIN_PRIORITY)
            .set_name(c"wq:main")
            .set_no_yield(MAIN_PRIORITY >= 0)
            .start(MAIN_LOOP_STACK.init_once(()).unwrap());

        let steno_worker = WorkQueueBuilder::new()
            .set_priority(STENO_PRIORITY)
            .set_name(c"qc:steno")
            .set_no_yield(STENO_PRIORITY >= 0)
            .start(STENO_STACK.init_once(()).unwrap());

        let (steno_send, steno_recv) = channel::bounded(10);
        let (stenotype_send, stenotype_recv) = channel::unbounded();

        let this = Arc::new(Dispatch {
            main_worker,
            steno_worker,
            steno_send,
            equeue_send: builder.equeue_send,
            usb: builder.usb,
            leds: Mutex::new(builder.leds),
            raw_mode: SpinMutex::new(false),
            current_mode: SpinMutex::new(LayoutMode::Steno),
        });

        // Fire off the steno main thread.
        let this2 = this.clone();
        let _ = kio::spawn(
            async {
                kio::spawn_local(Self::steno_main(this2, steno_recv, stenotype_send), c"w:steno");
            },
            &this.main_worker,
            c"w:steno-start",
        );

        // And a small thread to receive the events back, and enqueue them.  This small queue is
        // needed to avoid priority inversion problems with the low priority steno worker holding
        // the usb hid lock.
        let this2 = this.clone();
        let _ = kio::spawn(Self::steno_typer(this2, stenotype_recv), &this.main_worker, c"w:stenotype");

        let _ = kio::spawn(
            Self::loop_1ms(this.clone()),
            &this.main_worker,
            c"w:1ms_loop",
        );

        // We need to hold onto the various workers, but don't want them to be visible.  This
        // reference will prevent warnings about them not being used.
        let _ = &this.steno_worker;

        this
    }

    /// Pass a stroke along to the steno worker.
    pub fn translate_steno(&self, stroke: Stroke) {
        self.steno_send.try_send(stroke).unwrap();
    }

    /// Receive the translations back from the steno worker.
    async fn steno_typer(this: Arc<Self>, typed: Receiver<Joined>) {
        while let Ok(action) = typed.recv_async().await {
            match action {
                Joined::Type { remove, append } => {
                    for _ in 0..remove {
                        this.usb_hid_push(KeyAction::KeyPress(
                                Keyboard::DeleteBackspace,
                                Mods::empty()))
                            .await;
                        this.usb_hid_push(KeyAction::KeyRelease).await;
                    }
                    enqueue_action(&mut KeyActionWrap(&this), &append).await;
                }
            }
        }
        panic!("Steno typer exited");
    }

    /// The main task on the steno thread.
    ///
    /// This loops forever, receiving strokes, processing them, and sending them back as 'StenoText'
    /// events.  Eventually, this should be dispatching USB events directly.
    async fn steno_main(this: Arc<Self>, strokes: Receiver<Stroke>, typed: Sender<Joined>) {
        printkln!("Steno thread running");
        let mut eq_send = SendWrap(this.equeue_send.clone());
        let mut dict = Dict::new();
        loop {
            let stroke = strokes.recv_async().await.unwrap();
            for action in dict.handle_stroke(stroke, &mut eq_send, &WrapTimer) {
                typed.send(action).unwrap();
            }
        }
    }

    /// Push USB-hid events to the USB stack.
    pub async fn usb_hid_push(&self, key: KeyAction) {
        match key {
            KeyAction::KeyPress(code, mods) => {
                let code = code as u8;
                self.usb
                    .send_keyboard_report(mods.bits(), slice::from_ref(&code))
                    .await;
            }
            KeyAction::KeyRelease => {
                self.usb.send_keyboard_report(0, &[]).await;
            }
            KeyAction::KeySet(keys) => {
                // TODO We don't handle more than 6 keys, which qwerty mode can do.  For now, just
                // report if we can.
                let (mods, keys) = keyset_to_hid(keys);
                self.usb.send_keyboard_report(mods.bits(), &keys).await;
            }
            KeyAction::ModOnly(mods) => {
                self.usb.send_keyboard_report(mods.bits(), &[]).await;
            }
            KeyAction::Stall => (),
        }
    }

    /// Send a report over the plover protocol.  Or at least attempt to.
    pub fn send_plover_report(&self, report: &[u8]) {
        // TODO: This seems to block and should become async.
        self.usb.send_plover_report(report);
    }

    /// Once a ms loop.  This runs every 1ms, performing various tasks.
    async fn loop_1ms(this: Arc<Self>) {
        // TODO: Need to implement sleep that works with an Instant instead of just a duration.
        // As a workaround, we'll just make a semaphore that will never be available.
        let never = Semaphore::new(0, 1).unwrap();
        let period = Duration::millis_at_least(1);
        let mut next = time::now() + period;
        loop {
            let _ = never.take_async(next).await;

            // Read a USB keyboard report.
            let mut buf = [0u8; 8];
            if let Ok(Some(count)) = this.usb.get_keyboard_report(&mut buf) {
                info!("Keyboard out: {} bytes: {:?}", count, &buf[..count]);
            }

            next += period;
            let now = time::now();
            if next < now {
                warn!("Periodic 1m overflow: {} ticks", (now - next).ticks());
                next = now + period;
            }
        }
    }
}

impl LayoutActions for Dispatch {
    async fn set_mode(&self, mode: LayoutMode) {
        info!("mode: {:?}", mode);
        let next = match mode {
            LayoutMode::Steno => get_steno_indicator(*self.raw_mode.lock().unwrap()),
            LayoutMode::StenoDirect => &manager::STENO_DIRECT_INDICATOR,
            LayoutMode::Taipo => &manager::TAIPO_INDICATOR,
            LayoutMode::Qwerty => &manager::QWERTY_INDICATOR,
            _ => &manager::QWERTY_INDICATOR,
        };
        self.leds.lock().unwrap().set_base(0, next);
        *self.current_mode.lock().unwrap() = mode;
    }

    async fn set_mode_select(&self, mode: LayoutMode) {
        let next = match mode {
            LayoutMode::Steno => get_steno_select_indicator(*self.raw_mode.lock().unwrap()),
            LayoutMode::StenoDirect => &manager::STENO_DIRECT_SELECT_INDICATOR,
            LayoutMode::Taipo => &manager::TAIPO_SELECT_INDICATOR,
            LayoutMode::Qwerty => &manager::QWERTY_SELECT_INDICATOR,
            _ => &manager::QWERTY_SELECT_INDICATOR,
        };
        self.leds.lock().unwrap().set_base(0, next);
    }

    async fn send_key(&self, key: KeyAction) {
        self.usb_hid_push(key).await
    }

    async fn set_sub_mode(&self, _submode: MinorMode) {
        // At this point, this doesn't do anything.
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        if *self.current_mode.lock().unwrap() == LayoutMode::Steno {
            self.translate_steno(stroke);
        } else {
            // TODO: Restore gemini
            self.send_plover_report(&stroke.to_plover_hid());
            self.send_plover_report(&Stroke::empty().to_plover_hid());
        }
    }
}

// Qwerty mode just sends scan codes, but not the mod bits as expected by the HID layer.  To fix
// this, convert the codes from QWERTY into a proper formatted data for a report.
fn keyset_to_hid(keys: Vec<Keyboard>) -> (Mods, Vec<u8>) {
    let mut result = Vec::new();
    let mut mods = Mods::empty();
    for key in keys {
        match key {
            Keyboard::LeftControl => mods |= Mods::CONTROL,
            Keyboard::LeftShift => mods |= Mods::SHIFT,
            Keyboard::LeftAlt => mods |= Mods::ALT,
            Keyboard::LeftGUI => mods |= Mods::GUI,
            key => result.push(key as u8),
        }
    }
    (mods, result)
}

struct KeyActionWrap<'a>(&'a Dispatch);

impl<'a> ActionHandler for KeyActionWrap<'a> {
    async fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for act in events {
            self.0.usb_hid_push(act).await;
        }
    }
}

kobj_define! {
    // The main loop thread's stack.
    static MAIN_LOOP_STACK: ThreadStack<2048>;

    // The steno thread.
    static STENO_STACK: ThreadStack<4096>;
}
