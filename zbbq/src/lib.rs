#![no_std]

extern crate alloc;

use core::{panic::PanicInfo, slice};

use alloc::format;
use zephyr_sys::{ZephyrAllocator, Device, LedRgb, LedStripDriver};

use crate::zephyr::message;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe {
        zephyr_sys::c_k_panic();
    }
}

#[global_allocator]
static ZEPHYR_ALLOCATOR: ZephyrAllocator = ZephyrAllocator;

pub type Result<T> = core::result::Result<T, Error>;
#[derive(Debug)]
pub enum Error {
    Missing,
}

#[no_mangle]
fn rust_main() {
    message("Beginning of Rust.");

    let strip = Device::get_led_strip().expect("Getting led strip device");
    message(&format!("Device present {}", strip.is_ready()));
    message(&format!("LED device name: {:?}", strip.name()));

    let mut strip = unsafe { LedStripDriver::unsafe_from_device(strip) };

    // Program the LED just as an example.
    let leds = [
        LedRgb { r: 25, g: 25, b: 0 },
        LedRgb { r: 0, g: 25, b: 25 },
        LedRgb { r: 25, g: 0, b: 25 },
    ];
    loop {
        for led in &leds {
            strip.update_rgb(slice::from_ref(led));
            zephyr::sleep(500);
        }
    }

    // loop {}
}

mod zephyr {
    use alloc::ffi::CString;
    use crate::zephyr_sys;

    // Print a basic message from a Rust string.  This isn't particularly
    // efficient, as the message will be heap allocated (and then immediately
    // freed).
    pub fn message(text: &str) {
        let text = CString::new(text).expect("CString::new failed");
        unsafe {
            crate::zephyr_sys::msg_string(text.as_ptr());
        }
    }

    // This is sort of safe.  There could be a time overflow, but it is memory
    // safe, etc.
    pub fn sleep(ms: u32) {
        unsafe { zephyr_sys::c_k_sleep_ms(ms); }
    }

}

mod zephyr_sys {
    use core::{alloc::{GlobalAlloc, Layout}, ffi::{c_char, c_int, CStr}};

    use alloc::{string::{String, ToString}, alloc::handle_alloc_error};

    use crate::{Error, Result};

    extern "C" {
        pub fn c_k_panic() -> !;

        pub fn malloc(size: c_size_t) -> *mut u8;
        // pub fn realloc(ptr: *mut u8, size: c_size_t) -> *mut u8;
        pub fn free(ptr: *mut u8);

        // Log this message.
        pub fn msg_string(text: *const c_char);

        // Device operations.
        pub fn sys_device_is_ready(dev: *const ZDevice) -> c_int;

        // Query for specific device.
        pub fn get_led_strip() -> *mut ZDevice;

        // Sleep in ms.
        pub fn c_k_sleep_ms(ms: u32);
    }

    // The Underlying Zephyr `struct device`.
    #[repr(C)]
    pub struct ZDevice {
        name: *const c_char,
        config: *const u8,
        api: *const u8,
        state: *mut u8,
        data: *mut u8,
        #[cfg(zephyr = "CONFIG_DEVICE_DEPS")]
        deps: *const u8,
        // PM stuff.
    }

    // A Zephyr device.  To provide Rust ownership semantics, this struct keeps
    // a private pointer to the Zephyr-side struct, which presumably has a
    // static lifetime.
    pub struct Device {
        dev: *mut ZDevice,
    }

    impl Device {
        pub fn get_led_strip() -> Result<Device> {
            let dev = unsafe { get_led_strip() };
            if dev.is_null() {
                return Err(Error::Missing);
            }
            Ok(Device { dev })
        }

        pub fn is_ready(&self) -> bool {
            unsafe { sys_device_is_ready(self.dev) != 0 }
        }

        // Get the name of the device from the device struct.
        pub fn name(&self) -> String {
            // Ick, probably not how we want to do this.
            unsafe {
                let dev = self.dev as *const ZDevice;
                let name = CStr::from_ptr((*dev).name);
                String::from_utf8_lossy(name.to_bytes()).to_string()
            }
        }
    }

    // Simple binding to the rgb scratch API.
    #[repr(C)]
    pub struct LedRgb {
        #[cfg(zephyr = "CONFIG_LED_STRIP_RGB_SCRATCH")]
        scratch: u8,
        pub r: u8,
        pub g: u8,
        pub b: u8,
    }

    #[repr(C)]
    struct LedStripDriverApi {
        update_rgb: extern "C" fn(dev: *const ZDevice,
                                  pixels: *const LedRgb,
                                  num_pixels: c_size_t) -> c_int,
        update_channels: extern "C" fn (dev: *const ZDevice,
                                        channels: *const u8,
                                        num_channels: c_size_t) -> c_int,
    }

    // An led strip driver.
    pub struct LedStripDriver {
        dev: *mut ZDevice,
    }

    impl LedStripDriver {
        pub unsafe fn unsafe_from_device(dev: Device) -> LedStripDriver {
            LedStripDriver { dev: dev.dev }
        }

        // The API.
        pub fn update_rgb(&mut self, pixels: &[LedRgb]) -> c_int {
            unsafe {
                let api = (*self.dev).api as *const LedStripDriverApi;
                ((*api).update_rgb)(self.dev, pixels.as_ptr(), pixels.len())
            }
        }
    }

    pub struct ZephyrAllocator;

    unsafe impl GlobalAlloc for ZephyrAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let size = layout.size();
            let align = layout.align();

            // The picolibc/newlib malloc has an alignment of 8.  Any more than
            // this requires memalign, which isn't efficient.
            if align > 8 {
                handle_alloc_error(layout);
                // panic!("ZephyrAllocator, attempt at large alignment: {}", align);
            }

            malloc(size)
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            free(ptr);
        }

        // TODO: realloc might make sense with alignment <= 8.
    }

    // Define locally, as this is experimental.
    #[allow(non_camel_case_types)]
    pub type c_size_t = usize;
}
