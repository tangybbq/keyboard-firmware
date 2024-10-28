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

    use core::{ptr, sync::atomic::Ordering};

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
        hid0: Arc<Mutex<HidIn>>,
    }

    impl Usb {
        pub fn new() -> Result<Usb> {
            let hid0_dev = unsafe { raw::device_get_binding(c"HID_0".as_ptr()) };
            if hid0_dev.is_null() {
                error!("Cannot get USB HID 0 device");
                return Err(Error(raw::ENODEV));
            }

            // Setup our shared data before it is needed.
            let hid0 = Arc::new(Mutex::new(HidIn {
                device: hid0_dev,
                ready: true,
                additional: VecDeque::new(),
            }));

            // Just store the pointer.  TODO: This would actually be a good place to detect multiple
            // initialization.
            let hid0_ptr = Arc::into_raw(hid0.clone()) as *mut _;

            if HID0
                .compare_exchange(
                    ptr::null_mut(),
                    hid0_ptr,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                error!("Attempt at multiple initialization of USB");
                return Err(Error(raw::EINVAL));
            }

            let kbd_desc = unsafe { hid_get_kbd_desc() };
            unsafe {
                raw::usb_hid_register_device(hid0_dev, kbd_desc.base, kbd_desc.len, &USB_OPS);

                raw::usb_hid_init(hid0_dev);
                if raw::usb_enable(Some(status_cb)) != 0 {
                    error!("Failed to enable USB");
                    return Err(Error(raw::ENODEV));
                }
            }

            Ok(Usb { hid0 })
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

            let mut state = self.hid0.lock().unwrap();

            if state.ready {
                // We can directly send it.  We have the mutex which avoids the race with it getting
                // sent immediately.
                unsafe {
                    raw::hid_int_ep_write(
                        state.device,
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
    }

    // For now, go ahead and just allocate for events that are too large.  They aren't really
    // frequent enough for this to be too much of a concern, and allocation will certainly be better
    // than copying around a 64-byte ArrayDeque.
    /// The shared data for a hid in endpoint.  The queueing is a bit unusual here.  The driver is
    /// able to hold one event queued, and then will inform us when that event has been read, and
    /// that it is ready for a new event.
    struct HidIn {
        /// The device we are concerned with.
        device: *const raw::device,
        /// Is the driver's endpoint empty?
        ready: bool,
        /// Additional events to send.
        additional: VecDeque<Vec<u8>>,
    }

    static HID0: AtomicPtr<Mutex<HidIn>> = AtomicPtr::new(ptr::null_mut());

    static USB_OPS: raw::hid_ops = raw::hid_ops {
        get_report: None,
        int_in_ready: Some(hid0_in_ready),
        int_out_ready: None,
        on_idle: None,
        protocol_change: None,
        set_report: None,
    };

    // Note that this is called from a USB worker thread.  There might be concerns about stack, but
    // it should be safe to allocate/deallocate.
    extern "C" fn hid0_in_ready(device: *const raw::device) {
        let state = HID0.load(Ordering::Acquire);
        let state = unsafe { &*state };
        let mut state = state.lock().unwrap();

        if device != state.device {
            panic!("hid0_in_ready called with wrong callback");
        }

        if state.ready {
            warn!("in_ready callback while already ready");
            return;
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
}
