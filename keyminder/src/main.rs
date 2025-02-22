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

    println!("Checking {} bytes ({} pages)",
    dicts.data.len(),
    dicts.data.len().div_ceil(4096));

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
