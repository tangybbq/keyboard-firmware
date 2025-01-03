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

use core::ffi::c_int;

use bbq_keyboard::{dict::Dict, Event};
use bbq_steno::Stroke;
use zephyr::{kio, kobj_define, printkln, sync::{channel::{self, Receiver, Sender}, Arc}, work::{WorkQueue, WorkQueueBuilder}};

use crate::{SendWrap, WrapTimer};

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

        let this = Arc::new(Dispatch {
            main_worker,
            steno_worker,
            steno_send,
            equeue_send: builder.equeue_send,
        });

        // Fire off the steno main thread.
        let this2 = this.clone();
        let _ = kio::spawn(
            async {
                kio::spawn_local(
                    Self::steno_main(this2, steno_recv),
                    c"w:steno",
                );
            },
            &this.main_worker,
            c"w:steno-start",
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

    /// The main task on the steno thread.
    ///
    /// This loops forever, receiving strokes, processing them, and sending them back as 'StenoText'
    /// events.  Eventually, this should be dispatching USB events directly.
    async fn steno_main(this: Arc<Self>, strokes: Receiver<Stroke>) {
        printkln!("Steno thread running");
        let mut eq_send = SendWrap(this.equeue_send.clone());
        let mut dict = Dict::new();
        loop {
            let stroke = strokes.recv_async().await.unwrap();
            for action in dict.handle_stroke(stroke, &mut eq_send, &WrapTimer) {
                this.equeue_send.send(Event::StenoText(action)).unwrap();
            }
        }
    }
}

kobj_define! {
    // The main loop thread's stack.
    static MAIN_LOOP_STACK: ThreadStack<2048>;

    // The steno thread.
    static STENO_STACK: ThreadStack<4096>;
}
