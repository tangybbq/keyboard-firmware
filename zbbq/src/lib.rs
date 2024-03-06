#![no_std]

extern crate alloc;

use core::{panic::PanicInfo, slice};

use alloc::{format, vec};
use rand::{SeedableRng, RngCore};
use rand_xoshiro::Xoroshiro128StarStar;
use zephyr_sys::{ZephyrAllocator, Device, LedRgb, LedStripDriver, GpioFlags, Timer};

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
    GPIO,
}

#[no_mangle]
extern "C" fn rust_main(_: *const u8, _: *const u8, _: *const u8) {
    message("Beginning of Rust.");

    // message(&format!("User: {}", zephyr_sys::arch_is_user_context()));

    let strip = Device::get_led_strip().expect("Getting led strip device");
    message(&format!("Device present {}", strip.is_ready()));
    message(&format!("LED device name: {:?}", strip.name()));

    // Query the matrix.
    let (rows, cols) = zephyr_sys::get_matrix();
    message(&format!("rows: {}", rows.len()));
    for row in rows {
        message(&format!("  pin:{} flag:{} ready:{}", row.pin, row.flags,
                         row.is_ready()));
    }
    message(&format!("cols: {}", cols.len()));
    for col in cols {
        message(&format!("  pin:{} flag:{} ready:{}", col.pin, col.flags,
                         col.is_ready()));
    }


    // Throw together a matrix scan.
    if false {
        let mut scan_state = vec![Debouncer::new(); 2 * cols.len() * rows.len()];
        loop {
            // And the rows as inputs.
            for row in rows {
                row.pin_configure(GpioFlags::GPIO_INPUT).unwrap();
            }

            // Configure the columns as outputs, driving low/high.
            for col in cols {
                col.pin_configure(GpioFlags::GPIO_OUTPUT_INACTIVE).unwrap();
            }
            zephyr::sleep(1);

            let mut scanner = scan_state.iter_mut().enumerate();
            for col in cols {
                col.pin_set(true).unwrap();
                for row in rows {
                    let (key, state) = scanner.next().unwrap();
                    match state.react(row.pin_get().unwrap()) {
                        KeyAction::None => (),
                        KeyAction::Press => {
                            message(&format!("{} Press", key));
                        }
                        KeyAction::Release => {
                            message(&format!("{} Release", key));
                        }
                    }
                }
                col.pin_set(false).unwrap();
            }

            // Flip the driven around, and scan again from rows to columns.

            // Configure the columns as inputs.
            for col in cols {
                col.pin_configure(GpioFlags::GPIO_INPUT).unwrap();
            }

            // And the rows as inputs.
            for row in rows {
                row.pin_configure(GpioFlags::GPIO_OUTPUT_INACTIVE).unwrap();
            }
            zephyr::sleep(1);

            for row in rows {
                row.pin_set(true).unwrap();
                for col in cols {
                    let (key, state) = scanner.next().unwrap();
                    match state.react(col.pin_get().unwrap()) {
                        KeyAction::None => (),
                        KeyAction::Press => {
                            message(&format!("{} Press", key));
                        }
                        KeyAction::Release => {
                            message(&format!("{} Release", key));
                        }
                    }
                }
                row.pin_set(false).unwrap();
            }

            if false {
                break;
            }
        }
    }

    let mut strip = unsafe { LedStripDriver::unsafe_from_device(strip) };

    // Fixed pattern demo.
    if false {
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
    }

    let mut ticker = unsafe { Timer::new(&mut zephyr_sys::ms_timer, 500) };

    // Random colors.
    if true {
        let mut led = [LedRgb::default(); 1];

        // TODO, we could wrap the Zephyr api, or just use the rust crate.
        // Depends a bit on what else Zephyr might be using.
        let mut rng = Xoroshiro128StarStar::seed_from_u64(0);

        loop {
            for i in 0..led.len() {
                led[i].r = (rng.next_u32() >> 12 & 7) as u8;
                led[i].g = (rng.next_u32() >> 12 & 7) as u8;
                led[i].b = (rng.next_u32() >> 12 & 7) as u8;
            }
            strip.update_rgb(&led);
            ticker.wait();
            // zephyr::sleep(500);
        }
    }
    // ticker.stop();

    // loop {}
}

mod zephyr {
    use alloc::ffi::CString;
    use crate::zephyr_sys;

    // The passing of flags to rust is somewhat fragile, and things like having
    // RUST_FLAGS set in the environment will override any flags that are being
    // set in the cargo config.  To catch this quickly, fail right away if the
    // CONFIG_RUST flag isn't defined.
    #[cfg(not(CONFIG_RUST))]
    compile_error!("CONFIG_RUST not defined.  Crate must be built as part of Zehyr");

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

pub mod zephyr_sys {
    use core::{alloc::{GlobalAlloc, Layout}, ffi::{c_char, c_int, CStr}};
    use core::arch::asm;

    use alloc::{string::{String, ToString}, alloc::handle_alloc_error};
    use bitflags::bitflags;

    use crate::{Error, Result};

    extern "C" {
        pub fn c_k_panic() -> !;

        fn malloc(size: c_size_t) -> *mut u8;
        // pub fn realloc(ptr: *mut u8, size: c_size_t) -> *mut u8;
        fn free(ptr: *mut u8);

        // Log this message.
        pub fn msg_string(text: *const c_char);

        // Device operations.
        fn sys_device_is_ready(dev: *const ZDevice) -> c_int;

        // Query for specific device.
        pub fn get_led_strip() -> *mut ZDevice;

        // Sleep in ms.
        pub fn c_k_sleep_ms(ms: u32);

        // Get the matrix.
        fn get_matrix_info() -> MatrixInfo;

        // GPIO pin configuration.
        fn sys_gpio_pin_configure(port: *const ZDevice,
                                  pin: u8,
                                  flags: u32) -> c_int;

        fn sys_gpio_pin_get(port: *const ZDevice, pin: u8) -> c_int;
        fn sys_gpio_pin_set(port: *const ZDevice, pin: u8, value: c_int) -> c_int;

        // Arm Cortex-M syscall interface.
        fn z_arm_thread_is_in_user_mode() -> c_int;

        // Timer initialization.
        fn k_timer_init(timer: *mut KTimer,
                        expiry_fn: Option<KTimerExpiry>,
                        stop_fn: Option<KTimerStop>);

        // Start the timer (syscall).
        fn sys_k_timer_start(timer: *mut KTimer,
                             duration: KTimeout,
                             period: KTimeout);

        fn sys_k_timer_stop(timer: *mut KTimer);

        // Wait for the timer to have ticked.
        fn sys_k_timer_status_sync(timer: *mut KTimer);

        pub static mut ms_timer: KTimer;
    }

    /// A higher level periodic timer.  The timer will fire ever n miliseconds.
    pub struct Timer {
        timer: *mut KTimer,
    }

    impl Timer {
        pub unsafe fn new(timer: *mut KTimer, interval: u64) -> Timer {
            k_timer_init(timer, None, None);
            let ticks = KTimeout { ticks: interval * 10 };
            sys_k_timer_start(timer, ticks, ticks);
            Timer {timer}
        }

        pub fn wait(&mut self) {
            unsafe {
                sys_k_timer_status_sync(self.timer);
            }
        }

        pub fn stop(&mut self) {
            unsafe {
                sys_k_timer_stop(self.timer);
            }
        }
    }

    #[cfg(CONFIG_TIMEOUT_64BIT)]
    pub type KTicks = u64;
    #[cfg(not(CONFIG_TIMEOUT_64BIT))]
    pub type KTicks = u32;

    pub const K_TICKS_FOREVER: KTicks = !0;

    #[repr(C)]
    #[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
    pub struct KTimeout {
        ticks: KTicks,
    }

    pub const Z_TIMEOUT_NO_WAIT: KTimeout = KTimeout { ticks: 0 };
    pub const Z_FOREVER: KTimeout = KTimeout { ticks: K_TICKS_FOREVER };

    #[cfg(CONFIG_TRACING)]
    compile_error!("CONFIG_TRACING not yet supported within Rusr.");

    /// The internal timer structure.  This is really opaque.  In userspace, it
    /// isn't accessible anyway.
    type KTimer = u32;

    type KTimerExpiry = extern "C" fn (timer: *mut KTimer);
    type KTimerStop = extern "C" fn (timer: *mut KTimer);

    /// Cortex-M syscalls.  Rust implementation of arch_is_user_context().
    #[allow(dead_code)]
    pub fn arch_is_user_context() -> bool {
        // TODO: Cond for CONFIG_CPU_CORTEX_M.
        let mut value: u32;
        unsafe {
            asm!(
                "mrs {}, IPSR",
                out(reg) value,
                options(pure, nomem, nostack),
            );
        }
        if value != 0 {
            return false;
        }

        unsafe {
            z_arm_thread_is_in_user_mode() != 0
        }
    }

    bitflags! {
        pub struct GpioFlags: u32 {
            const GPIO_INPUT = 1 << 16;
            const GPIO_OUTPUT = 1 << 17;
            const GPIO_OUTPUT_INIT_LOW = 1 << 18;
            const GPIO_OUTPUT_INIT_HIGH = 1 << 19;
            const GPIO_OUTPUT_INIT_LOGICAL = 1 << 20;

            const GPIO_OUTPUT_LOW = Self::GPIO_OUTPUT.bits() | Self::GPIO_OUTPUT_INIT_LOW.bits();
            const GPIO_OUTPUT_HIGH = Self::GPIO_OUTPUT.bits() | Self::GPIO_OUTPUT_INIT_HIGH.bits();
            const GPIO_OUTPUT_INACTIVE =
                (Self::GPIO_OUTPUT.bits() |
                 Self::GPIO_OUTPUT_INIT_LOW.bits() |
                 Self::GPIO_OUTPUT_INIT_LOGICAL.bits());
            const GPIO_OUTPUT_ACTIVE =
                (Self::GPIO_OUTPUT.bits() |
                 Self::GPIO_OUTPUT_INIT_HIGH.bits() |
                 Self::GPIO_OUTPUT_INIT_LOGICAL.bits());

            // TODO: Add the interrupt signals
        }
    }

    #[repr(C)]
    struct MatrixInfo {
        rows: *const ZGpioDtSpec,
        nrows: u32,
        cols: *const ZGpioDtSpec,
        ncols: u32,
    }

    pub fn get_matrix() -> (&'static [ZGpioDtSpec], &'static [ZGpioDtSpec]) {
        unsafe {
            let info = get_matrix_info();
            (core::slice::from_raw_parts(info.rows, info.nrows as usize),
             core::slice::from_raw_parts(info.cols, info.ncols as usize))
        }
    }

    #[repr(C)]
    pub struct ZGpioDtSpec {
        port: *const ZDevice,
        pub pin: u8,  // TODO: Keep types
        pub flags: u16, // TODO: Keep types
    }

    impl ZGpioDtSpec {
        pub fn is_ready(&self) -> bool {
            (unsafe { sys_device_is_ready(self.port) }) != 0
        }

        pub fn pin_configure(&self, flags: GpioFlags) -> Result<()> {
            if unsafe {
                sys_gpio_pin_configure(self.port, self.pin, self.flags as u32 | flags.bits())
            } != 0 {
                return Err(Error::GPIO);
            }
            Ok(())
        }

        pub fn pin_get(&self) -> Result<bool> {
            match unsafe {sys_gpio_pin_get(self.port, self.pin)} {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(Error::GPIO),
            }
        }

        // TODO: This really should be `mut`, but that isn't how the C API is written.
        pub fn pin_set(&self, value: bool) -> Result<()> {
            match unsafe {sys_gpio_pin_set(self.port, self.pin, if value { 1 } else { 0 })} {
                0 => Ok(()),
                _ => Err(Error::GPIO),
            }
        }
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
    #[derive(Default, Clone, Copy)]
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

/// Individual state tracking.
#[derive(Clone, Copy, Eq, PartialEq)]
enum KeyState {
    /// Key is in released state.
    Released,
    /// Key is in pressed state.
    Pressed,
    /// We've seen a release edge, and will consider it released when consistent.
    DebounceRelease,
    /// We've seen a press edge, and will consider it pressed when consistent.
    DebouncePress,
}

#[derive(Clone, Copy)]
enum KeyAction {
    None,
    Press,
    Release,
}

// Don't really want Copy, but needed for init.
#[derive(Clone, Copy)]
struct Debouncer {
    /// State for this key.
    state: KeyState,
    /// Count how many times we've seen a given debounce state.
    counter: usize,
}

const DEBOUNCE_COUNT: usize = 20;

impl Debouncer {
    fn new() -> Debouncer {
        Debouncer {
            state: KeyState::Released,
            counter: 0,
        }
    }

    fn react(&mut self, pressed: bool) -> KeyAction {
        match self.state {
            KeyState::Released => {
                if pressed {
                    self.state = KeyState::DebouncePress;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::Pressed => {
                if !pressed {
                    self.state = KeyState::DebounceRelease;
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::DebounceRelease => {
                if pressed {
                    // Reset the counter any time we see a press state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Released;
                        KeyAction::Release
                    } else {
                        KeyAction::None
                    }
                }
            }
            // TODO: We could probably just do two states, and a press/released flag.
            KeyState::DebouncePress => {
                if !pressed {
                    // Reset the counter any time we see a released state.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Pressed;
                        KeyAction::Press
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn is_pressed(&self) -> bool {
        self.state == KeyState::Pressed || self.state == KeyState::DebounceRelease
    }
}
