//! Handle keyminder requests.

use log::info;
use minder::{Request, SerialDecoder};
use zephyr::{device::uart::UartIrq, kobj_define, sync::Arc, time::Duration};

use crate::Stats;

/// The minder.
pub struct Minder();

impl Minder {
    pub fn new(stats: Arc<Stats>, uart: UartIrq) -> Minder {
        let mut thread = MINDER_THREAD.init_once(MINDER_STACK.init_once(()).unwrap()).unwrap();
        thread.set_priority(6);
        thread.set_name(c"minder");
        thread.spawn(move || {
            minder_thread(stats, uart);
        });

        Minder()
    }
}

fn minder_thread(stats: Arc<Stats>, mut uart: UartIrq) {

    let mut minder_packet = [0u8; 64];
    let mut decoder = SerialDecoder::new();

    // TODO: This should be better than just counting, as it would print way more frequently with
    // more messages.

    let mut stat_count = 0;
    loop {
        let count = unsafe { uart.try_read(&mut minder_packet, Duration::millis_at_least(1_000)) };

        stats.start("minder");
        if count > 0 {
            for &byte in &minder_packet[..count] {
                if let Some(packet) = decoder.add_decode::<Request>(byte) {
                    info!("Minder: {:?}", packet);
                }
            }
        }
        stats.stop("minder");

        stat_count += 1;
        if stat_count >= 60 {
            stat_count = 0;
            stats.start("stats");
            stats.show();
            stats.stop("stats");
        }
    }
}

kobj_define! {
    static MINDER_THREAD: StaticThread;
    static MINDER_STACK: ThreadStack<4096>;
}
