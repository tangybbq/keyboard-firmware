//! Led strip support
//!
//! This supports managing a single ws2812 instance.  The updates happen from an async task.

use embassy_rp::{pio::Instance, pio_programs::ws2812::PioWs2812};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use heapless::Vec;
use smart_leds::RGB8;

use super::{LedGroup, MAX_GROUP_SIZE};

// On the rp2040, the LED driver uses both DMA and the PIO drivers.

/// The worker thread uses this Signal for updates.
///
/// Note that multiple instances of the `LedStripGroup` will all share the same update value.
static LATEST_LED: Signal<CriticalSectionRawMutex, Vec<RGB8, MAX_GROUP_SIZE>> = Signal::new();

/// The builder for the led strip provides both an async Future that is used to run, as well as a
/// Handle with an update method on it.
pub struct LedStripGroup<'d, P: Instance, const S: usize, const N: usize> {
    // Underlying device.
    strip: PioWs2812<'d, P, S, N>,
}

impl<'d, P: Instance, const S: usize, const N: usize> LedStripGroup<'d, P, S, N> {
    pub fn new(strip: PioWs2812<'d, P, S, N>) -> Self {
        Self { strip }
    }

    /// Get the handle that implements [`LedGroup`].
    pub fn get_handle(&self) -> LedStripHandle {
        LedStripHandle(N)
    }

    /// The worker that will actually update the LEDs when written.  This is generalized a bit
    /// because top-level tasks need to have specific types, but we don't want to leak into that.
    pub async fn update_task(mut self) {
        loop {
            let values = LATEST_LED.wait().await;
            if let Ok(values) = <&[RGB8; N]>::try_from(values.as_slice()) {
                self.strip.write(values).await;
            } else {
                panic!("Length mismatch on LED write: {} vs {}", values.len(), N);
            }
        }
    }
}

/// The item that is assembled with the generic dyn is more simple.  The parameter is the size.
pub struct LedStripHandle(usize);

impl LedGroup for LedStripHandle {
    fn len(&self) -> usize {
        self.0
    }

    fn update(&mut self, values: &[RGB8]) {
        let mut buf = Vec::new();
        buf.extend_from_slice(values).unwrap();
        LATEST_LED.signal(buf);
    }
}
