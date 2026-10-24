//! Minder protocol.
//!
//! The vendor-specific USB interface in `minder_usb.c` carries CBOR-encoded minder requests and
//! replies.  This is jolt-embassy-rp's `minder.rs`, and answers the same way; what differs is
//! underneath it.  Requests arrive whole rather than as packets (see [`crate::minder_usb`]), the
//! flash and the reset go through `platform.c`, and the boot id is the ROSC's random bit read
//! from there.

extern crate alloc;

use alloc::{format, string::ToString, vec, vec::Vec};
use core::cell::RefCell;

use bbq_keyboard::layout::fingerprint::layout_fingerprint;
use embassy_futures::select::{Either3, select3};
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    signal::Signal,
};
use embassy_time::{Duration, Timer};
use heapless::Deque;
use minder::{Event, Reply, Request, VERSION, cap};
use sha2::{Digest, Sha256};
use zephyr::printkln;

use crate::minder_usb;

unsafe extern "C" {
    fn jolt_boot_id() -> u64;
    fn jolt_flash_program(offset: u32, data: *const u8, len: usize, erase_size: usize) -> i32;
    fn jolt_reboot() -> !;
}

pub struct Minder {
    unique: &'static str,

    /// Identifies this run of the firmware; see `Reply::Hello`.
    boot_id: u64,
}

/// Size limit for a request.  Value chosen so cbor data with 4k buffer in it should be fine.
/// `minder_usb.c` drops anything longer before it gets here.
const SIZE_LIMIT: usize = 4200;

/// How many events can be waiting for the host at once.
///
/// Events are notifications, not data: the key log lives in its own ring buffer, and an event
/// only says that there is something there to fetch.  A host that has fallen this far behind
/// gains nothing from the older notifications, so the queue drops the oldest rather than blocking
/// the side that raised it.
const EVENT_QUEUE_DEPTH: usize = 8;

/// Events waiting to be handed to the host.
static EVENTS: EventQueue = EventQueue::new();

/// A pending `TestEvent` request, picked up by [`test_event_task`].
static TEST_EVENT_REQ: Signal<CriticalSectionRawMutex, (u32, u32)> = Signal::new();

/// A pending `Reset`, picked up by [`reset_task`].
static RESET_REQ: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// A small bounded queue of events for the host, which drops the oldest on overflow.
///
/// Push is non-blocking and callable from anywhere, which is what the key log needs: logging must
/// never delay or drop a key.
struct EventQueue {
    queue: Mutex<CriticalSectionRawMutex, RefCell<Deque<Event, EVENT_QUEUE_DEPTH>>>,
    waker: Signal<CriticalSectionRawMutex, ()>,
}

impl EventQueue {
    const fn new() -> Self {
        Self {
            queue: Mutex::new(RefCell::new(Deque::new())),
            waker: Signal::new(),
        }
    }

    /// Queue an event, discarding the oldest if the queue is full.
    fn push(&self, event: Event) {
        self.queue.lock(|queue| {
            let mut queue = queue.borrow_mut();
            if queue.is_full() {
                let _ = queue.pop_front();
            }
            let _ = queue.push_back(event);
        });
        self.waker.signal(());
    }

    fn pop(&self) -> Option<Event> {
        self.queue.lock(|queue| queue.borrow_mut().pop_front())
    }

    /// Wait until there is an event, and take it.
    ///
    /// Cancel safe: an event is only removed from the queue by the poll that returns it, so
    /// dropping this future in a `select` cannot lose one.  A stale signal only costs an extra
    /// trip around the loop.
    async fn wait(&self) -> Event {
        loop {
            if let Some(event) = self.pop() {
                return event;
            }
            self.waker.wait().await;
        }
    }
}

/// How a pending `GetEvent` ended.
enum GetEvent {
    /// An event arrived.
    Event(Event),
    /// The requested timeout expired.
    Timeout,
    /// A new request arrived, whole.
    Interrupted(Vec<u8>),
    /// The read failed.
    ReadError(minder_usb::Error),
}

/// Raise the test events asked for by `Request::TestEvent`.
///
/// Kept as a task so the delay does not hold up the minder loop, which is the whole point: the
/// events land while a `GetEvent` is already pending.  Only one batch can be pending at a time; a
/// second request during the delay of a first replaces it.
#[embassy_executor::task]
pub async fn test_event_task() {
    loop {
        let (count, delay_ms) = TEST_EVENT_REQ.wait().await;
        Timer::after(Duration::from_millis(delay_ms as u64)).await;
        for seq in 1..=count {
            EVENTS.push(Event::Test { seq });
        }
    }
}

/// Reset the device, a little after the `Reset` reply has gone.
#[embassy_executor::task]
pub async fn reset_task() {
    RESET_REQ.wait().await;
    Timer::after(Duration::from_millis(500)).await;
    // SAFETY: no arguments; it does not return.
    unsafe { jolt_reboot() }
}

/// The minder loop, as its own task.
#[embassy_executor::task]
pub async fn minder_task(unique: &'static str) {
    Minder::new(unique).main_loop().await
}

impl Minder {
    pub fn new(unique: &'static str) -> Self {
        Self {
            unique,
            // SAFETY: no arguments; reads a register.
            boot_id: unsafe { jolt_boot_id() },
        }
    }

    /// The main loop, reads requests and replies to them.
    ///
    /// Strictly sequential: exactly one request is read, dispatched and replied to at a time.  That
    /// is what keeps a pending `GetEvent` from being able to fire inside `program`, which blocks
    /// with interrupts masked.  It stays true only while this loop is sequential.
    pub async fn main_loop(mut self) -> ! {
        // A request that arrived while a `GetEvent` was pending.
        let mut pending: Option<Vec<u8>> = None;

        loop {
            let rbuf = match pending.take() {
                Some(rbuf) => rbuf,
                None => match minder_usb::read().await {
                    Ok(rbuf) => rbuf,
                    Err(err) => {
                        printkln!("Minder read error: {:?}", err);
                        continue;
                    }
                },
            };
            if rbuf.len() >= SIZE_LIMIT {
                printkln!("Excessively large USB bulk data received, discarding");
                continue;
            }

            let reply = match minicbor::decode::<Request>(&rbuf) {
                Ok(Request::Hello { version }) => self.hello(&version),
                Ok(Request::ReadFlash { offset, size }) => {
                    // Not implemented on jolt-embassy-rp either, where it panics.
                    let _ = (offset, size);
                    Reply::Error {
                        text: "ReadFlash is not supported".into(),
                    }
                }
                Ok(Request::Reset) => {
                    RESET_REQ.signal(());
                    Reply::Reset
                }
                Ok(Request::Hash { offset, size }) => self.hash(offset, size),
                Ok(Request::Program { offset, data }) => self.program(offset, data.into()),
                Ok(Request::GetEvent { timeout_ms }) => match self.get_event(timeout_ms).await {
                    GetEvent::Event(event) => Reply::Event { event },
                    GetEvent::Timeout => Reply::NoEvent,
                    GetEvent::Interrupted(rbuf) => {
                        // Answer the poll before the request that interrupted it.  Every request
                        // still gets exactly one reply, in order, so the host can tell them apart
                        // without the requests being tagged.
                        pending = Some(rbuf);
                        Reply::NoEvent
                    }
                    GetEvent::ReadError(err) => {
                        // Still answer the poll, or the host waits forever for a reply that the
                        // accounting says is owed.
                        printkln!("Minder read error: {:?}", err);
                        Reply::NoEvent
                    }
                },
                Ok(Request::SetLogging { .. })
                | Ok(Request::GetEventLog { .. })
                | Ok(Request::EventLogAck { .. }) => Reply::Error {
                    text: "No key log".into(),
                },
                Ok(Request::TestEvent { count, delay_ms }) => {
                    TEST_EVENT_REQ.signal((count, delay_ms));
                    Reply::Ok
                }
                Err(_) => {
                    printkln!("Error decoding minder packet");
                    continue;
                }
            };

            // Send the reply back.
            let mut wbuf = Vec::new();
            if minicbor::encode(&reply, &mut wbuf).is_err() {
                printkln!("Error encoding minder packet");
                continue;
            }

            if let Err(err) = minder_usb::write(&wbuf).await {
                printkln!("Error writing to USB: {:?}", err);
            }
        }
    }

    /// Wait, for up to `timeout_ms`, for something to report to the host.
    ///
    /// The order of the branches is the priority when more than one is ready: a queued event beats
    /// a new request, and a new request beats the timeout.  Both reads are cancel safe.
    async fn get_event(&mut self, timeout_ms: u32) -> GetEvent {
        let timeout = Timer::after(Duration::from_millis(timeout_ms as u64));

        match select3(EVENTS.wait(), minder_usb::read(), timeout).await {
            Either3::First(event) => GetEvent::Event(event),
            Either3::Second(Ok(rbuf)) => GetEvent::Interrupted(rbuf),
            Either3::Second(Err(err)) => GetEvent::ReadError(err),
            Either3::Third(()) => GetEvent::Timeout,
        }
    }

    /// Given a hello pack, generate our detailed response.
    ///
    /// The host's version is not checked.  Capabilities are what it should branch on, and
    /// a device that cannot do something simply does not list it.
    fn hello(&mut self, _version: &str) -> Reply {
        Reply::Hello {
            version: VERSION.into(),
            info: self.unique.into(),
            boot_id: Some(self.boot_id),
            layout_fingerprint: Some(layout_fingerprint()),
            capabilities: Some(vec![
                cap::EVENTS.to_string(),
                cap::TEST_EVENTS.to_string(),
                cap::FLASH.to_string(),
            ]),
        }
    }

    /// Calculate the hash of a region.  Sync.
    fn hash(&self, offset: u32, size: u32) -> Reply {
        // Validate that the offset and size is in a valid region.
        if offset < FLASH_START {
            return Reply::Error {
                text: "Out of bounds".into(),
            };
        }

        if let Some(end) = offset.checked_add(size) {
            if end > FLASH_START + FLASH_SIZE {
                return Reply::Error {
                    text: "Out of bounds".into(),
                };
            }
        } else {
            return Reply::Error {
                text: "Out of bounds".into(),
            };
        }

        let mut hasher = Sha256::new();

        // SAFETY: the range was checked above to lie within the memory-mapped flash, which is
        // always readable, and nothing writes it while this runs: only `program` does, from this
        // same sequential loop.
        let slice = unsafe { core::slice::from_raw_parts(offset as *const u8, size as usize) };
        hasher.update(slice);
        let digest: [u8; 32] = hasher.finalize().into();
        Reply::Hash {
            hash: digest.into(),
        }
    }

    /// Program a single page of the flash.
    fn program(&mut self, offset: u32, data: Vec<u8>) -> Reply {
        // Ensure the offset falls on a page boundary.
        if offset & (ERASE_SIZE as u32 - 1) != 0 {
            return Reply::Error {
                text: "Program not on erase boundary".into(),
            };
        }

        // Ensure the size is equal or less than the erase size.
        if data.len() > ERASE_SIZE {
            return Reply::Error {
                text: "Program larger than a single erase block".into(),
            };
        }

        // The offset needs to be adjusted by the start of the flash device.
        let Some(offset) = offset.checked_sub(FLASH_BASE) else {
            return Reply::Error {
                text: "Flash offset out of bounds".into(),
            };
        };

        // Note that on the RP2040, since we are XIP, erase and write are both blocking operations
        // (including masking interrupts).  This is likely to disrupt typing, but as this is user
        // initiated, it shouldn't really be an issue.  The USB controller NAKs while it waits.
        //
        // A whole erase block is erased, and whatever data was given is written; a partial page
        // at the end remains erased.
        //
        // SAFETY: `data` is readable for its whole length, which is what is passed.
        let ret = unsafe { jolt_flash_program(offset, data.as_ptr(), data.len(), ERASE_SIZE) };
        if ret != 0 {
            return Reply::Error {
                text: format!("program error: {}", ret),
            };
        }

        Reply::ProgramDone
    }
}

/// Where the flash is mapped.
const FLASH_BASE: u32 = 0x1000_0000;

/// The flash's erase block.
const ERASE_SIZE: usize = 4096;

/// The start address of valid flash.
const FLASH_START: u32 = 0x10100000;

/// The size of the valid flash.
const FLASH_SIZE: u32 = 8 * 1024 * 1024 - 0x100000;
