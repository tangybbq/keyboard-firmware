//! Minder protocol.
//!
//! For the jolt3, USB devices each have an additional vendor-specific interface that defines two
//! bulk endpoints.  The protocol is packetized, per usb-bulk, and the payload itself is CBOR
//! encoded of the Minder protocol.

// The minder packets use alloc, and we use alloc to manage the write buffer.
extern crate alloc;

use alloc::{format, string::ToString, vec, vec::Vec};
use core::cell::RefCell;
use embassy_executor::Spawner;
use embassy_futures::select::{select3, Either3};
use bbq_keyboard::layout::fingerprint::layout_fingerprint;
use embassy_rp::{clocks::RoscRng, flash::{Blocking, Flash}, peripherals::{FLASH, WATCHDOG}, watchdog::Watchdog};
use embassy_sync::{blocking_mutex::{raw::CriticalSectionRawMutex, Mutex}, signal::Signal};
use embassy_time::{Duration, Timer};
use embassy_usb::driver::{EndpointError, EndpointIn, EndpointOut};
use embedded_storage::nor_flash::NorFlash;
use heapless::Deque;
use minder::{cap, Event, Reply, Request, VERSION};
use sha2::{Digest, Sha256};

#[allow(unused_imports)]
use crate::logging::{info, unwrap, warn};

type FlashType = Flash<'static, FLASH, Blocking, FLASH_DEV_SIZE>;

pub struct Minder<Rd, Wr>
where
    Rd: EndpointOut,
    Wr: EndpointIn,
{
    reader: Rd,
    writer: Wr,
    unique: &'static str,

    read_buf: [u8; 64],

    flash: FlashType,

    /// Identifies this run of the firmware; see `Reply::Hello`.
    boot_id: u64,
}

/// Size limit for read. Prevents memory loss from excessive data.
/// Value chosen so cbor data with 4k buffer in it should be fine.
const SIZE_LIMIT: usize = 4200;

/// How many events can be waiting for the host at once.
///
/// Events are notifications, not data: phase 2's log lives in its own ring buffer, and an event
/// only says that there is something there to fetch.  A host that has fallen this far behind gains
/// nothing from the older notifications, so the queue drops the oldest rather than blocking the
/// side that raised it.
const EVENT_QUEUE_DEPTH: usize = 8;

/// Events waiting to be handed to the host.
static EVENTS: EventQueue = EventQueue::new();

/// A pending `TestEvent` request, picked up by [`test_event_task`].
static TEST_EVENT_REQ: Signal<CriticalSectionRawMutex, (u32, u32)> = Signal::new();

/// A small bounded queue of events for the host, which drops the oldest on overflow.
///
/// Push is non-blocking and callable from anywhere, which is what phase 2's key event hook needs:
/// logging must never delay or drop a key.
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
#[allow(dead_code)]
pub fn push_event(event: Event) {
    EVENTS.push(event);
}

/// How a pending `GetEvent` ended.
enum GetEvent {
    /// An event arrived.
    Event(Event),
    /// The requested timeout expired.
    Timeout,
    /// A new request started arriving; its first packet, of this length, is in the read buffer.
    Interrupted(usize),
    /// The read failed.
    ReadError(EndpointError),
}

/// Raise the test events asked for by `Request::TestEvent`.
///
/// Kept as a task so the delay does not hold up the minder loop, which is the whole point: the
/// events land while a `GetEvent` is already pending.  Only one batch can be pending at a time; a
/// second request during the delay of a first replaces it.
#[embassy_executor::task]
async fn test_event_task() {
    loop {
        let (count, delay_ms) = TEST_EVENT_REQ.wait().await;
        Timer::after(Duration::from_millis(delay_ms as u64)).await;
        for seq in 1..=count {
            EVENTS.push(Event::Test { seq });
        }
    }
}

impl<Rd: EndpointOut, Wr: EndpointIn> Minder<Rd, Wr> {
    pub fn new(reader: Rd, writer: Wr, unique: &'static str) -> Self {
        let flash = Flash::new_blocking(unsafe { FLASH::steal() });

        Self {
            reader,
            writer,
            unique,
            read_buf: [0; 64],
            flash,
            // The ROSC is free-running and is what the chip has to offer here.  This only
            // has to differ from the last run, not be unpredictable.
            boot_id: RoscRng.next_u64(),
        }
    }

    /// The main loop, reads requests and replies to them.
    ///
    /// Strictly sequential: exactly one request is read, dispatched and replied to at a time.  That
    /// is what keeps a pending `GetEvent` from being able to fire inside `program`, which blocks
    /// with interrupts masked.  It stays true only while this loop is sequential.
    pub async fn main_loop(mut self) -> ! {
        let spawner = unsafe { Spawner::for_current_executor() }.await;
        spawner.spawn(unwrap!(test_event_task()));

        // The first packet of a request that arrived while a `GetEvent` was pending, already
        // sitting in the read buffer.
        let mut pending_first: Option<usize> = None;

        loop {
            let first_len = match pending_first.take() {
                Some(len) => len,
                None => match self.read_packet().await {
                    Ok(len) => len,
                    Err(err) => {
                        warn!("Minder read error: {:?}", err);
                        continue;
                    }
                },
            };

            let rbuf = match self.bulk_read_rest(first_len).await {
                Ok(rbuf) => rbuf,
                Err(err) => {
                    warn!("Minder read error: {:?}", err);
                    continue;
                }
            };

            // info!("Minder read {} bytes", rbuf.len());
            let reply = match minicbor::decode::<Request>(&rbuf) {
                Ok(Request::Hello { version }) => self.hello(&version).await,
                Ok(Request::ReadFlash { offset, size }) => {
                    let _ = (offset, size);
                    todo!();
                }
                Ok(Request::Reset) => self.reset().await,
                Ok(Request::Hash { offset, size }) => self.hash(offset, size),
                Ok(Request::Program { offset, data }) => self.program(offset, data.into()),
                Ok(Request::GetEvent { timeout_ms }) => match self.get_event(timeout_ms).await {
                    GetEvent::Event(event) => Reply::Event { event },
                    GetEvent::Timeout => Reply::NoEvent,
                    GetEvent::Interrupted(len) => {
                        // Answer the poll before the request that interrupted it.  Every request
                        // still gets exactly one reply, in order, so the host can tell them apart
                        // without the requests being tagged.
                        pending_first = Some(len);
                        Reply::NoEvent
                    }
                    GetEvent::ReadError(err) => {
                        // Still answer the poll, or the host waits forever for a reply that the
                        // accounting says is owed.
                        warn!("Minder read error: {:?}", err);
                        Reply::NoEvent
                    }
                },
                Ok(Request::TestEvent { count, delay_ms }) => {
                    TEST_EVENT_REQ.signal((count, delay_ms));
                    Reply::Ok
                }
                Err(_) => {
                    warn!("Error decoding packet");
                    continue;
                }
            };

            // Send the reply back.
            let mut wbuf = Vec::new();
            if minicbor::encode(&reply, &mut wbuf).is_err() {
                warn!("Error encoding packet");
                continue;
            }

            // info!("USB write {} bytes", wbuf.len());
            if let Err(e) = self.bulk_write(&wbuf).await {
                warn!("Error writing to USB: {:?}", e);
            }
        }
    }

    /// Write a packet out, via USB bulk, breaking into individual packets as needed.
    async fn bulk_write(&mut self, packet: &[u8]) -> Result<(), EndpointError> {
        let mut offset = 0;
        let length = packet.len();

        while offset < length {
            let end = (offset + 64).min(length);
            let chunk = &packet[offset..end];
            self.writer.write(chunk).await?;
            offset = end;
        }

        // If the data is a multiple of the packet size, send a zero-byte packet.
        if length % 64 == 0 {
            self.writer.write(&[]).await?;
        }

        Ok(())
    }

    /// Read a single USB bulk packet into the read buffer, returning its length.
    ///
    /// This is split out from the rest of the read so that waiting for a request to *start* can be
    /// selected over.  Once a first packet has arrived, the remaining packets of that request must
    /// be read without interruption, or the stream desynchronizes.
    async fn read_packet(&mut self) -> Result<usize, EndpointError> {
        self.reader.read(&mut self.read_buf).await
    }

    /// Assemble a full message, given the length of a first packet already sitting in the read
    /// buffer.  Continues reading packets until a short one, and places the result into a Vec.
    async fn bulk_read_rest(&mut self, first_len: usize) -> Result<Vec<u8>, EndpointError> {
        let mut result = Vec::new();
        let mut warned = false;
        let mut len = first_len;

        loop {
            if result.len() + len < SIZE_LIMIT {
                result.extend_from_slice(&self.read_buf[..len]);
            } else {
                if !warned {
                    warn!("Excessively large USB bulk data received, discarding");
                    warned = true;
                }
            }

            if len < 64 {
                break;
            }

            len = self.reader.read(&mut self.read_buf).await?;
        }

        Ok(result)
    }

    /// Wait, for up to `timeout_ms`, for something to report to the host.
    ///
    /// The select is deliberately over [`read_packet`](Self::read_packet) rather than a whole
    /// request: a request that has started must be read to completion, or its remaining packets
    /// desynchronize everything that follows.  The interrupting packet is handed back for the main
    /// loop to finish reading, after the `NoEvent` for this poll has gone out.
    ///
    /// The order of the branches is the priority when more than one is ready: a queued event beats
    /// a new request, and a new request beats the timeout.
    async fn get_event(&mut self, timeout_ms: u32) -> GetEvent {
        let timeout = Timer::after(Duration::from_millis(timeout_ms as u64));

        match select3(EVENTS.wait(), self.read_packet(), timeout).await {
            Either3::First(event) => GetEvent::Event(event),
            Either3::Second(Ok(len)) => GetEvent::Interrupted(len),
            Either3::Second(Err(err)) => GetEvent::ReadError(err),
            Either3::Third(()) => GetEvent::Timeout,
        }
    }

    /// Given a hello pack, generate our detailed response.
    ///
    /// The host's version is not checked.  Capabilities are what it should branch on, and
    /// a device that cannot do something simply does not list it.
    async fn hello(&mut self, _version: &str) -> Reply {
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

    /// Trigger a reset shortly after we acknowledge.
    async fn reset(&mut self) -> Reply {
        let spawner = unsafe { Spawner::for_current_executor() }.await;

        spawner.spawn(unwrap!(reset_device()));

        Reply::Reset
    }

    // TODO: Do this with the flash driver?
    /// Calculate the hash of a region.  Sync.
    fn hash(&self, offset: u32, size: u32) -> Reply {
        // Validate that the offset and size is in a valid region.
        if offset < FLASH_START {
            return Reply::Error { text: "Out of bounds".into() };
        }

        if let Some(end) = offset.checked_add(size) {
            if end > FLASH_START + FLASH_SIZE {
                return Reply::Error { text: "Out of bounds".into() };
            }
        } else {
            return Reply::Error { text: "Out of bounds".into() };
        }

        let mut hasher = Sha256::new();

        let start_addr = offset as *const u8;
        let slice = unsafe { core::slice::from_raw_parts(start_addr, size as usize) };

        // info!("Hashing");
        hasher.update(slice);
        let digest = hasher.finalize();
        // info!("Done Hashing {:x}", digest.as_slice());

        let digest: [u8; 32] = digest.into();
        Reply::Hash { hash: digest.into() }
    }

    /// Program a single page of the flash.
    fn program(&mut self, offset: u32, data: Vec<u8>) -> Reply {
        let esize = FlashType::ERASE_SIZE;
        let base = embassy_rp::flash::FLASH_BASE as u32;

        // Ensure the offset falls on a page boundary.
        if offset & (esize as u32 - 1) != 0 {
            return Reply::Error { text: "Program not on erase boundary".into() };
        }

        // Ensure the size is equal or less than the erase size.
        if data.len() > esize {
            return Reply::Error { text: "Program larger than a single erase block".into() };
        }

        // Note that on the RP2040, since we are XIP, erase and write are both blocking operations
        // (including masking interrupts).  This is likely to disrupt typing, but as this is user
        // initiated, it shouldn't really be an issue.
        // The USB controller should be able to handle the delay caused by this, as it will auto-nak
        // queries.

        // The offset needs to be adjusted by the start of the flash device.
        let offset = if let Some(offset) = offset.checked_sub(base) {
            offset
        } else {
            return Reply::Error { text: format!("Flash offset out of bounds") };
        };

        // At this point, we will just erase an full erase block, and then program whatever data
        // we've been given.  The assumption being that the partial page at the end will just remain
        // as the erased value.
        if let Err(err) = self.flash.blocking_erase(offset, offset + esize as u32) {
            return Reply::Error { text: format!("erase error: {:?}", err) };
        }

        if let Err(err) = self.flash.blocking_write(offset, &data) {
            return Reply::Error { text: format!("program error: {:?}", err) };
        }

        Reply::ProgramDone
    }
}

/// The size of the entire flash device.
const FLASH_DEV_SIZE: usize = 8 * 1024 * 1024;

/// The start address of valid flash.
const FLASH_START: u32 = 0x10100000;

/// The size of the valid flash.
const FLASH_SIZE: u32 = 8 * 1024 * 1024 - 0x100000;

/// Reset the device.  This delays a small amount, and then uses the watchdog hardware to reset the
/// device.
#[embassy_executor::task]
async fn reset_device() {
    Timer::after(Duration::from_millis(500)).await;

    let mut dog = Watchdog::new(unsafe { WATCHDOG::steal() });
    dog.trigger_reset();
}
