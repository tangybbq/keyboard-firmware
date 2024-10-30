//! Device management
//!
//! Management of the various devices used in the keyboards.  Some are just direct types from
//! Zephyr, and others are wrapped.

pub mod leds {
    use bbq_keyboard::RGB8;
    use zephyr::raw::led_rgb;

    // Wrap the Zephyr rgb indicator.
    #[derive(Copy, Clone)]
    pub struct LedRgb(pub led_rgb);

    // TODO: There might be an additional field depend on configs.
    impl Default for LedRgb {
        fn default() -> Self {
            LedRgb::new(0, 0, 0)
        }
    }

    impl LedRgb {
        pub const fn new(r: u8, g: u8, b: u8) -> LedRgb {
            LedRgb(led_rgb { r, g, b })
        }

        pub fn to_rgb8(self) -> RGB8 {
            RGB8::new(self.0.r, self.0.g, self.0.b)
        }
    }
}

pub mod usb {
    //! Zephyr USB interface.
    //!
    //! This interfaces directly with the USB stack.  As this is not very general, we just use the
    //! unsafe entries directly.

    use core::{ffi::CStr, ptr, sync::atomic::Ordering};

    use alloc::{collections::vec_deque::VecDeque, vec::Vec};
    use log::{error, warn};
    use zephyr::{
        raw,
        sync::{atomic::AtomicPtr, Arc, Mutex},
        Error, Result,
    };

    use crate::rust_usb_status;

    /// There is a single instance of the USB system.  As this is somewhat unsafe, we'll just
    /// require the caller to create only a single instance of this (for now).
    pub struct Usb {
        hid0: Arc<HidWrap>,
        hid1: Arc<HidWrap>,
    }

    impl Usb {
        pub fn new() -> Result<Usb> {
            let hid0 = Self::setup_hid(c"HID_0", &HID0);
            let hid1 = Self::setup_hid(c"HID_1", &HID1);

            let kbd_desc = unsafe { hid_get_kbd_desc() };
            unsafe {
                raw::usb_hid_register_device(hid0.device, kbd_desc.base, kbd_desc.len, &USB_OPS);
                raw::usb_hid_init(hid0.device);

                raw::usb_hid_register_device(hid1.device,
                                             PLOVER_HID_DESC.as_ptr(),
                                             PLOVER_HID_DESC.len(),
                                             &USB_OPS);
                raw::usb_hid_init(hid1.device);

                if raw::usb_enable(Some(status_cb)) != 0 {
                    error!("Failed to enable USB");
                    return Err(Error(raw::ENODEV));
                }
            }

            Ok(Usb { hid0, hid1 })
        }

        fn setup_hid(cname: &CStr, global: &AtomicPtr<HidWrap>) -> Arc<HidWrap> {
            let dev = unsafe { raw::device_get_binding(cname.as_ptr()) };
            if dev.is_null() {
                panic!("Cannot get USB {:?} device", cname);
            }

            let hid = Arc::new(HidWrap {
                device: dev,
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

        pub fn send_keyboard_report(&self, mods: u8, keys: &[u8]) {
            if keys.len() > 6 {
                // Ignore ones that have too many keys down?
                return;
            }

            let mut report = [0u8; 8];
            report[0] = mods;
            for (i, key) in keys.iter().enumerate() {
                report[i + 2] = *key;
            }

            let mut state = self.hid0.state.lock().unwrap();

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
        state: Mutex<HidIn>,
    }

    static HID0: AtomicPtr<HidWrap> = AtomicPtr::new(ptr::null_mut());
    static HID1: AtomicPtr<HidWrap> = AtomicPtr::new(ptr::null_mut());

    static USB_OPS: raw::hid_ops = raw::hid_ops {
        get_report: None,
        int_in_ready: Some(hid_in_ready),
        int_out_ready: None,
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
        0x06, 0x50, 0xff,              // UsagePage (65360)
        0x0a, 0x56, 0x4c,              // Usage (19542)
        0xa1, 0x02,                    // Collection (Logical)
        0x85, 0x50,                    //     ReportID (80)
        0x25, 0x01,                    //     LogicalMaximum (1)
        0x75, 0x01,                    //     ReportSize (1)
        0x95, 0x40,                    //     ReportCount (64)
        0x05, 0x0a,                    //     UsagePage (ordinal)
        0x19, 0x00,                    //     UsageMinimum (Ordinal(0))
        0x29, 0x3f,                    //     UsageMaximum (Ordinal(63))
        0x81, 0x02,                    //     Input (Variable)
        0xc0,                          // EndCollection
    ];
}
