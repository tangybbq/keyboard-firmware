//! Handle keyminder requests.

use alloc::string::ToString;
use core::convert::Infallible;

use log::info;
use minder::{Reply, Request, SerialDecoder, SerialWrite};
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
    let mut reply_hello = false;
    loop {
        let count = unsafe { uart.try_read(&mut minder_packet, Duration::millis_at_least(1_000)) };

        stats.start("minder");
        if count > 0 {
            for &byte in &minder_packet[..count] {
                if let Some(packet) = decoder.add_decode::<Request>(byte) {
                    info!("Minder: {:?}", packet);
                    reply_hello = true;
                }
            }
        }

        // If we got a hello, send a reply.
        if reply_hello {
            reply_hello = false;

            let reply = Reply::Hello {
                version: minder::VERSION.to_string(),
                info: "todo: put build information here".to_string(),
            };
            minder::serial_encode(&reply, WritePort(&mut uart)).unwrap();
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

struct WritePort<'a>(&'a mut UartIrq);

impl<'a> SerialWrite for WritePort<'a> {
    type Error = Infallible;

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        Ok(unsafe { self.0.write(buf) })
    }
}

kobj_define! {
    static MINDER_THREAD: StaticThread;
    static MINDER_STACK: ThreadStack<4096>;
}
