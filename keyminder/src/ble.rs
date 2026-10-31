//! Minder over Bluetooth, through the GATT service in jolt's `minder_ble.c`.
//!
//! The keyboard is already connected to the host, as a keyboard, and while it is connected it
//! does not advertise, so it is looked for among the host's connected devices rather than by a
//! scan.  Bluetooth is async and the rest of this crate is not, so a worker thread owns the
//! Bluetooth side and hands packets across channels.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use bluest::{Adapter, Characteristic, Device, Uuid};
use futures::channel::mpsc as async_mpsc;
use futures::{executor::block_on, FutureExt, StreamExt};
use minder::{ble, HidDecoder, HidWrite, PACKET_SIZE};
use minicbor::Encode;

/// The HID Service, which every keyboard has: the second way to find a keyboard among the
/// connected devices, should the host not have listed its minder service.
const HID_SERVICE: Uuid = Uuid::from_u128(0x0000_1812_0000_1000_8000_0080_5f9b_34fb);

/// A minder connection to a keyboard over Bluetooth.
pub struct BleLink {
    /// Packets for the worker to write.
    outgoing: async_mpsc::UnboundedSender<Vec<u8>>,
    /// Packets the keyboard notified.
    incoming: mpsc::Receiver<Vec<u8>>,
    /// The reply being reassembled.
    decoder: HidDecoder,
}

impl BleLink {
    /// Find a connected keyboard whose name contains `name` (any, if it is empty) and that
    /// carries the minder service, and open the service.
    pub fn open(name: &str) -> Result<Self> {
        let (setup_tx, setup_rx) = mpsc::channel();
        let (outgoing, outgoing_rx) = async_mpsc::unbounded();
        let (incoming_tx, incoming) = mpsc::channel();
        let want = name.to_string();

        thread::spawn(move || block_on(worker(want, setup_tx, outgoing_rx, incoming_tx)));

        let found = setup_rx.recv().context("the Bluetooth worker ended")??;
        eprintln!("Bluetooth: {}", found);
        Ok(Self {
            outgoing,
            incoming,
            decoder: HidDecoder::new(),
        })
    }

    /// Send a request, as packets.
    pub fn send<Out: Encode<()>>(&mut self, req: &Out) -> Result<()> {
        minder::hid_encode(req, Sink(&self.outgoing))
            .map_err(|err| anyhow!("sending over Bluetooth: {}", err))
    }

    /// Wait for a whole reply, and give back the decoder holding it.
    pub fn recv(&mut self, timeout: Duration) -> Result<&mut HidDecoder> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let packet = self.incoming.recv_timeout(left).map_err(|err| match err {
                mpsc::RecvTimeoutError::Timeout => anyhow!("timed out waiting for a reply"),
                mpsc::RecvTimeoutError::Disconnected => anyhow!("the Bluetooth link closed"),
            })?;
            if packet.is_empty() {
                continue;
            }
            self.decoder.add_packet(&packet);
            if self.decoder.is_ready() {
                return Ok(&mut self.decoder);
            }
        }
    }

    /// Throw away anything already notified: the rest of a reply owed to an earlier client.
    pub fn drain(&mut self) {
        while self.incoming.recv_timeout(Duration::from_millis(50)).is_ok() {}
        self.decoder = HidDecoder::new();
    }
}

/// Hands each packet `hid_encode` makes to the worker.
struct Sink<'a>(&'a async_mpsc::UnboundedSender<Vec<u8>>);

impl HidWrite for Sink<'_> {
    type Error = String;

    fn write_packet(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0
            .unbounded_send(buf.to_vec())
            .map_err(|_| "the Bluetooth link closed".to_string())
    }
}

/// The Bluetooth side: open the service, report how that went, then pass packets both ways
/// until either end goes.
async fn worker(
    want: String,
    setup: mpsc::Sender<Result<String>>,
    mut outgoing: async_mpsc::UnboundedReceiver<Vec<u8>>,
    incoming: mpsc::Sender<Vec<u8>>,
) {
    // The adapter is held for as long as the link: on macOS it owns CoreBluetooth's central
    // manager, and releasing that ends this program's connection to the keyboard.
    let (_adapter, _device, request, reply, found) = match open(&want).await {
        Ok(opened) => opened,
        Err(err) => {
            let _ = setup.send(Err(err));
            return;
        }
    };
    let mut notifications = match reply.notify().await {
        Ok(notifications) => notifications,
        Err(err) => {
            let _ = setup.send(Err(anyhow!("subscribing to replies: {}", err)));
            return;
        }
    };
    if setup.send(Ok(found)).is_err() {
        return;
    }

    loop {
        futures::select! {
            packet = outgoing.next().fuse() => match packet {
                Some(packet) => {
                    if let Err(err) = request.write_without_response(&packet).await {
                        eprintln!("Bluetooth write failed: {}", err);
                        return;
                    }
                }
                None => return,
            },
            note = notifications.next().fuse() => match note {
                Some(Ok(packet)) => {
                    if incoming.send(packet).is_err() {
                        return;
                    }
                }
                Some(Err(err)) => {
                    eprintln!("Bluetooth notification failed: {}", err);
                    return;
                }
                None => return,
            },
        }
    }
}

/// Find the keyboard and its two characteristics, and say what was found.
async fn open(want: &str) -> Result<(Adapter, Device, Characteristic, Characteristic, String)> {
    let adapter = Adapter::default()
        .await
        .ok_or_else(|| anyhow!("no Bluetooth adapter"))?;
    adapter.wait_available().await?;

    let service_uuid = Uuid::from_u128(ble::SERVICE);
    let devices = adapter
        .connected_devices_with_services(&[service_uuid, HID_SERVICE])
        .await?;

    for device in devices {
        let name = device.name_async().await.unwrap_or_default();
        if !name.contains(want) {
            continue;
        }
        // Connected to the host is not connected for this program; this is quick.
        adapter.connect_device(&device).await?;
        let Some(service) = device
            .discover_services_with_uuid(service_uuid)
            .await?
            .into_iter()
            .next()
        else {
            continue;
        };
        let request = characteristic(&service, ble::REQUEST).await?;
        let reply = characteristic(&service, ble::REPLY).await?;

        let max = request.max_write_len_async().await?;
        if max < PACKET_SIZE {
            return Err(anyhow!(
                "{} takes writes of {} bytes, and minder's packets are {}: the ATT MTU is \
                 too small",
                name,
                max,
                PACKET_SIZE
            ));
        }
        let found = format!("{} (writes up to {} bytes)", name, max);
        return Ok((adapter, device, request, reply, found));
    }

    Err(anyhow!(
        "no connected keyboard{} has the minder service",
        if want.is_empty() {
            String::new()
        } else {
            format!(" named like {:?}", want)
        }
    ))
}

async fn characteristic(service: &bluest::Service, uuid: u128) -> Result<Characteristic> {
    service
        .discover_characteristics_with_uuid(Uuid::from_u128(uuid))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("the minder service has no characteristic {:x}", uuid))
}
