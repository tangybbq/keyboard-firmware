//! The HID keyboard reports: built from the layout's key actions, queued, and sent to the host.
//!
//! A report goes to USB when a host has configured the HID interface, and otherwise, on a board
//! with a radio, to the Bluetooth host if one is connected.  See `jolt-mesa.md`, "Bluetooth on the
//! mesa3w".

use bbq_keyboard::{KeyAction, Keyboard, Mods};
use core::cell::UnsafeCell;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    semaphore::{GreedySemaphore, Semaphore},
};
#[cfg(CONFIG_BT)]
use embassy_time::Timer;
use zephyr::{
    printkln,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

unsafe extern "C" {
    fn usb_send_report(report: *const u8, len: u16) -> i32;
    #[cfg(CONFIG_BT)]
    fn ble_send_report(report: *const u8, len: u16, epoch: usize) -> i32;
}

const HID_QUEUE_DEPTH: usize = 32;

/// How many Bluetooth reports may be waiting to go out at once.  More than one, so that a chord's
/// press and release, or an Orsy word, can share a connection event.
#[cfg(CONFIG_BT)]
const BLE_IN_FLIGHT_MAX: usize = 4;

/// `ble_send_report`'s answer when the stack is out of buffers: send the report again shortly.
#[cfg(CONFIG_BT)]
const BLE_SEND_BUSY: i32 = 1;

/// How long to wait before trying a report again when the Bluetooth stack had no buffer for it.
#[cfg(CONFIG_BT)]
const BLE_RETRY_MS: u64 = 5;

#[repr(C)]
struct HidReport {
    generation: usize,
    bytes: [u8; 8],
}

/// The report the USB stack is sending.  `hid_device_submit_report` does not copy the report: the
/// buffer has to stay put, and pointer aligned, until `input_report_done`.  So the one report in
/// flight lives here rather than in `send_usb`, which is the only writer and writes only
/// while it holds the `HID_IN_FLIGHT` permit that the done callback gives back.
#[repr(C, align(4))]
struct InFlightReport(UnsafeCell<[u8; 8]>);

// SAFETY: the only access is from `send_usb`, serialized with the USB stack's reads by the
// `HID_IN_FLIGHT` permit, as described on the type.
unsafe impl Sync for InFlightReport {}

static HID_READY: AtomicBool = AtomicBool::new(false);
static HID_GENERATION: AtomicUsize = AtomicUsize::new(0);
static HID_REPORTS: Channel<CriticalSectionRawMutex, HidReport, HID_QUEUE_DEPTH> = Channel::new();
static HID_IN_FLIGHT: GreedySemaphore<CriticalSectionRawMutex> = GreedySemaphore::new(0);
static HID_IN_FLIGHT_REPORT: InFlightReport = InFlightReport(UnsafeCell::new([0; 8]));

/// A Bluetooth host is connected, over an encrypted link.
#[cfg(CONFIG_BT)]
static BLE_READY: AtomicBool = AtomicBool::new(false);
/// Bluetooth reports sent and not yet gone out, counted down from `BLE_IN_FLIGHT_MAX`.
#[cfg(CONFIG_BT)]
static BLE_IN_FLIGHT: GreedySemaphore<CriticalSectionRawMutex> = GreedySemaphore::new(0);
/// Counts Bluetooth connections, so that a report finishing after its connection has gone, once
/// `BLE_IN_FLIGHT` has been reset for the next, is not counted against the new one.
#[cfg(CONFIG_BT)]
static BLE_EPOCH: AtomicUsize = AtomicUsize::new(0);

/// Queue the report for a key action from the layout or the steno typer.
pub fn send_key_action(key: KeyAction) {
    match key {
        KeyAction::KeyPress(key, mods) => {
            submit_report(keypress_report(key, mods));
        }
        KeyAction::ModOnly(mods) => {
            submit_report([modifier_bits(mods), 0, 0, 0, 0, 0, 0, 0]);
        }
        KeyAction::KeyRelease => {
            submit_report([0; 8]);
        }
        KeyAction::KeySet(keys) => {
            submit_report(keyset_report(&keys));
        }
        KeyAction::Stall => {
            printkln!("USB stall action");
        }
    }
}

fn submit_report(report: [u8; 8]) {
    let report = HidReport {
        generation: HID_GENERATION.load(Ordering::Acquire),
        bytes: report,
    };

    if HID_REPORTS.try_send(report).is_err() {
        printkln!("Dropping HID report: queue full");
    }
}

/// Where a report can go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Transport {
    Usb,
    #[cfg(CONFIG_BT)]
    Ble,
}

impl Transport {
    fn is_ready(self) -> bool {
        match self {
            Transport::Usb => HID_READY.load(Ordering::Acquire),
            #[cfg(CONFIG_BT)]
            Transport::Ble => BLE_READY.load(Ordering::Acquire),
        }
    }
}

/// The transport a report goes to now, if any is ready for one: USB first.
fn route() -> Option<Transport> {
    if Transport::Usb.is_ready() {
        return Some(Transport::Usb);
    }
    #[cfg(CONFIG_BT)]
    if Transport::Ble.is_ready() {
        return Some(Transport::Ble);
    }
    None
}

/// A transport came or went.  The reports already queued were meant for the old route, so they
/// are dropped, and an all-up report is queued in their place: the sender gives it to the old
/// transport as well when that is still there, so that its host is not left with a key held.
fn transport_changed() {
    HID_GENERATION.fetch_add(1, Ordering::AcqRel);
    submit_report([0; 8]);
}

#[embassy_executor::task]
pub async fn sender_task() -> () {
    let mut last = None;
    loop {
        let report = HID_REPORTS.receive().await;

        if report.generation != HID_GENERATION.load(Ordering::Acquire) {
            continue;
        }

        let Some(transport) = route() else {
            continue;
        };
        if last != Some(transport) {
            if let Some(old) = last.filter(|old: &Transport| old.is_ready()) {
                send(
                    old,
                    HidReport {
                        generation: report.generation,
                        bytes: [0; 8],
                    },
                )
                .await;
            }
            last = Some(transport);
        }
        send(transport, report).await;
    }
}

async fn send(transport: Transport, report: HidReport) {
    match transport {
        Transport::Usb => send_usb(report).await,
        #[cfg(CONFIG_BT)]
        Transport::Ble => send_ble(report).await,
    }
}

async fn send_usb(report: HidReport) {
    let permit = HID_IN_FLIGHT.acquire(1).await.unwrap();
    permit.disarm();

    if report.generation != HID_GENERATION.load(Ordering::Acquire)
        || !HID_READY.load(Ordering::Acquire)
    {
        HID_IN_FLIGHT.release(1);
        return;
    }

    let buf = HID_IN_FLIGHT_REPORT.0.get();
    // SAFETY: holding the permit means no report is in flight, so the USB stack is not
    // reading the buffer, and this task is its only writer.
    unsafe { *buf = report.bytes };
    // SAFETY: `buf` is 8 aligned bytes in a static, and nothing writes them again until
    // `input_report_done` returns the permit.
    let ret = unsafe { usb_send_report(buf.cast::<u8>(), 8) };
    if ret != 0 {
        HID_IN_FLIGHT.release(1);
        printkln!("usb_send_report failed: {}", ret);
    }
}

#[cfg(CONFIG_BT)]
async fn send_ble(report: HidReport) {
    loop {
        let permit = BLE_IN_FLIGHT.acquire(1).await.unwrap();
        permit.disarm();

        if report.generation != HID_GENERATION.load(Ordering::Acquire)
            || !BLE_READY.load(Ordering::Acquire)
        {
            BLE_IN_FLIGHT.release(1);
            return;
        }

        let epoch = BLE_EPOCH.load(Ordering::Acquire);
        // SAFETY: the report is 8 bytes, and the stack copies it into the notification before
        // returning.
        let ret = unsafe { ble_send_report(report.bytes.as_ptr(), 8, epoch) };
        if ret == 0 {
            return;
        }
        BLE_IN_FLIGHT.release(1);
        if ret != BLE_SEND_BUSY {
            printkln!("ble_send_report failed: {}", ret);
            return;
        }
        // Dropping the report could leave a key held, so wait for buffers to go out.
        Timer::after_millis(BLE_RETRY_MS).await;
    }
}

#[unsafe(no_mangle)]
extern "C" fn usb_iface_ready_callback(ready: bool) {
    HID_READY.store(ready, Ordering::Release);
    // Nothing is in flight either way.  Not zero when USB goes, so that a sender waiting on the
    // permit wakes, finds USB gone and goes on to the next report, which may be Bluetooth's.
    HID_IN_FLIGHT.set(1);
    transport_changed();
}

#[unsafe(no_mangle)]
extern "C" fn usb_input_report_done_callback() {
    if HID_READY.load(Ordering::Acquire) {
        HID_IN_FLIGHT.release(1);
    }
}

#[cfg(CONFIG_BT)]
#[unsafe(no_mangle)]
extern "C" fn ble_ready_callback(ready: bool) {
    BLE_READY.store(ready, Ordering::Release);
    BLE_EPOCH.fetch_add(1, Ordering::AcqRel);
    // As for USB: a new connection has nothing in flight, and a waiting sender has to wake when
    // the connection goes.
    BLE_IN_FLIGHT.set(BLE_IN_FLIGHT_MAX);
    transport_changed();
}

#[cfg(CONFIG_BT)]
#[unsafe(no_mangle)]
extern "C" fn ble_report_done_callback(epoch: usize) {
    if epoch == BLE_EPOCH.load(Ordering::Acquire) {
        BLE_IN_FLIGHT.release(1);
    }
}

fn keypress_report(key: Keyboard, mods: Mods) -> [u8; 8] {
    [modifier_bits(mods), 0, key as u8, 0, 0, 0, 0, 0]
}

fn keyset_report(keys: &[Keyboard]) -> [u8; 8] {
    let mut report = [0u8; 8];
    for (index, key) in keys.iter().take(6).enumerate() {
        report[index + 2] = *key as u8;
    }
    report
}

fn modifier_bits(mods: Mods) -> u8 {
    let mut bits = 0u8;
    if mods.contains(Mods::CONTROL) {
        bits |= 0x01;
    }
    if mods.contains(Mods::SHIFT) {
        bits |= 0x02;
    }
    if mods.contains(Mods::ALT) {
        bits |= 0x04;
    }
    if mods.contains(Mods::GUI) {
        bits |= 0x08;
    }
    bits
}
