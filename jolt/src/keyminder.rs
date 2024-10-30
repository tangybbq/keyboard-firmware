//! Handle keyminder requests.

use alloc::vec::Vec;
use log::{info, warn};
use minder::{HidDecoder, Request};
use zephyr::{kobj_define, sync::Arc, time::Forever};

use crate::{devices::usb::Usb, Stats};

/// The minder.
pub struct Minder();

impl Minder {
    pub fn new(stats: Arc<Stats>, usb: Arc<Usb>) -> Minder {
        let mut thread = MINDER_THREAD.init_once(MINDER_STACK.init_once(()).unwrap()).unwrap();
        thread.set_priority(6);
        thread.set_name(c"minder");
        thread.spawn(move || {
            minder_thread(stats, usb);
        });

        Minder()
    }
}

fn minder_thread(_stats: Arc<Stats>, usb: Arc<Usb>) {
    let mut minder_packet = [0u8; 64];
    let mut decoder = HidDecoder::new();

    loop {
        match usb.minder_read_out(Forever, &mut minder_packet) {
            Ok(len) => {
                // info!("Minder: {:02x?}", &minder_packet[..len]);
                decoder.add_packet(&minder_packet[..len]);

                if decoder.is_ready() {
                    let req: Result<Vec<Request>, _> = decoder.decode();
                    if let Ok(req) = req {
                        info!("Minder Request: {:?}", req);
                    } else {
                        warn!("Invalid minder request");
                    }
                }
            }
            Err(_) => (),
        }
    }
}

kobj_define! {
    static MINDER_THREAD: StaticThread;
    static MINDER_STACK: ThreadStack<4096>;
}
