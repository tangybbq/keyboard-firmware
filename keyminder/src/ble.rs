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
use minder::{ble, HidDecoder, HidWrite, Reply, Request, PACKET_SIZE};
use minicbor::Encode;

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
    /// Find the connected keyboard with the minder service whose serial number is `name`, or
    /// else whose Bluetooth name contains it (any, if it is empty), and open the service.  More
    /// than one match is an error that lists them.
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

/// A connected keyboard with the minder service, and its two characteristics.
struct Candidate {
    device: Device,
    request: Characteristic,
    reply: Characteristic,
    name: String,
    /// The `info` of its `Hello`, which is also its USB serial number, once asked; `Err` if it
    /// did not say.
    serial: Option<Result<String>>,
}

impl Candidate {
    /// Ask for its serial number, if that has not been done.
    async fn ask(&mut self) -> &Option<Result<String>> {
        if self.serial.is_none() {
            self.serial = Some(ask_serial(&self.request, &self.reply).await);
        }
        &self.serial
    }

    fn is_serial(&self, want: &str) -> bool {
        matches!(&self.serial, Some(Ok(serial)) if serial == want)
    }

    fn describe(&self) -> String {
        match &self.serial {
            Some(Ok(serial)) => format!("{} ({})", self.name, serial),
            Some(Err(err)) => format!("{} (serial unknown: {})", self.name, err),
            None => self.name.clone(),
        }
    }
}

/// Find the keyboard and its two characteristics, and say what was found.
///
/// `want` picks the keyboard: part of its Bluetooth name, or else its serial number exactly
/// (the USB serial, which is also the `info` of its `Hello`).  Two keyboards on the same
/// profile have the same name, "Jolt Keyboard 1", so a choice that matches more than one
/// keyboard is refused, with the list, rather than settled by whichever macOS happened to list
/// first.
///
/// Asking a keyboard its serial number is a minder request, and another program can be
/// talking minder to the same keyboard: macOS gives both programs every reply, so a `Hello`
/// from here interrupts, say, TaipoTeacher's long poll and hands it a reply it did not ask
/// for.  So the name is tried first, and a keyboard is only asked when the name cannot settle
/// it: a serial number to match, or several keyboards to tell apart.
async fn open(want: &str) -> Result<(Adapter, Device, Characteristic, Characteristic, String)> {
    let adapter = Adapter::default()
        .await
        .ok_or_else(|| anyhow!("no Bluetooth adapter"))?;
    adapter.wait_available().await?;

    let service_uuid = Uuid::from_u128(ble::SERVICE);
    // By the minder service alone.  The HID Service would find nothing more: macOS keeps it to
    // itself and lists no device by it.
    let devices = adapter
        .connected_devices_with_services(&[service_uuid])
        .await?;

    let mut candidates = Vec::new();
    for device in devices {
        let name = device.name_async().await.unwrap_or_default();
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
        candidates.push(Candidate {
            device,
            request,
            reply,
            name,
            serial: None,
        });
    }

    let named = candidates.iter().filter(|c| c.name.contains(want)).count();
    let mut chosen: Vec<Candidate> = if named > 0 {
        candidates
            .into_iter()
            .filter(|c| c.name.contains(want))
            .collect()
    } else {
        // No name has it, so it is a serial number, which only the keyboards can say.
        let mut found = Vec::new();
        for mut c in candidates {
            c.ask().await;
            if c.is_serial(want) {
                found.push(c);
            }
        }
        found
    };

    let candidate = match chosen.len() {
        0 => {
            return Err(anyhow!(
                "no connected keyboard{} has the minder service",
                if want.is_empty() {
                    String::new()
                } else {
                    format!(" named like, or with the serial number, {:?}", want)
                }
            ))
        }
        1 => chosen.remove(0),
        _ => {
            let mut list = Vec::new();
            for c in chosen.iter_mut() {
                c.ask().await;
                list.push(format!("  {}", c.describe()));
            }
            return Err(anyhow!(
                "{} connected keyboards match {:?}; choose one with `ble:` and its serial \
                 number:\n{}",
                list.len(),
                want,
                list.join("\n")
            ));
        }
    };

    let max = candidate.request.max_write_len_async().await?;
    if max < PACKET_SIZE {
        return Err(anyhow!(
            "{} takes writes of {} bytes, and minder's packets are {}: the ATT MTU is too small",
            candidate.name,
            max,
            PACKET_SIZE
        ));
    }
    let found = format!("{} (writes up to {} bytes)", candidate.describe(), max);
    Ok((adapter, candidate.device, candidate.request, candidate.reply, found))
}

/// How long a keyboard has to answer the `Hello` that asks its serial number.  An idle keyboard
/// answers its host only every half second or so (peripheral latency), and a request takes a
/// round trip.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Ask a keyboard for its serial number: the `info` of its `Hello`.
async fn ask_serial(request: &Characteristic, reply: &Characteristic) -> Result<String> {
    let mut notifications = reply.notify().await?;

    let mut packets = Vec::new();
    minder::hid_encode(
        &Request::Hello {
            version: minder::VERSION.to_string(),
        },
        Collect(&mut packets),
    )
    .map_err(|err| anyhow!("encoding Hello: {}", err))?;
    for packet in &packets {
        request.write_without_response(packet).await?;
    }

    let mut decoder = HidDecoder::new();
    let mut deadline = Box::pin(timer(HELLO_TIMEOUT).fuse());
    loop {
        futures::select! {
            note = notifications.next().fuse() => match note {
                Some(Ok(packet)) => {
                    if packet.is_empty() {
                        continue;
                    }
                    decoder.add_packet(&packet);
                    if decoder.is_ready() {
                        // A long poll pending from another client (TaipoTeacher) is answered
                        // first, with `NoEvent`, and the Hello after it; skip whatever comes
                        // before the Hello.
                        if let Ok(Reply::Hello { info, .. }) = decoder.decode::<Reply>() {
                            return Ok(info);
                        }
                        decoder = HidDecoder::new();
                    }
                }
                Some(Err(err)) => return Err(err.into()),
                None => return Err(anyhow!("the Bluetooth link closed")),
            },
            () = deadline => return Err(anyhow!("no answer to Hello")),
        }
    }
}

/// Collects the packets `hid_encode` makes.
struct Collect<'a>(&'a mut Vec<Vec<u8>>);

impl HidWrite for Collect<'_> {
    type Error = String;

    fn write_packet(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0.push(buf.to_vec());
        Ok(())
    }
}

/// A future that finishes after `delay`.  Nothing here has an async runtime with timers, so a
/// thread sleeps and then says so.
fn timer(delay: Duration) -> impl std::future::Future<Output = ()> {
    let (done, wait) = futures::channel::oneshot::channel();
    thread::spawn(move || {
        thread::sleep(delay);
        let _ = done.send(());
    });
    async move {
        let _ = wait.await;
    }
}

async fn characteristic(service: &bluest::Service, uuid: u128) -> Result<Characteristic> {
    service
        .discover_characteristics_with_uuid(Uuid::from_u128(uuid))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("the minder service has no characteristic {:x}", uuid))
}
