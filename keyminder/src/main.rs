//! Keyminder.

use std::{io::Write, time::{Duration, Instant}};

use anyhow::Result;
use clap::{Parser, Subcommand};
use keyminder::{FlashImage, Flasher, VendorMinder};
use minder::{Reply, Request};

#[derive(Parser)]
#[command(name = "keyminder")]
#[command(about = "Utility for speaking with bbq keyboards")]
struct Cli {
    /// The uart port to use.
    #[arg(long, default_value = "")]
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
    /// Long poll the device for events.
    Poll(PollArgs),
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
struct PollArgs {
    /// Serial number of the keyboard to talk to.
    #[arg(short, long)]
    serial: String,
    /// How long the device should hold each poll open, in milliseconds.
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u32,
    /// Stop after this many polls.  Runs until interrupted if not given.
    #[arg(long)]
    count: Option<u32>,
    /// Ask the device to raise this many test events before polling.
    #[arg(long, default_value_t = 0)]
    test: u32,
    /// How long the device should wait before raising the test events.
    ///
    /// The default is long enough that they land while the first poll is already pending, which
    /// is the case worth exercising.
    #[arg(long, default_value_t = 500)]
    test_delay_ms: u32,
    /// Check that a request sent while a poll is pending is answered in order.
    #[arg(long)]
    interrupt: bool,
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
        Commands::Poll(args) => {
            poll(args)?;
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
    let mut minder = VendorMinder::new(&args.serial)?;
    // A device left mid-conversation by a killed client still owes a reply; without this
    // the first one read here is that one, and everything after is off by one.
    minder.drain()?;

    let reply: Reply = minder.call(&Request::Hello {
        version: minder::VERSION.to_string(),
    })?;

    let Reply::Hello {
        version,
        info,
        boot_id,
        layout_fingerprint,
        capabilities,
    } = &reply
    else {
        return Err(anyhow::anyhow!("Unexpected reply to Hello: {:?}", reply));
    };

    println!("device:       {}", info);
    println!("protocol:     {} (host {})", version, minder::VERSION);
    match boot_id {
        Some(id) => println!("boot id:      {:#018x}", id),
        None => println!("boot id:      not reported"),
    }
    match capabilities {
        Some(caps) => println!("capabilities: {}", caps.join(", ")),
        None => println!("capabilities: not reported (firmware predates the list)"),
    }

    // The tables the device is running have to be the ones the host is reading, or
    // replaying a key log through them is quietly wrong rather than visibly wrong.
    match (layout_fingerprint, keyminder::layouts_fingerprint()) {
        (Some(dev), Some(host)) if dev == &host => {
            println!("layout:       {:#018x}  (matches layouts.json)", dev)
        }
        (Some(dev), Some(host)) => println!(
            "layout:       {:#018x}  MISMATCH: layouts.json has {:#018x}",
            dev, host
        ),
        (Some(dev), None) => println!("layout:       {:#018x}  (layouts.json unreadable)", dev),
        (None, _) => println!("layout:       not reported"),
    }

    Ok(())
}

/// Long poll the device for events.
///
/// This is the host side of taipo-teacher.md phase 1a, and until phase 2 raises real events the
/// only way to see the path work is `--test`, which asks the device to raise some.
fn poll(args: &PollArgs) -> Result<()> {
    let mut minder = VendorMinder::new(&args.serial)?;
    minder.drain()?;

    // The device's poll has to be allowed to expire before the read does, or every poll looks
    // like a USB timeout.
    minder.read_timeout = Duration::from_millis(args.timeout_ms as u64) + Duration::from_secs(5);

    if args.interrupt {
        interrupt_check(&mut minder, args)?;
    }

    if args.test > 0 {
        let reply: Reply = minder.call(&Request::TestEvent {
            count: args.test,
            delay_ms: args.test_delay_ms,
        })?;
        println!("TestEvent: {:?}", reply);
    }

    let mut remaining = args.count;
    loop {
        match remaining {
            Some(0) => break,
            Some(ref mut n) => *n -= 1,
            None => (),
        }

        let start = Instant::now();
        let reply: Reply = minder.call(&Request::GetEvent {
            timeout_ms: args.timeout_ms,
        })?;
        println!("[{:7.3}s] {:?}", start.elapsed().as_secs_f64(), reply);
    }

    Ok(())
}

/// Check the ordering rule that makes the long poll work without tagged requests.
///
/// Sends a `GetEvent` and then, without waiting for its reply, a `Hello`.  The device owes one
/// reply per request, in order, so a `NoEvent` for the interrupted poll must come back first, and
/// the `Hello` reply second.
fn interrupt_check(minder: &mut VendorMinder, args: &PollArgs) -> Result<()> {
    println!("Interrupt check: GetEvent, then Hello without waiting");

    minder.send(&Request::GetEvent {
        timeout_ms: args.timeout_ms,
    })?;
    minder.send(&Request::Hello {
        version: minder::VERSION.to_string(),
    })?;

    let start = Instant::now();
    let first: Reply = minder.recv()?;
    println!("  first : [{:7.3}s] {:?}", start.elapsed().as_secs_f64(), first);
    let second: Reply = minder.recv()?;
    println!("  second: [{:7.3}s] {:?}", start.elapsed().as_secs_f64(), second);

    match (&first, &second) {
        (Reply::NoEvent, Reply::Hello { .. }) => println!("  ok"),
        _ => println!("  WRONG: expected NoEvent then Hello"),
    }

    Ok(())
}

fn dict(args: &DictArgs) -> Result<()> {
    let mut flasher = Flasher::new(&args.serial)?;

    let dicts = match args.dict {
        Dictionary::Main => FlashImage::load("../bbq-tool/dicts.bin", 0x1030_0000)?,
        Dictionary::User => FlashImage::load("../bbq-tool/user-dict.bin", 0x1020_0000)?,
    };

    println!(
        "Checking {} bytes ({} pages)",
        dicts.data.len(),
        dicts.data.len().div_ceil(4096)
    );

    let updates = flasher.check(&dicts)?;

    if updates.is_empty() {
        return Ok(());
    }

    let len = updates.len();

    for (i, update) in updates.iter().enumerate() {
        print!("Update [{}/{}]\r", i, len);
        let _ = std::io::stdout().flush();
        flasher.write(update.data, update.offset)?;
    }
    println!("");

    flasher.reset()?;

    Ok(())
}
