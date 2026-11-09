//! Minder protocol.
//!
//! The vendor-specific USB interface in `minder_usb.c` carries CBOR-encoded minder requests and
//! replies.  This is jolt-embassy-rp's `minder.rs`, and answers the same way; what differs is
//! underneath it.  Requests arrive whole rather than as packets (see [`crate::minder_usb`]), the
//! flash and the reset go through `platform.c`, and the boot id is the ROSC's random bit read
//! from there.
//!
//! The loop talks to its host through a [`Link`]: a request comes in on one, and its reply goes
//! back on the same one.  USB is always a link, and on a board with a radio Bluetooth is another
//! ([`crate::minder_ble`]); a request on either interrupts a `GetEvent` pending on the other.

extern crate alloc;

use alloc::{format, string::ToString, vec, vec::Vec};
use core::cell::RefCell;

use bbq_keyboard::layout::fingerprint::layout_fingerprint;
#[cfg(CONFIG_BT)]
use embassy_futures::select::{Either, select};
use embassy_futures::select::{Either3, select3};
use embassy_sync::{
    blocking_mutex::{Mutex, raw::CriticalSectionRawMutex},
    signal::Signal,
};
use embassy_time::{Duration, Timer};
use heapless::Deque;
use minder::keylog::RECORD_SIZE;
use minder::{Event, Reply, Request, VERSION, cap};
use sha2::{Digest, Sha256};
use zephyr::{
    devicetree::{Value, Word},
    printkln,
};

#[cfg(CONFIG_BT)]
use crate::minder_ble;
use crate::{keylog, minder_usb};

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

/// Queue an event to be reported to the host on its next `GetEvent`.
///
/// Never blocks, and never fails; if the queue is full the oldest event is dropped.
pub fn push_event(event: Event) {
    EVENTS.push(event);
}

/// A transport the host can reach minder through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    Usb,
    #[cfg(CONFIG_BT)]
    Ble,
}

/// A whole request, and the link it came on, which its reply goes back on.
struct Incoming {
    link: Link,
    data: Vec<u8>,
}

/// A failed read or write, as the link's own error.
enum LinkError {
    Usb(minder_usb::Error),
    #[cfg(CONFIG_BT)]
    Ble(minder_ble::Error),
}

impl core::fmt::Debug for LinkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LinkError::Usb(err) => write!(f, "USB {:?}", err),
            #[cfg(CONFIG_BT)]
            LinkError::Ble(err) => write!(f, "Bluetooth {:?}", err),
        }
    }
}

/// Receive the next request, from whichever link has one.
///
/// Cancel safe, as each link's read is, which `GetEvent` relies on, and which this relies on
/// itself in dropping the link that lost.
async fn read() -> Result<Incoming, LinkError> {
    #[cfg(CONFIG_BT)]
    let (link, data) = match select(minder_usb::read(), minder_ble::read()).await {
        Either::First(usb) => (Link::Usb, usb.map_err(LinkError::Usb)?),
        Either::Second(ble) => (Link::Ble, ble),
    };
    #[cfg(not(CONFIG_BT))]
    let (link, data) = (Link::Usb, minder_usb::read().await.map_err(LinkError::Usb)?);

    Ok(Incoming { link, data })
}

/// Send a reply on a link, and wait for it to have gone.
async fn write(link: Link, message: &[u8]) -> Result<(), LinkError> {
    match link {
        Link::Usb => minder_usb::write(message).await.map_err(LinkError::Usb),
        #[cfg(CONFIG_BT)]
        Link::Ble => minder_ble::write(message).await.map_err(LinkError::Ble),
    }
}

/// How a pending `GetEvent` ended.
enum GetEvent {
    /// An event arrived.
    Event(Event),
    /// The requested timeout expired.
    Timeout,
    /// A new request arrived, whole.
    Interrupted(Incoming),
    /// The read failed.
    ReadError(LinkError),
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
        let mut pending: Option<Incoming> = None;

        loop {
            let Incoming { link, data: rbuf } = match pending.take() {
                Some(incoming) => incoming,
                None => match read().await {
                    Ok(incoming) => incoming,
                    Err(err) => {
                        printkln!("Minder read error: {:?}", err);
                        continue;
                    }
                },
            };
            if rbuf.len() >= SIZE_LIMIT {
                printkln!("Excessively large minder request on {:?}, discarding", link);
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
                Ok(Request::ImageInfo) => Reply::Error {
                    text: "ImageInfo is not supported".into(),
                },
                Ok(Request::Hash { offset, size }) => self.hash(offset, size),
                Ok(Request::Program { offset, data }) => self.program(offset, data.into()),
                Ok(Request::GetEvent { timeout_ms }) => match self.get_event(timeout_ms).await {
                    GetEvent::Event(event) => Reply::Event { event },
                    GetEvent::Timeout => Reply::NoEvent,
                    GetEvent::Interrupted(incoming) => {
                        // Answer the poll before the request that interrupted it.  Every request
                        // still gets exactly one reply, in order, so the host can tell them apart
                        // without the requests being tagged.
                        pending = Some(incoming);
                        Reply::NoEvent
                    }
                    GetEvent::ReadError(err) => {
                        // Still answer the poll, or the host waits forever for a reply that the
                        // accounting says is owed.
                        printkln!("Minder read error: {:?}", err);
                        Reply::NoEvent
                    }
                },
                Ok(Request::SetLogging { enabled, watermark }) => {
                    keylog::set_logging(enabled, watermark);
                    if enabled {
                        // A host attaching mid-session has no idea which mode, chord table or
                        // row position the engine is in.  Say so straight away, so the first
                        // records it gets are interpretable.
                        keylog::log_state();
                    }
                    Reply::Ok
                }
                Ok(Request::GetEventLog { max_bytes }) => self.event_log(max_bytes),
                Ok(Request::EventLogAck { through_seq }) => {
                    keylog::ack(through_seq);
                    Reply::Ok
                }
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

            if let Err(err) = write(link, &wbuf).await {
                printkln!("Minder write error: {:?}", err);
            }
        }
    }

    /// Wait, for up to `timeout_ms`, for something to report to the host.
    ///
    /// The order of the branches is the priority when more than one is ready: a queued event beats
    /// a new request, and a new request beats the timeout.  Both reads are cancel safe.
    async fn get_event(&mut self, timeout_ms: u32) -> GetEvent {
        let timeout = Timer::after(Duration::from_millis(timeout_ms as u64));

        match select3(EVENTS.wait(), read(), timeout).await {
            Either3::First(event) => GetEvent::Event(event),
            Either3::Second(Ok(rbuf)) => GetEvent::Interrupted(rbuf),
            Either3::Second(Err(err)) => GetEvent::ReadError(err),
            Either3::Third(()) => GetEvent::Timeout,
        }
    }

    /// Copy a batch of key log records out for the host.
    ///
    /// Capped well inside `SIZE_LIMIT`, which bounds a whole CBOR message rather than just
    /// its payload; the rest of the reply's fields and the CBOR framing have to fit too.
    fn event_log(&mut self, max_bytes: u32) -> Reply {
        const MAX_RECORDS: usize = 900;
        let mut buf = [0u8; MAX_RECORDS * RECORD_SIZE];

        let want = (max_bytes as usize).min(buf.len());
        let batch = keylog::drain(&mut buf, want);

        Reply::EventLog {
            boot_id: self.boot_id,
            seq: batch.seq,
            dropped: batch.dropped,
            anchor_ms: batch.anchor_ms,
            records: buf[..batch.count * RECORD_SIZE].to_vec().into(),
            remaining: batch.remaining,
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
                cap::KEY_LOG.to_string(),
            ]),
        }
    }

    /// Calculate the hash of a region.  Sync.
    fn hash(&self, offset: u32, size: u32) -> Reply {
        if !within(offset, size, data_region()) {
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

        // The whole block is erased, so the whole block has to be in the data region: below it
        // is the firmware, and on the mesa3w MCUboot.
        if !within(offset, ERASE_SIZE as u32, data_region()) {
            return Reply::Error {
                text: "Out of bounds".into(),
            };
        }

        // The offset needs to be adjusted by the start of the flash device.
        let Some(offset) = offset.checked_sub(flash_device().0) else {
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

/// The flash's erase block: 4 KiB on both the RP2040's QSPI flash and the nRF52840.
const ERASE_SIZE: usize = 4096;

/// Where the flash device is mapped, and its size, from the chosen `zephyr,flash`: 8 MiB at
/// `0x1000_0000` on a Tiny 2040, 1 MiB at 0 on an nRF52840.
fn flash_device() -> (u32, u32) {
    match zephyr::devicetree::chosen::zephyr_flash::RAW_REG {
        [Value::Words([Word::Number(base), Word::Number(size), ..])] => (*base, *size),
        _ => panic!("chosen zephyr,flash does not provide a usable reg property"),
    }
}

/// How far into the flash device `Hash` and `Program` may start.
///
/// The first MiB is the firmware's, and above it is where data such as the steno dictionaries
/// is written, as on jolt-embassy-rp.  An nRF52840 has only the one MiB, so there they reach
/// nothing.
const DATA_OFFSET: u32 = 0x10_0000;

/// Whether `size` bytes at `offset` lie wholly within `region`, a start address and size.
fn within(offset: u32, size: u32, (start, len): (u32, u32)) -> bool {
    offset >= start && offset.checked_add(size).is_some_and(|end| end <= start + len)
}

/// The part of the flash `Hash` may read and `Program` may write, as its start address and size.
fn data_region() -> (u32, u32) {
    let (base, size) = flash_device();
    let offset = DATA_OFFSET.min(size);
    (base + offset, size - offset)
}
