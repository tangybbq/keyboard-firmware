//! UART-based inter-board communication.
//!
//! The UART-based protocol effectively synchronizes a small amount of shared data between the two
//! sides of the keyboard.  The protocol is designed to communicate as little as possible, only
//! needing retransmits if it determins that the other side is out of date.
//!
//! Packets are encoded using
//! [COBS](https://blog.mbedded.ninja/programming/serialization-formats/consistent-overhead-byte-stuffing-cobs/),
//! which allows us to have predictable framing with fixed overhead, in this case a single byte at
//! the start and end of the packet.

use bbq_keyboard::KeyEvent;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use embedded_io_async::{Read, Write};
use minder::cobs::{CobsDecoder, CobsEncoder};
use smart_leds::RGB8;

use crate::inter::CRC;
use crate::logging::warn;

/// How many LEDS are in the protocol.
const LED_COUNT: usize = 2;

/// The data that is synchronized.
#[derive(Default, Debug, Eq, PartialEq, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Shared {
    /// The keys that are down on the passive side.  One bit per key, biased by the first key on the
    /// side (so scancode 24 is bit 0).
    keys: u32,
    /// The state of two LEDs.
    #[cfg_attr(feature = "defmt", defmt(Debug2Format))]
    leds: [RGB8; LED_COUNT],
}

/// A magic number for this protocol.
///
/// Randomly generated.
const MAGIC: u32 = 0x84ca7faa;

/// The size of the packet, without the stuffing (see below).
const PACKET_SIZE: usize =
    4 // Magic
    + 4 // keys
    + LED_COUNT*3 // leds
    + 2 // CRC16.
    ;

// The size of the full packet, including the COBS stuffing.
const FULL_PACKET_SIZE: usize = PACKET_SIZE + 2;

type Packet = heapless::Vec<u8, FULL_PACKET_SIZE>;

impl Shared {
    /// Encode the shared packet.
    pub fn encode(&self) -> Packet {
        let mut enc = CobsEncoder::<FULL_PACKET_SIZE>::new();
        let mut digest = CRC.digest();

        let bytes = MAGIC.to_le_bytes();
        enc.push_slice(&bytes);
        digest.update(&bytes);

        let bytes = self.keys.to_le_bytes();
        enc.push_slice(&bytes);
        digest.update(&bytes);

        let mut bytes = heapless::Vec::<u8, {3 * LED_COUNT}>::new();
        for led in &self.leds {
            bytes.push(led.r).unwrap();
            bytes.push(led.g).unwrap();
            bytes.push(led.b).unwrap();
        }
        enc.push_slice(&bytes);
        digest.update(&bytes);

        let bytes = digest.finalize().to_le_bytes();
        enc.push_slice(&bytes);

        enc.finish()
    }

    /// Decoder for packets.  This should be the result of the CobsDecoder, when a full packet is
    /// retrieved.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != PACKET_SIZE {
            return None;
        }

        // Validate the CRC.
        let mut digest = CRC.digest();
        digest.update(&data);
        let check = digest.finalize();
        if check != 0x0f47 {
            return None;
        }

        if u32::from_le_bytes(data[0..4].try_into().ok()?) != MAGIC {
            return None;
        }

        Some(Shared {
            keys: u32::from_le_bytes(data[4..8].try_into().ok()?),
            leds: [
                RGB8 {
                    r: data[8],
                    g: data[9],
                    b: data[10],
                },
                RGB8 {
                    r: data[11],
                    g: data[12],
                    b: data[13],
                },
            ]
        })
    }
}

/// We have two codecs, one for each side.
pub struct ActiveCodec {
    decoder: CobsDecoder::<PACKET_SIZE>,
}

impl Codec<FULL_PACKET_SIZE, [RGB8; LED_COUNT], u32> for ActiveCodec {
    fn encode<'a>(&'a mut self, l: &[RGB8; LED_COUNT], r: &u32) -> heapless::Vec<u8, FULL_PACKET_SIZE> {
        Shared {
            keys: *r,
            leds: *l,
        }
        .encode()
    }

    fn decode(&mut self, byte: u8) -> Option<([RGB8; LED_COUNT], u32)> {
        if let Some(packet) = self.decoder.add_byte(byte) {
            if let Some(info) = Shared::decode(packet) {
                return Some((info.leds, info.keys));
            }
        }
        None
    }
}

pub type InterActive = BiSync<FULL_PACKET_SIZE, [RGB8; LED_COUNT], u32, ActiveCodec, u32>;

// Methods specific to this type.
impl InterActive {
    pub fn new() -> Self {
        Self::new_internal([RGB8::default(); LED_COUNT], 0,
                           ActiveCodec {
                               decoder: CobsDecoder::new(),
                           }, 0)
    }

    /// Wait for an event indicating new keys from the other side.
    pub async fn get_key(&self) -> KeyEvent {
        loop {
            let mut st = self.state.lock().await;

            // If we still have keys we haven't notified about, send those.
            let delta = st.remote ^ st.extra;
            if delta != 0 {
                let bit = delta.trailing_zeros();

                let event = if st.remote & (1 << bit) != 0 {
                    KeyEvent::Press(bit as u8 + 24)
                } else {
                    KeyEvent::Release(bit as u8 + 24)
                };

                // Make the 'extra' bit match.
                st.extra ^= 1 << bit;

                return event;
            } else {
                // No keys to send, drop the lock, and wait.
                drop(st);
                let _ = self.listener_wake.wait().await;
            }
        }
    }
}

/// The passive side codec.
pub struct PassiveCodec {
    decoder: CobsDecoder::<PACKET_SIZE>,
}

impl Codec<FULL_PACKET_SIZE, u32, [RGB8; LED_COUNT]> for PassiveCodec {
    fn encode<'a>(&'a mut self, l: &u32, r: &[RGB8; LED_COUNT]) -> heapless::Vec<u8, FULL_PACKET_SIZE> {
        Shared {
            keys: *l,
            leds: *r,
        }
        .encode()
    }

    fn decode(&mut self, byte: u8) -> Option<(u32, [RGB8; LED_COUNT])> {
        if let Some(packet) = self.decoder.add_byte(byte) {
            if let Some(info) = Shared::decode(packet) {
                return Some((info.keys, info.leds));
            }
        }
        None
    }
}

pub type InterPassive = BiSync<FULL_PACKET_SIZE, u32, [RGB8; LED_COUNT], PassiveCodec, ()>;

/// A codec is some type that implements this encode/decode protocol.  It is expected that the Codec
/// maintains the buffers needed to do this.
pub trait Codec<const N: usize, L, R> {
    /// Encode a single instance of the data.
    fn encode<'a>(&'a mut self, l: &L, r: &R) -> heapless::Vec<u8, N>;

    /// Add a single byte to the decoder, returning a fully decoded packet, if available.
    fn decode(&mut self, byte: u8) -> Option<(L, R)>;
}

impl InterPassive {
    pub fn new() -> Self {
        Self::new_internal(0, [RGB8::default(); LED_COUNT],
        PassiveCodec {
            decoder: CobsDecoder::new(),
        }, ())
    }

    pub async fn update_keys(&self, event: KeyEvent) {
        let code = match event.key() {
            code @ 24..48 => code - 24,
            // Anything else, don't do it.
            _ => return,
        };

        let mut st = self.state.lock().await;
        if event.is_press() {
            st.local |= 1 << code;
        } else {
            st.local &= !(1 << code);
        }

        // Must match what `update` does.
        st.local_dirty = true;
        self.tx_wake.signal(());
    }
}

/// A BiSync coordinates data synchronization of data between two parties.
///
/// Each party maintains a set of "Local" data and a set of "Remote" data.  The party can make
/// updates to its "Local" data which will be propagated to the other side's "Remote", and
/// vice-versa.
///
/// A provider must provide a way to encode and decode this data into a sequence of bytes.  Because
/// of the criss-cross symmetry, it is important to make sure that the encoding is consistent (often
/// the sides will pack the data into the same struct which supports encode/decode.
pub struct BiSync<
    // The packet size for encoded data.
    const N: usize,
    // The local data.
    L, 
    // The remote data.
    R,
    // The needed encoder/decoder.
    C,
    // Extra data to be stored in 'extra' in the state.
    E,
>
where
    C: Codec<N, L, R>,
{
    /// The internal state.
    state: Mutex<CriticalSectionRawMutex, State<N, L, R, C, E>>,

    /// Wake the tx task.
    tx_wake: Signal<CriticalSectionRawMutex, ()>,

    /// Wake the listener (notified upon fresh 'R' data).
    listener_wake: Signal<CriticalSectionRawMutex, ()>,
}

/// The state maintains most of the work of the syncronization.
pub struct State<const N: usize, L, R, C, E>
where
    C: Codec<N, L, R>,
{
    /// The codec for encoding/decoding.
    codec: C,

    /// Our latest version of the data.
    local: L,

    /// Our copy of the remote data.
    remote: R,

    /// We have an update to the local side, which needs to be transmitted until we see evidence
    /// that the other side has this value.
    local_dirty: bool,

    /// We have received a packet from the other side, and need to reply once, as evidence that we
    /// have received the data.
    // remote_dirty: bool,

    /// We've received a new remote value since last time it was requested.
    remote_fresh: bool,

    /// Extra data.
    extra: E,
}

impl<const N: usize, L, R, C, E> State<N, L, R, C, E>
where
    C: Codec<N, L, R>,
{
    pub fn new(local: L, remote: R, codec: C, extra: E) -> Self {
        Self {
            codec,
            local,
            remote,
            local_dirty: false,
            // remote_dirty: false,
            remote_fresh: false,
            extra,
        }
    }
}

impl<const N: usize, L, R, C, E> BiSync<N, L, R, C, E>
where
    L: Clone,
    R: Clone,
    C: Codec<N, L, R>,
{
    pub fn new_internal(local: L, remote: R, codec: C, extra: E) -> Self {
        let state = Mutex::new(State::new(local, remote, codec, extra));
        let tx_wake = Signal::new();
        let listener_wake = Signal::new();
        Self { state, tx_wake, listener_wake }
    }

    /// User should create a separate task to run this transmit task.
    pub async fn tx_task<W: Write>(&'static self, tx: &mut W) -> ! {
        loop {
            // Idle phase, we want on the signal for new data to write out.
            let () = self.tx_wake.wait().await;

            let mut st = self.state.lock().await;

            // If there isn't a reason to transmit, don't.
            if !st.local_dirty {
                continue;
            }

            // Transmit the new state.
            // Ideally, we'd avoid the clone here, but not sure how to do all of these borrows.
            let local = st.local.clone();
            let remote = st.remote.clone();
            let encoded = st.codec.encode(&local, &remote);

            drop(st);

            // Actually transmit.
            let _ = tx.write_all(&encoded).await;

            // Count the number of times we've transmitted a given packet, without change.  Once we
            // exceed this, we just stop.  This is a lot simpler than the kind of retransmission
            // handling that TCP tries to do, and this still results in robust use as long as
            // dropped packets are uncommon.
            let mut count = 5;
            loop {
                Timer::after(Duration::from_millis(2)).await;

                let mut st = self.state.lock().await;

                // Before checking, "absorb" the wake signal, as we're about to handle it.
                let _ = self.tx_wake.try_take();

                // If we've been asked to transmit new data, reset the counter.
                if st.local_dirty {
                    count = 5;
                }

                let local = st.local.clone();
                let remote = st.remote.clone();
                let encoded = st.codec.encode(&local, &remote);

                // Consider our data not dirty, so we can tell if it gets updated again.
                st.local_dirty = false;

                drop(st);

                // Actually transmit.
                let _ = tx.write_all(&encoded).await;

                if count == 0 {
                    break;
                }

                count -= 1;

                // And go again, with the short retry.
            }
        }
    }

    /// User should create a separate task to run this receive task.
    pub async fn rx_task<Rd: Read>(&'static self, rx: &mut Rd) -> ! {
        // TODO: Should the size of this buffer be given, and not hard-coded like this.
        let mut buffer = [0u8; FULL_PACKET_SIZE];
        loop {
            let len = match rx.read(&mut buffer).await {
                Ok(len) => len,
                Err(_) => {
                    warn!("UART read error");
                    continue;
                }
            };

            // TODO: Locking time could be reduced by having the decoder part separate from the
            // encode.
            let mut st = self.state.lock().await;
            for &byte in &buffer[..len] {
                if let Some((_, remote)) = st.codec.decode(byte) {
                    // Store the remote data.
                    st.remote = remote.clone();

                    // And indicate it is fresh.
                    st.remote_fresh = true;

                    self.listener_wake.signal(());
                }
            }
        }
    }

    /// Update the local item, causing the item to be transmitted.
    #[allow(dead_code)]
    pub async fn update(&'static self, new_local: L) {
        let mut st = self.state.lock().await;
        st.local = new_local;
        st.local_dirty = true;
        self.tx_wake.signal(());
    }

    /// Wait for a new value of the 'remote'.
    #[allow(dead_code)]
    pub async fn wait_remote(&'static self) -> R {
        loop {
            let mut st = self.state.lock().await;
            if st.remote_fresh {
                let item = st.remote.clone();
                st.remote_fresh = false;
                drop(st);
                return item;
            }

            let () = self.listener_wake.wait().await;
        }
    }
}
