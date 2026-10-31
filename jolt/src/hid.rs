//! The HID keyboard reports: built from the layout's key actions, queued, and sent to the host.

use bbq_keyboard::{KeyAction, Keyboard, Mods};
use core::cell::UnsafeCell;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex,
    channel::Channel,
    semaphore::{GreedySemaphore, Semaphore},
};
use zephyr::{
    printkln,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

unsafe extern "C" {
    fn usb_send_report(report: *const u8, len: u16) -> i32;
}

const HID_QUEUE_DEPTH: usize = 32;

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

/// Where a report can go.  Only USB so far.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Transport {
    Usb,
}

/// The transport a report goes to now, if any is ready for one.
fn route() -> Option<Transport> {
    if HID_READY.load(Ordering::Acquire) {
        Some(Transport::Usb)
    } else {
        None
    }
}

#[embassy_executor::task]
pub async fn sender_task() -> () {
    loop {
        let report = HID_REPORTS.receive().await;

        if report.generation != HID_GENERATION.load(Ordering::Acquire) {
            continue;
        }

        match route() {
            Some(Transport::Usb) => send_usb(report).await,
            None => (),
        }
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

#[unsafe(no_mangle)]
extern "C" fn usb_iface_ready_callback(ready: bool) {
    HID_READY.store(ready, Ordering::Release);
    HID_GENERATION.fetch_add(1, Ordering::AcqRel);
    HID_IN_FLIGHT.set(if ready { 1 } else { 0 });
}

#[unsafe(no_mangle)]
extern "C" fn usb_input_report_done_callback() {
    if HID_READY.load(Ordering::Acquire) {
        HID_IN_FLIGHT.release(1);
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
