//! WS2812 strip support, over Zephyr's LED strip driver.
//!
//! `led_strip_update_rgb` blocks for the whole frame, the reset delay included, so the strip is
//! written from a thread of its own, as jolt-embassy-rp writes it from a task of its own: the
//! main thread runs the scanner, the layout and USB, and an LED write must not hold up any of
//! them.  [`LedStripGroup::update`] leaves the frame for that thread and wakes it.  A frame that
//! arrives before the last one has been written replaces it, which is all an indicator needs.

use core::cell::RefCell;

use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use heapless::Vec;
use smart_leds::RGB8;
use zephyr::{
    device::led_strip::{LedRgb, LedStrip},
    printkln,
    sys::sync::Semaphore,
    time::Forever,
};

use super::{LedGroup, Shade};

/// The longest strip supported.  The boards in use have four.
const MAX_PIXELS: usize = 8;

const LED_THREAD_STACK_SIZE: usize = 1024;

/// Below the main thread, which runs everything that has a deadline.
const LED_THREAD_PRIO: core::ffi::c_int = 6;

/// The frame waiting to be written, if any.
static LATEST: Mutex<CriticalSectionRawMutex, RefCell<Option<Vec<RGB8, MAX_PIXELS>>>> =
    Mutex::new(RefCell::new(None));

/// Given when there is a frame in [`LATEST`].  A limit of one, so that any number of updates
/// before the thread runs wake it once.
static WAKE: Semaphore = Semaphore::new(0, 1);

/// The handle the LED manager writes to.  The strip itself belongs to the thread.
pub struct LedStripGroup {
    len: usize,
}

impl LedGroup for LedStripGroup {
    fn len(&self) -> usize {
        self.len
    }

    fn update(&mut self, values: &[Shade]) {
        let frame = values.iter().map(|shade| shade.color).collect();
        LATEST.lock(|latest| *latest.borrow_mut() = Some(frame));
        WAKE.give();
    }

    fn needs_refresh(&self) -> bool {
        true
    }
}

impl LedStripGroup {
    /// Find the chosen `bbq,led-strip`, and start the thread that writes it.
    pub fn get_instance() -> Option<Self> {
        let strip = zephyr::devicetree::chosen::bbq_led_strip::get_instance()?;
        if !strip.is_ready() {
            printkln!("LED strip is not ready");
            return None;
        }
        let len = strip.length();
        if len > MAX_PIXELS {
            printkln!(
                "LED strip has {} pixels, only {} supported",
                len,
                MAX_PIXELS
            );
            return None;
        }

        let thread = led_strip_thread(strip);
        thread.set_priority(LED_THREAD_PRIO);
        let _thread = thread.start();

        Some(Self { len })
    }
}

#[zephyr::thread(stack_size = LED_THREAD_STACK_SIZE)]
fn led_strip_thread(mut strip: LedStrip) -> ! {
    loop {
        // A take with no timeout only returns once given.
        let _ = WAKE.take(Forever);
        let Some(frame) = LATEST.lock(|latest| latest.borrow_mut().take()) else {
            continue;
        };
        let mut pixels: Vec<LedRgb, MAX_PIXELS> = frame
            .iter()
            .map(|color| LedRgb {
                r: color.r,
                g: color.g,
                b: color.b,
            })
            .collect();
        if let Err(err) = strip.update_rgb(&mut pixels) {
            printkln!("LED strip update failed: {:?}", err);
        }
    }
}
