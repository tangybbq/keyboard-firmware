//! Inter-board communication
//!
//! The Jolt-3 introduces a signifcant change from the earlier boards, the availability of I2C to
//! communicate between the boards.  To make this even more efficient, the boards also connect the
//! uart pins, and we use the line from the passive side to the active side as an interrupt line.
//!
//! # Protocol.
//!
//! The protocol is intended to be quite simple, and compact.

use bbq_keyboard::KeyEvent;
use crc::{Crc, CRC_16_IBM_SDLC};
use embassy_rp::{gpio::{Input, Output}, i2c::{self, Instance}, i2c_slave};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Sender, mutex::Mutex};
use embassy_time::{Duration, Timer};
use smart_leds::RGB8;
use static_cell::StaticCell;

use crate::{logging::info, BUILD_ID};

pub const CRC: Crc<u16> = Crc::<u16>::new(&CRC_16_IBM_SDLC);

/// Largest Request.
const REQUEST_MAX: usize = 7 + 2;

/// Largest Reply.
const REPLY_MAX: usize = 9 + 2;

/// Protocol requests.
#[derive(Debug)]
pub enum Request {
    // 01
    Hello,
    // 02 rr gg bb rr gg bb
    SetLeds {
        leds: [RGB8; 2],
    },
    // 03
    ReadKeys,
}

#[cfg(feature = "defmt")]
impl defmt::Format for Request {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            Request::Hello => defmt::write!(fmt, "Request::Hello"),
            Request::SetLeds { leds } => {
                defmt::write!(
                    fmt,
                    "Request::SetLeds([({},{},{}),({},{},{})])",
                    leds[0].r,
                    leds[0].g,
                    leds[0].b,
                    leds[1].r,
                    leds[1].g,
                    leds[1].b,
                );
            }
            Request::ReadKeys => defmt::write!(fmt, "Request::ReadKeys"),
        }
    }
}

type RequestBuf = heapless::Vec<u8, REQUEST_MAX>;
type ReplyBuf = heapless::Vec<u8, REPLY_MAX>;

impl Request {
    pub fn encode(&self) -> RequestBuf {
        let mut buf = RequestBuf::new();

        match self {
            Request::Hello => {
                buf.push(0x01).unwrap();
            }
            Request::SetLeds { leds } => {
                buf.push(0x02).unwrap();
                for led in leds {
                    buf.push(led.r).unwrap();
                    buf.push(led.g).unwrap();
                    buf.push(led.b).unwrap();
                }
            }
            Request::ReadKeys => {
                buf.push(0x03).unwrap();
            }
        }

        let mut digest = CRC.digest();
        digest.update(&buf);
        let code = digest.finalize();
        buf.push((code & 0xff) as u8).unwrap();
        buf.push((code >> 8) as u8).unwrap();

        buf
    }

    pub fn decode(mut data: &[u8]) -> Option<Self> {
        // Validate the CRC.
        if data.len() < 3 {
            return None;
        }

        let mut digest = CRC.digest();
        digest.update(data);
        let code = digest.finalize();
        if code != 0x0f47 {
            info!("Incorrect CRC: {:#x}", code);
            return None;
        }

        let packet = match pop_front(&mut data) {
            Some(0x01) => Self::Hello,
            Some(0x02) => {
                if data.len() != 6 {
                    return None;
                }
                let res = Self::SetLeds {
                    leds: [
                        RGB8::new(data[0], data[1], data[2]),
                        RGB8::new(data[3], data[4], data[5]),
                    ],
                };
                data = &data[6..];
                res
            }
            Some(0x03) => Self::ReadKeys,
            _ => return None,
        };

        // Make sure that the CRC is exactly the remaining payload.
        if data.len() != 2 {
            // Extra data is also an error.
            return None
        }

        Some(packet)
    }
}

/// Pop the first byte of the array, returning Ok(b) and adjusting the slice itself to be shorter.
fn pop_front(buf: &mut &[u8]) -> Option<u8> {
    if let Some(&elt) = buf.first() {
        *buf = &buf[1..];
        Some(elt)
    } else {
        None
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Reply {
    // 01 aa bb cc dd ee ff gg hh
    Hello {
        version: [u8; 8],
    },
    // 03 aa bb cc dd
    Keys(u32),
}

const HELLO_REPLY_SIZE: usize = 9 + 2;
const KEYS_REPLY_SIZE: usize = 5 + 2;

impl Reply {
    pub fn encode(&self) -> ReplyBuf {
        let mut buf = ReplyBuf::new();

        match self {
            Reply::Hello { version } => {
                buf.push(0x01).unwrap();
                buf.extend_from_slice(version).unwrap();
            }
            Reply::Keys(bits) => {
                buf.push(0x03).unwrap();
                let mut tmp = *bits;
                for _ in 0..4 {
                    buf.push((tmp & 0xff) as u8).unwrap();
                    tmp >>= 8;
                }
            }
        }

        let mut digest = CRC.digest();
        digest.update(&buf);
        let code = digest.finalize();
        buf.push((code & 0xff) as u8).unwrap();
        buf.push((code >> 8) as u8).unwrap();

        buf
    }

    pub fn decode(mut data: &[u8]) -> Option<Self> {
        // Validate the CRC.
        if data.len() < 3 {
            return None;
        }

        let mut digest = CRC.digest();
        digest.update(data);
        let code = digest.finalize();
        if code != 0x0f47 {
            info!("Incorrect CRC: {:#x}", code);
            return None;
        }

        let packet = match pop_front(&mut data) {
            Some(0x01) => { 
                if data.len() < 8 {
                    return None;
                }
                let version = match data[..8].try_into() {
                    Ok(buf) => buf,
                    Err(_) => return None,
                };
                data = &data[8..];
                Self::Hello {
                    version,
                }
            }
            Some(0x03) => {
                if data.len() < 4 {
                    return None;
                }
                let mut tmp = 0u32;
                for b in data[..4].iter().rev() {
                    tmp <<= 8;
                    tmp |= *b as u32;
                }
                data = &data[4..];
                Self::Keys(tmp)
            }
            _ => return None,
        };

        // Make sure that the CRC is exactly the remaining payload.
        if data.len() != 2 {
            return None
        }

        Some(packet)
    }
}

/// The manager for the "passive" side of the keyboard.  This side merely takes keystrokes, and
/// sends them to the other side.
pub struct InterPassive {
    state: &'static Mutex<CriticalSectionRawMutex, PassiveState>,
}

struct PassiveState {
    /// What keys have been pressed on our side.  These are numbered from 0, and not biased for the
    /// other side (that is taken care of on the Active side).
    keys: u32,
    irq: Output<'static>,
}

/// Information passed to the passive task.
pub struct PassiveTask<T: Instance + 'static> {
    bus: i2c_slave::I2cSlave<'static, T>,
    state: &'static Mutex<CriticalSectionRawMutex, PassiveState>
}

impl InterPassive {
    /// Construct a new instance of our passive state, returning that instance, as well as a future
    /// for the handler.  This will fail if called more than once, due to the static cell.
    pub fn new<T: Instance>(
        bus: i2c_slave::I2cSlave<'static, T>,
        irq: Output<'static>,
    ) -> (Self, PassiveTask<T>) {
        static STATE: StaticCell<Mutex<CriticalSectionRawMutex, PassiveState>> = StaticCell::new();
        let state = STATE.init(Mutex::new(PassiveState {
            keys: 0,
            irq,
        }));

        let state = state as &_;

        let this = InterPassive {
            state,
        };

        (this, PassiveTask { bus, state })
    }

    pub async fn update(&self, event: KeyEvent) {
        let mut state = self.state.lock().await;

        let code = match event.key() {
            code @ 24..48 => code - 24,
            // Anything else, don't store.
            _ => return,
        };

        // Update our bitmap of keys.
        if event.is_press() {
            state.keys |= 1 << code;
        } else {
            state.keys &= !(1 << code);
        }

        // Assert the IRQ line so the other side will know to read our data.
        state.irq.set_high();
    }
}

impl<T: Instance + 'static> PassiveTask<T> {
    pub async fn handler(mut self) {
        info!("I2C passive wait");
        let mut buf = [0u8; REQUEST_MAX];
        loop {
            // We should only get Write or WriteRead requests.
            match self.bus.listen(&mut buf).await {
                Ok(i2c_slave::Command::Write(len)) => {
                    if let Some(req) = Request::decode(&buf[..len]) {
                        info!("Write: {:?}", req);
                    }
                    // Nothing in particular to do with a write, it has already been acked.
                }
                Ok(i2c_slave::Command::WriteRead(len)) => {
                    if let Some(req) = Request::decode(&buf[..len]) {
                        // info!("WriteRead: {:?}", req);
                        let reply = match req {
                            Request::Hello => {
                                Reply::encode(
                                    &Reply::Hello {
                                        version: BUILD_ID.to_le_bytes(),
                                    }
                                )
                            }
                            Request::SetLeds { leds } => {
                                let _ = leds;
                                // This shouldn't be a WriteRead.
                                info!("I2C: SetLeds shouldn't be WriteRead");
                                let mut result = ReplyBuf::new();
                                result.push(0xff).unwrap();
                                result
                            }
                            Request::ReadKeys => {
                                let mut state = self.state.lock().await;
                                let keys = state.keys;
                                // De-assert the IRQ before giving this value back.  If a keypress
                                // comes in during this transaction, it will be reasserted.
                                state.irq.set_low();
                                drop(state);

                                Reply::encode(
                                    &Reply::Keys(keys)
                                )
                            }
                        };
                        self.reply(&reply).await;
                    } else {
                        // If we didn't recognize the request, just reply with an error indicator.
                        self.reply(&[0xff]).await;
                    }
                }
                Ok(i2c_slave::Command::Read) => (),
                Ok(i2c_slave::Command::GeneralCall(_len)) => (),
                Err(_) => (),
            }
        }
    }

    async fn reply(&mut self, data: &[u8]) {
        match self.bus.respond_and_fill(data, 0xff).await {
            Ok(i2c_slave::ReadStatus::Done) => (),
            Ok(i2c_slave::ReadStatus::NeedMoreBytes) => unreachable!(),
            Ok(i2c_slave::ReadStatus::LeftoverBytes(x)) => {
                info!("Tried to write {} extra bytes on i2c", x);
            }
            // Warn on error?
            Err(_) => (),
        }
    }
}

/// For the active side, the task running this (tied to the I2C bus specifics), will respond to
/// interrupts.  As well as having a receipt channel to send keys back.
pub async fn active_task<I: Instance>(
    mut irq: Input<'static>,
    mut bus: i2c::I2c<'static, I, i2c::Async>,
    keys_out: Sender<'static, CriticalSectionRawMutex, KeyEvent, 1>,
) -> ! {
    // Initialization, try to "hello" the other side.
    let mut last_keys = 0;

    info!("Starting I2C active");
    let mut delay = 1;
    loop {
        let message = Request::Hello.encode();
        let mut resp_buf = [0u8; HELLO_REPLY_SIZE];
        info!("Sending hello");
        match bus.write_read_async(0x42u16, message.iter().cloned(), &mut resp_buf).await {
            Ok(()) => {
                if let Some(reply) = Reply::decode(&resp_buf) {
                    info!("Hello reply: {:?}", reply);
                    break;
                }
            }
            Err(e) => {
                info!("Error reply: {:?}", e);
                delay *= 2.min(600);
            }
        }

        // An error from the other side.  Just try again.
        Timer::after(Duration::from_secs(delay)).await;
    }

    loop {
        irq.wait_for_high().await;

        // info!("I2C active: Reading keys");
        let mut resp_buf = [0u8; KEYS_REPLY_SIZE];
        let message = Request::ReadKeys.encode();
        match bus.write_read_async(0x42u16, message.iter().cloned(), &mut resp_buf).await {
            Ok(()) => {
                if let Some(Reply::Keys(keys)) = Reply::decode(&resp_buf) {
                    // info!("Keys: {:#x}", keys);

                    let mut delta = last_keys ^ keys;
                    while delta != 0 {
                        let bit = delta.trailing_zeros();

                        let event = if keys & (1 << bit) != 0 {
                            KeyEvent::Press(bit as u8 + 24)
                        } else {
                            KeyEvent::Release(bit as u8 + 24)
                        };

                        delta &= !(1 << bit);

                        // info!("  ev: {:?}", event);
                        keys_out.send(event).await;
                    }

                    last_keys = keys;

                    continue;
                }
            }
            Err(e) => {
                info!("I2C read error: {:?}", e);
            }
        }

        // On read error, wait a bit so we don't just hammer the system with failed requests.
        // TODO: Better here would be to detect repeated cases of this and consider the side
        // disconnected, and stop paying attention to the irq line until it responds to hello.
        Timer::after(Duration::from_millis(100)).await;
    }
}

/*
/// The manager for the "active" side of the keyboard.
pub struct InterActive {
    /// The current view of what keys have been pressed on the other side.
    keys: u32,
    /// The shared I2C device we can update.
    bus: &'static Mutex<CriticalSectionRawMutex, i2c::
}
*/
