//! Zephyr USB interface.
//!
//! This interfaces directly with the USB stack.  As this is not very general, we just use the
//! unsafe entries directly.

use core::{ffi::CStr, ptr, sync::atomic::Ordering};

use alloc::{collections::vec_deque::VecDeque, vec::Vec};
use log::{error, info, warn};
use zephyr::{
    error::to_result_void, kio::sync::Mutex, raw, sync::{atomic::AtomicPtr, Arc}, sys::sync::Semaphore, time::{NoWait, Timeout}, Error, Result
};

use crate::rust_usb_status;

/// There is a single instance of the USB system.  As this is somewhat unsafe, we'll just
/// require the caller to create only a single instance of this (for now).
pub struct Usb {
    hid0: Arc<HidWrap>,
    hid1: Arc<HidWrap>,
    hid2: Arc<HidWrap>,
}

impl Usb {
    pub fn new() -> Result<Usb> {
        let hid0 = Self::setup_hid(c"HID_0", &HID0, Semaphore::new(0, u32::MAX).unwrap());
        let hid1 = Self::setup_hid(c"HID_1", &HID1, Semaphore::new(0, u32::MAX).unwrap());
        let hid2 = Self::setup_hid(c"HID_2", &HID2, Semaphore::new(0, u32::MAX).unwrap());

        let kbd_desc = unsafe { hid_get_kbd_desc() };
        unsafe {
            raw::usb_hid_register_device(hid0.device, kbd_desc.base, kbd_desc.len, &USB_OPS);
            raw::usb_hid_init(hid0.device);

            raw::usb_hid_register_device(
                hid1.device,
                PLOVER_HID_DESC.as_ptr(),
                PLOVER_HID_DESC.len(),
                &USB_OPS,
            );
            raw::usb_hid_init(hid1.device);

            raw::usb_hid_register_device(
                hid2.device,
                MINDER_HID_DESC.as_ptr(),
                MINDER_HID_DESC.len(),
                &USB_OPS,
            );
            raw::usb_hid_init(hid2.device);

            if raw::usb_enable(Some(status_cb)) != 0 {
                error!("Failed to enable USB");
                return Err(Error(raw::ENODEV));
            }
        }

        Ok(Usb { hid0, hid1, hid2 })
    }

    fn setup_hid(cname: &CStr, global: &AtomicPtr<HidWrap>, out_sem: Semaphore) -> Arc<HidWrap> {
        let dev = unsafe { raw::device_get_binding(cname.as_ptr()) };
        if dev.is_null() {
            panic!("Cannot get USB {:?} device", cname);
        }

        let hid = Arc::new(HidWrap {
            device: dev,
            out_sem,
            state: Mutex::new(HidIn {
                ready: true,
                additional: VecDeque::new(),
            }),
        });

        let hid_ptr = Arc::into_raw(hid.clone()) as *mut _;

        if global
            .compare_exchange(
                ptr::null_mut(),
                hid_ptr,
                Ordering::SeqCst,
                Ordering::Relaxed,
            )
            .is_err()
        {
            panic!("Attempt to multiply-initialize USB {:?}", cname);
        }

        hid
    }

    pub async fn send_keyboard_report(&self, mods: u8, keys: &[u8]) {
        if keys.len() > 6 {
            // Ignore ones that have too many keys down?
            return;
        }

        let mut report = [0u8; 8];
        report[0] = mods;
        for (i, key) in keys.iter().enumerate() {
            report[i + 2] = *key;
        }

        let mut state = self.hid0.state.lock_async().await.unwrap();

        if state.ready {
            // We can directly send it.  We have the mutex which avoids the race with it getting
            // sent immediately.
            unsafe {
                raw::hid_int_ep_write(
                    self.hid0.device,
                    report.as_ptr(),
                    report.len() as u32,
                    ptr::null_mut(),
                );
            }
            state.ready = false;
        } else {
            // Queue it up to be sent as the prior reports are read.
            state.additional.push_back(report.to_vec());
        }
    }

    /// Read a HID out report from the keyboard, or None, if there is none available.
    /// TODO: We really want to be able to sleep on this, or have it send an event, but for now,
    /// polling should at least keep the keyboard from freezing.
    pub fn get_keyboard_report(&self, data: &mut [u8]) -> Result<Option<usize>> {
        if self.hid0.out_sem.take(NoWait).is_ok() {
            let mut count: u32 = 0;
            unsafe {
                to_result_void(raw::hid_int_ep_read(
                    self.hid0.device,
                    data.as_mut_ptr(),
                    data.len() as u32,
                    &mut count,
                ))?;
            }

            Ok(Some(count as usize))
        } else {
            Ok(None)
        }
    }

    pub fn send_plover_report(&self, report: &[u8]) {
        let mut state = self.hid1.state.lock().unwrap();

        // Todo, this is repeated, perhaps in the HidWrap as a method.
        if state.ready {
            unsafe {
                raw::hid_int_ep_write(
                    self.hid1.device,
                    report.as_ptr(),
                    report.len() as u32,
                    ptr::null_mut(),
                );
            }
            state.ready = false;
        } else {
            state.additional.push_back(report.to_vec());
        }
    }

    // TODO: Ideally, some minder protocols should be able to be dropped if the queue gets too
    // large, so that should probably be an argument here.
    #[allow(dead_code)]
    pub fn send_minder_report(&self, report: &[u8]) {
        let mut state = self.hid2.state.lock().unwrap();

        // Todo, this is repeated, perhaps in the HidWrap as a method.
        if state.ready {
            unsafe {
                info!("Send report {:02x?}", report);
                raw::hid_int_ep_write(
                    self.hid2.device,
                    report.as_ptr(),
                    report.len() as u32,
                    ptr::null_mut(),
                );
            }
            state.ready = false;
        } else {
            state.additional.push_back(report.to_vec());
        }
    }

    /// Try reading a minder packet.  Might return a timeout if the timeout isn't met.
    #[allow(dead_code)]
    pub fn minder_read_out<T>(&self, timeout: T, buf: &mut [u8]) -> Result<usize>
    where
        T: Into<Timeout>,
    {
        self.hid2.out_sem.take(timeout)?;

        let mut count: u32 = 0;
        unsafe {
            to_result_void(raw::hid_int_ep_read(
                self.hid2.device,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut count,
            ))?;
        }

        Ok(count as usize)
    }
}

// For now, go ahead and just allocate for events that are too large.  They aren't really
// frequent enough for this to be too much of a concern, and allocation will certainly be better
// than copying around a 64-byte ArrayDeque.
/// The shared data for a hid in endpoint.  The queueing is a bit unusual here.  The driver is
/// able to hold one event queued, and then will inform us when that event has been read, and
/// that it is ready for a new event.
struct HidIn {
    /// Is the driver's endpoint empty?
    ready: bool,
    /// Additional events to send.
    additional: VecDeque<Vec<u8>>,
}

/// The outer wrapper holds the device (which will be constant) and the Mutex (and possibly a
/// Condvar later) to be able to match these without having to take each Mutex.
struct HidWrap {
    device: *const raw::device,
    out_sem: Semaphore,
    state: Mutex<HidIn>,
}

// There is a raw device that keeps this from automatically being Send, so just allow that.
unsafe impl Send for HidWrap {}
unsafe impl Sync for HidWrap {}

static HID0: AtomicPtr<HidWrap> = AtomicPtr::new(ptr::null_mut());
static HID1: AtomicPtr<HidWrap> = AtomicPtr::new(ptr::null_mut());
static HID2: AtomicPtr<HidWrap> = AtomicPtr::new(ptr::null_mut());

static USB_OPS: raw::hid_ops = raw::hid_ops {
    get_report: None,
    int_in_ready: Some(hid_in_ready),
    int_out_ready: Some(hid_out_ready),
    on_idle: None,
    protocol_change: None,
    set_report: None,
};

// Note that this is called from a USB worker thread.  There might be concerns about stack, but
// it should be safe to allocate/deallocate.
extern "C" fn hid_in_ready(device: *const raw::device) {
    if check_hid_in_ready(device, &HID0) {
        return;
    }
    if check_hid_in_ready(device, &HID1) {
        return;
    }
    if check_hid_in_ready(device, &HID2) {
        return;
    }
    panic!("hid callback from unknown device");
}

// Handle the endpoint returning true if this is the right device.
fn check_hid_in_ready(device: *const raw::device, global: &AtomicPtr<HidWrap>) -> bool {
    let wrap = global.load(Ordering::Acquire);
    if wrap.is_null() {
        panic!("USB callback before initializatoin");
    }
    let wrap = unsafe { &*wrap };
    if device != wrap.device {
        return false;
    }
    let mut state = wrap.state.lock().unwrap();

    if state.ready {
        warn!("in_ready callback while already ready");
        // But, it did "handle it".
        return true;
    }

    // If we have more data to send, just send it.
    if let Some(report) = state.additional.pop_front() {
        // This should never block, as long as we manage the state properly.  Presumably it is
        // safe to call this from the callback?
        unsafe {
            raw::hid_int_ep_write(
                device,
                report.as_ptr(),
                report.len() as u32,
                ptr::null_mut(),
            );
        }
    } else {
        // Otherwise, indicate ready, so the next send will go here.
        state.ready = true;
    }

    true
}

extern "C" fn hid_out_ready(device: *const raw::device) {
    if check_hid_out_ready(device, &HID0) {
        return;
    }
    if check_hid_out_ready(device, &HID1) {
        return;
    }
    if check_hid_out_ready(device, &HID2) {
        return;
    }
    panic!("hid out callback from unknown device");
}

fn check_hid_out_ready(device: *const raw::device, global: &AtomicPtr<HidWrap>) -> bool {
    let wrap = global.load(Ordering::Acquire);
    if wrap.is_null() {
        panic!("USB callback before initialization");
    }
    let wrap = unsafe { &*wrap };
    if device != wrap.device {
        return false;
    }

    wrap.out_sem.give();

    return true;
}

extern "C" fn status_cb(status: raw::usb_dc_status_code, _param: *const u8) {
    // There is some slightly redundant use of types here.
    match status {
        raw::usb_dc_status_code_USB_DC_CONFIGURED => rust_usb_status(0),
        raw::usb_dc_status_code_USB_DC_SUSPEND => rust_usb_status(1),
        raw::usb_dc_status_code_USB_DC_RESUME => rust_usb_status(2),
        _ => (),
    }
}

#[repr(C)]
struct U8Vec {
    base: *const u8,
    len: usize,
}

extern "C" {
    fn hid_get_kbd_desc() -> U8Vec;
}

/// Plover HID descriptor.
///
/// Defined at https://github.com/dnaq/plover-machine-hid
static PLOVER_HID_DESC: [u8; 25] = [
    0x06, 0x50, 0xff, // UsagePage (65360)
    0x0a, 0x56, 0x4c, // Usage (19542)
    0xa1, 0x02, // Collection (Logical)
    0x85, 0x50, //     ReportID (80)
    0x25, 0x01, //     LogicalMaximum (1)
    0x75, 0x01, //     ReportSize (1)
    0x95, 0x40, //     ReportCount (64)
    0x05, 0x0a, //     UsagePage (ordinal)
    0x19, 0x00, //     UsageMinimum (Ordinal(0))
    0x29, 0x3f, //     UsageMaximum (Ordinal(63))
    0x81, 0x02, //     Input (Variable)
    0xc0, // EndCollection
];

/// Minder HID descriptor.
///
/// Generated by ChatGPT, with comments.
static MINDER_HID_DESC: [u8; 35] = [
    0x06, 0x4d, 0xFF, // Usage Page (Vendor Defined 0xFF4D)
    0x0a, 0x4e, 0x44, // Usage (Vendor Defined Usage 0x4E44)
    0xA1, 0x02, // Collection (Application)
    // Input Report (64 bytes)
    0x09, 0x02, // Usage (Vendor Defined Usage 0x02)
    // 0x85, 0x01,
    0x15, 0x00, // Logical Minimum (0)
    0x26, 0xFF, 0x00, // Logical Maximum (255)
    0x75, 0x08, // Report Size (8 bits)
    0x95, 0x40, // Report Count (64)
    0x81, 0x02, // Input (Data, Var, Abs)
    // Output Report (64 bytes)
    0x09, 0x03, // Usage (Vendor Defined Usage 0x03)
    0x15, 0x00, // Logical Minimum (0)
    0x26, 0xFF, 0x00, // Logical Maximum (255)
    0x75, 0x08, // Report Size (8 bits)
    0x95, 0x40, // Report Count (64)
    0x91, 0x02, // Output (Data, Var, Abs)
    0xC0, // End Collection
];
