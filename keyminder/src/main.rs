//! Keyminder.

use std::{io::Write, path::Path, time::Duration};

use anyhow::Result;
use clap::{Parser, Subcommand};
use minder::{Reply, Request};
use minicbor::{Decode, Encode};
use rusb::{DeviceHandle, Direction, GlobalContext};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(name = "keyminder")]
#[command(about = "Utility for speaking with bbq keyboards")]
struct Cli {
    /// The uart port to use.
    #[arg(long, default_value="")]
    port: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan all USB devices.
    Scan,
    /// Chat with the device over USB bulk.
    Chat(ChatArgs),
    /// Dictionary Upgraders
    Dict(DictArgs),
}

#[derive(clap::Args, Debug)]
struct ChatArgs {
    /// Serial number of the keyboard to talk to.
    #[arg(short, long)]
    serial: String,
}

#[derive(clap::Args, Debug)]
struct DictArgs {
    /// Serial number of the keyboard to talk to.
    #[arg(short, long)]
    serial: String,
    /// Which dictionary to update
    #[arg(short, long)]
    dict: Dictionary,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Dictionary {
    Main,
    User,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Scan => {
            scan()?;
        }
        Commands::Chat(args) => {
            chat(args)?;
        }
        Commands::Dict(args) => {
            dict(args)?;
        }
    }

    Ok(())
}

fn scan() -> Result<()> {
    for dev in rusb::devices()?.iter() {
        let desc = dev.device_descriptor()?;
        if desc.vendor_id() != 0xc0de || desc.product_id() != 0xcafe {
            continue;
        }
        // println!("{:?}", dev);
        // println!("  dev desc: {:02x?}", desc);
        let serial = desc.serial_number_string_index().unwrap();
        {
            let handle = dev.open()?;
            let serial = handle.read_string_descriptor_ascii(serial)?;
            println!("  serial: {:?}", serial);
        }
        /*
        let conf = dev.active_config_descriptor()?;
        // println!("  conf: {:?}", conf);
        for int in conf.interfaces() {
            // println!("    int: {:?}", int.number());
            for desc in int.descriptors() {
                // println!("     desc: {:?}", desc);
                if desc.class_code() == 0xff {
                    for endp in desc.endpoint_descriptors() {
                        println!("        endp: {:?}", endp);
                    }
                }
            }
        }
        */
    }
    Ok(())
}

fn chat(args: &ChatArgs) -> Result<()> {
    println!("Opening {:?}", args);

    let mut minder = VendorMinder::new(&args.serial)?;

    // println!("send: {:#x}, recv: {:#x}", minder.send, minder.recv);

    let req = Request::Hello {
        version: minder::VERSION.to_string(),
    };
    let reply: Reply = minder.call(&req)?;
    println!("Hello: {:?}", reply);

    Ok(())
}

fn dict(args: &DictArgs) -> Result<()> {
    let mut flasher = Flasher::new(&args.serial)?;

    let dicts = match args.dict {
        Dictionary::Main => FlashImage::load("../bbq-tool/dicts.bin", 0x1030_0000)?,
        Dictionary::User => FlashImage::load("../bbq-tool/user-dict.bin", 0x1020_0000)?,
    };

    flasher.check(&dicts)?;

    // flasher.reset()?;
    // flasher.hash(0x10300000, 4096)?;

    Ok(())
}

/// An image to be loaded into flash at a given offset.
struct FlashImage {
    data: Vec<u8>,
    offset: u32,
}

impl FlashImage {
    fn load<P: AsRef<Path>>(name: P, offset: u32) -> Result<Self> {
        let data = std::fs::read(name)?;
        Ok(Self { data, offset })
    }
}

struct Flasher {
    minder: VendorMinder,
}

impl Flasher {
    fn new(serial: &str) -> Result<Self> {
        let mut minder = VendorMinder::new(serial)?;
        minder.drain()?;
        Ok(Self {
            minder,
        })
    }

    /// Ask the device to reset itself.
    #[allow(dead_code)]
    fn reset(&mut self) -> Result<()> {
        let reply: Reply = self.minder.call(&Request::Reset)?;
        println!("Reset: {:?}", reply);

        Ok(())
    }

    /// Hash a region of the flash on the device.
    fn hash(&mut self, offset: u32, size: u32) -> Result<[u8; 32]> {
        let reply: Reply = self.minder.call(&Request::Hash { offset, size })?;
        match reply {
            Reply::Hash { hash } => Ok(hash.into()),
            e => Err(anyhow::anyhow!("Error hashing: {:?}", e)),
        }
    }

    /// Work through the image, building a map of what pages need to be updated.
    fn check(&mut self, image: &FlashImage) -> Result<()> {
        let mut offset = 0;
        let length = image.data.len();
        let total_blocks = length.div_ceil(4096);
        let mut block = 0;
        let mut to_update = 0;

        // Before getting too far, try hashing the entire image to see if anything needs to be done.
        let mut digest = Sha256::new();
        digest.update(&image.data);
        let digest: [u8; 32] = digest.finalize().into();

        let thash = self.hash(image.offset, image.data.len() as u32)?;
        if digest == thash {
            println!("Image is up to date");
            return Ok(());
        }

        while offset < length {
            let count = (image.data.len() - offset).min(4096);
            let slice = &image.data[offset..offset + count];

            print!("[{:4}/{:4}] {} dirty\r", block, total_blocks, to_update);
            let _ = std::io::stdout().flush();

            let mut digest = Sha256::new();
            digest.update(slice);
            let digest: [u8; 32] = digest.finalize().into();

            // Ask the target to hash this.
            let thash = self.hash(offset as u32 + image.offset, count as u32)?;

            if digest != thash {
                // println!("differs: {:#08x} {:#04x}", offset + image.offset as usize, count);
                to_update += 1;
            }

            offset += count;
            block += 1;
        }

        Ok(())
    }
}

struct VendorMinder {
    /// The handle of the keyboard we a talking to.
    handle: DeviceHandle<GlobalContext>,

    /// The receive buffer.  Zero-copy structs will reference directly into this.
    rbuf: Vec<u8>,

    /// Endpoints to use.
    send: u8,
    recv: u8,
}

impl VendorMinder {
    /// Attempt to open the keyboard with the given serial number.
    pub fn new(serial: &str) -> Result<Self> {
        for dev in rusb::devices()?.iter() {
            let desc = dev.device_descriptor()?;
            if desc.vendor_id() != 0xc0de || desc.product_id() != 0xcafe {
                continue;
            }

            // Fetch the serial number.
            let serial_index = desc.serial_number_string_index().unwrap();
            let handle = dev.open()?;
            let dev_serial = handle.read_string_descriptor_ascii(serial_index)?;
            if dev_serial != serial {
                continue;
            }

            // Dig down and get the endpoint descriptors.
            let mut send = None;
            let mut recv = None;
            let conf = dev.active_config_descriptor()?;
            for int in conf.interfaces() {
                for desc in int.descriptors() {
                    if desc.class_code() == 0xff {
                        for endp in desc.endpoint_descriptors() {
                            // println!("end: {:?}", endp);
                            if endp.direction() == Direction::In {
                                recv = Some(endp.address());
                            } else {
                                send = Some(endp.number());
                            }
                        }

                        // Be sure to claim this interface.
                        handle.claim_interface(int.number())?;
                    }
                }
            }

            return Ok(Self {
                handle,
                rbuf: vec![0u8; 532],
                send: send.unwrap(),
                recv: recv.unwrap(),
            });
        }

        Err(anyhow::anyhow!("Unable to find device with given serial"))
    }

    /// Perform a round trip communication.
    pub fn call<'d, In, Out>(&'d mut self, req: &Out) -> Result<In>
        where
            In: Decode<'d, ()>,
            Out: Encode<()>,
    {
        let mut obuf = Vec::new();
        minicbor::encode(req, &mut obuf)?;
        let count = self.handle.write_bulk(self.send, &obuf, Duration::from_secs(1))?;
        if count != obuf.len() {
            panic!("Short write");
        }

        let count = self.handle.read_bulk(self.recv, &mut self.rbuf, Duration::from_secs(15))?;
        let inbuf = &self.rbuf[..count];

        Ok(minicbor::decode(inbuf)?)
    }

    // Drain any pending data on the bulk endpoint.
    fn drain(&mut self) -> Result<()> {
        loop {
            match self.handle.read_bulk(self.recv, &mut self.rbuf, Duration::from_millis(2)) {
                Ok(_) => (),
                Err(_) => break,
            }
        }

        Ok(())
    }
}

// Zephyr vid: 2fe3
// bbq keyboard: 4201
// bbq keyoard test: 4202

/*
const JOLT_VID: u16 = 0x2fe3;
const JOLT_PID: u16 = 0x4202;

// Alternatively, look for the interface with the right data.
static PREFIX: [u8; 6] = [0x06, 0x4d, 0xff, 0x0a, 0x4e, 0x44];

fn main() -> Result<()> {
    for device in rusb::devices()?.iter() {
        let desc = device.device_descriptor()?;

        if desc.vendor_id() != JOLT_VID || desc.product_id() != JOLT_PID {
            continue;
        }

        println!("Bus {:03} Device {:03} ID {:04x}:{:04x} {:#?}",
                 device.bus_number(),
                 device.address(),
                 desc.vendor_id(),
                 desc.product_id(),
                 desc,
                 );

        let config = device.active_config_descriptor()?;
        println!("config: {:#?}", config);

        let dev = device.open()?;

        let mut in_ep = None;
        let mut out_ep = None;

        for iface in config.interfaces() {
            // Try to read the HID Descriptior.
            let mut buf = vec![0u8; 256];
            let request_type = rusb::request_type(Direction::In, RequestType::Standard, Recipient::Interface);
            let ret = dev.read_control(
                request_type,
                0x06,
                0x2122,
                iface.number() as u16,
                &mut buf,
                Duration::from_secs(1),
            );
            if ret.is_err() {
                continue;
            }
            let count = ret.unwrap();

            println!("iface {}", iface.number());

            println!("HID: {:02x?}", &buf[..count]);

            // Really didn't need to read that, I don't think.
            let ret = dev.read_control(
                request_type,
                0x06,
                0x2200,
                iface.number() as u16,
                &mut buf,
                Duration::from_secs(1),
            );
            if ret.is_err() {
                continue;
            }
            let count = ret.unwrap();

            println!("Report Desc: {:02x?}", &buf[..count]);

            // If this isn't the one we care about, continue.
            if !buf[..count].starts_with(&PREFIX) {
                continue;
            }

            for desc in iface.descriptors() {
                if desc.class_code() != 3 {
                    // continue;
                }
                // println!("  extra: {:#02x?}", desc.extra());
                for endpoint in desc.endpoint_descriptors() {
                    if endpoint.direction() == Direction::In {
                        println!("  IN: {:02x}", endpoint.number());
                        in_ep = Some(endpoint.address());
                    } else {
                        println!(" OUT: {:02x}", endpoint.number());
                        out_ep = Some(endpoint.address());
                    }
                    println!("  endpoint {:#x?}", endpoint);
                }
            }
            println!("HID control: {:?} {:?}", in_ep, out_ep);
        }

        dev.claim_interface(out_ep.unwrap())?;
        minder::hid_encode(&[Request::Hello { version: minder::VERSION.to_string() }],
        HidDev(&dev, out_ep.unwrap()))?;

        /*
        let dev = device.open()?;
        println!("Config: {}", dev.active_configuration()?);
        */
    }

    Ok(())
}

struct HidDev<'a, T: UsbContext>(&'a DeviceHandle<T>, u8);

impl<'a, T: UsbContext> HidWrite for HidDev<'a, T> {
    type Error = rusb::Error;

    fn write_packet(&mut self, buf: &[u8]) -> std::result::Result<(), Self::Error> {
        let count = self.0.write_interrupt(self.1, buf, Duration::from_secs(1))?;
        if count != buf.len() {
            println!("Warning: short write");
        }
        Ok(())
    }
}
*/

/*
fn main() -> Result<()> {
    let api = HidApi::new()?;

    let mut minder = None;
    for device in api.device_list() {
        if device.vendor_id() != 0x2fe3 {
            continue;
        }
        if device.product_id() != 0x4202 {
            continue;
        }
        /*
        println!("{:?}: {:x?} sn:{:?}: {:x?}, i:{}, ",
                 device.product_string(),
                 device.path(),
                 device.serial_number(),
                 device,
                 device.interface_number(),
                 );
        */

        // Try opening it, and get the get the information.
        let dev = match device.open_device(&api) {
            Ok(dev) => dev,
            // Allow for errors here, because the keyboard/mouse endpoints aren't usable.
            Err(_) => continue,
        };
        let mut desc = [0u8; MAX_REPORT_DESCRIPTOR_SIZE];
        let len = dev.get_report_descriptor(&mut desc)?;
        let desc = &desc[..len];
        if desc.starts_with(&PREFIX) {
            println!("i:{}, desc:{:02x?}", device.interface_number(), &desc[..len]);
            minder = Some(dev);
        }

        // Store the last one here.  We might want to try to distinguish multiple keyboards since
        // that will happen a lot while debugging.
        // minder = Some(dev);
    }

    let minder = minder.unwrap();

    // Send a single Hello request.
    minder::hid_encode(
        &[Request::Hello {
            version: minder::VERSION.to_string(),
        },
        ],
        HidDev(&minder),
    ).unwrap();

    // Send a larger one to see if we drop things.
    let mut req = Vec::new();
    let tmp = &[Request::Hello { version: minder::VERSION.to_string() }];
    for _ in 0..4 {
        req.push(&tmp);
    }
    minder::hid_encode(&req[..], HidDev(&minder)).unwrap();

    /*
    // Read and print messages.
    let mut buf = [0u8; 64];
    loop {
        let len = minder.read(&mut buf)?;
        println!("rep: {:02x?}", &buf[..len]);
    }
    */

    Ok(())
}

struct HidDev<'a>(&'a HidDevice);

impl<'a> HidWrite for HidDev<'a> {
    type Error = HidError;

    fn write_packet(&mut self, buf: &[u8]) -> std::result::Result<(), Self::Error> {
        let count = self.0.write(buf)?;
        if count != buf.len() {
            println!("Warning: short hid write");
        }
        Ok(())
    }
}
*/
