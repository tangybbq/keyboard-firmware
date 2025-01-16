//! Handle keyminder requests.

use alloc::vec;
use alloc::{string::ToString, vec::Vec};

use log::info;
use minder::{Reply, Request, SerialDecoder};
use zephyr::{
    device::uart::UartIrq,
    kobj_define, printkln,
    sync::{Arc, Mutex},
    time::{Duration, NoWait},
};

use crate::logging::Logger;

/// The minder.
pub struct Minder();

/// Our uart, with fixed sized rings.
type Uart = UartIrq<2, READ_RINGS>;

const READ_RINGS: usize = 8;

/// The size of the read buffers.
const READ_BUFSIZE: usize = 256;

impl Minder {
    pub fn new(uart: Uart, log: Arc<Mutex<Logger>>) -> Minder {
        let mut thread = MINDER_THREAD
            .init_once(MINDER_STACK.init_once(()).unwrap())
            .unwrap();
        thread.set_priority(4);
        thread.set_name(c"minder");
        thread.spawn(move || {
            minder_thread(uart, log);
        });

        Minder()
    }
}

fn minder_thread(mut uart: Uart, log: Arc<Mutex<Logger>>) {
    let mut decoder = SerialDecoder::new();

    // Add two buffers for reading.
    for _ in 0..READ_RINGS {
        uart.read_enqueue(vec![0u8; READ_BUFSIZE]).unwrap();
    }

    // TODO: This should be better than just counting, as it would print way more frequently with
    // more messages.

    let mut reply_hello = false;
    loop {
        match uart.read_wait(Duration::millis_at_least(100)) {
            Ok(buf) => {
                for &byte in buf.as_slice() {
                    if let Some(packet) = decoder.add_decode::<Request>(byte) {
                        info!("Minder: {:?}", packet);
                        reply_hello = true;
                    }
                }

                // Put the buffer back.
                uart.read_enqueue(buf.into_inner()).unwrap();
            }
            // Timeout, just go on.
            Err(_) => (),
        }

        // If we got a hello, send a reply.
        if reply_hello {
            reply_hello = false;

            let mut buffer = Vec::new();
            let reply = Reply::Hello {
                version: minder::VERSION.to_string(),
                info: "todo: put build information here".to_string(),
            };
            minder::serial_encode(&reply, &mut buffer, true).unwrap();

            // Attempt to write it, but just ignore the error if we can't.
            let len = buffer.len();
            let _ = uart.write_enqueue(buffer, 0..len);
        }

        // Try printing out log messages.  We intentionally only lock for each message to avoid
        // locking anything too long.
        loop {
            let mut inner = log.lock().unwrap();
            let msg = inner.pop(0);
            drop(inner);

            if let Some(msg) = msg {
                printkln!("log: {}", msg);
            } else {
                break;
            }
        }

        // Also try sending a message over the minder port.  Unsure how data will be handled if
        // there is no listener.
        loop {
            // Handle any completed writes.
            // For now, just discard the buffer, as we'll dynamically allocate new ones.
            while let Ok(_) = uart.write_wait(NoWait) {}

            // Don't do any of this unless something is actually connected.
            if unsafe { !uart.inner().is_dtr_set().unwrap() } {
                break;
            }

            // Also don't try to write if there isn't any space.
            if uart.write_is_full() {
                break;
            }

            let mut inner = log.lock().unwrap();
            let msg = inner.pop(1);
            drop(inner);

            if let Some(msg) = msg {
                // Encode the message to a new Vec<u8> so we can write it as a single unit.
                let mut buffer = Vec::new();
                let reply = Reply::Log { message: msg };
                minder::serial_encode(&reply, &mut buffer, true).unwrap();

                // Write the entire thing.
                let len = buffer.len();
                uart.write_enqueue(buffer, 0..len)
                    .expect("Queue full, despite check");
            } else {
                break;
            }
        }
        /*
        while let Some(msg) = log.lock().unwrap().pop(1) {
            let reply = Reply::Log {
                message: msg,
            };
            minder::serial_encode(&reply, WritePort(&mut uart)).unwrap();
        }
        */
    }
}

kobj_define! {
    static MINDER_THREAD: StaticThread;
    static MINDER_STACK: ThreadStack<4096>;
}
