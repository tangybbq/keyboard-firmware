//! Keyminder.

use std::{fs::OpenOptions, io::Write, path::PathBuf, time::Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};
use keyminder::{EventPump, FlashImage, Flasher, Flow, VendorMinder};
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
    /// Record the key event log to a file.
    Log(LogArgs),
}

#[derive(clap::Args, Debug)]
struct LogArgs {
    /// Serial number of the keyboard to talk to.
    #[arg(short, long)]
    serial: String,
    /// File to append to.
    #[arg(short, long, default_value = "keylog.txt")]
    output: PathBuf,
    /// Raise an event once this many records are waiting.  1 is lowest latency.
    #[arg(long, default_value_t = 32)]
    watermark: u32,
    /// How long the device holds each poll open, in milliseconds.
    #[arg(long, default_value_t = 5000)]
    timeout_ms: u32,
    /// Stop after this many batches.  Runs until interrupted if not given.
    #[arg(long)]
    batches: Option<u32>,
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
        Commands::Log(args) => {
            log(args)?;
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
    let (mut pump, _hello) = EventPump::connect(&args.serial, args.timeout_ms)?;

    if args.interrupt {
        interrupt_check(pump.minder(), args)?;
    }

    if args.test > 0 {
        let reply: Reply = pump.minder().call(&Request::TestEvent {
            count: args.test,
            delay_ms: args.test_delay_ms,
        })?;
        println!("TestEvent: {:?}", reply);
    }

    // `--count 0` asks for no polls at all, which is how the interrupt check is run on
    // its own.  `run` would otherwise poll once before the callback could say to stop.
    if args.count == Some(0) {
        return Ok(());
    }

    let mut remaining = args.count;
    let mut start = Instant::now();
    pump.run(|_minder, event| {
        match event {
            Some(event) => println!("[{:7.3}s] {:?}", start.elapsed().as_secs_f64(), event),
            None => println!("[{:7.3}s] no event", start.elapsed().as_secs_f64()),
        }
        start = Instant::now();
        match remaining {
            Some(0) | Some(1) => Flow::Stop,
            Some(ref mut n) => {
                *n -= 1;
                Flow::Continue
            }
            None => Flow::Continue,
        }
    })?;

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

/// Record the key event log to a file.
///
/// taipo-teacher.md phase 2.  This is the one-shot form; phase 3 makes it a background loop
/// in the collector, which is why the fetch/append/ack cycle lives in the library rather
/// than here.
///
/// **This records everything typed on the keyboard.**  The device holds it in RAM only and
/// loses it at power off, and this file is local, but it is a keylogger and the chord codes
/// are the letters.  Logging is off on the device until this asks for it, and turned back
/// off on the way out.
fn log(args: &LogArgs) -> Result<()> {
    let (mut pump, hello) = EventPump::connect(&args.serial, args.timeout_ms)?;
    if !hello.supports(minder::cap::KEY_LOG) {
        return Err(anyhow::anyhow!(
            "device does not support the {:?} capability",
            minder::cap::KEY_LOG
        ));
    }
    let (boot_id, info) = match &hello {
        Reply::Hello { boot_id, info, .. } => (boot_id.unwrap_or(0), info.clone()),
        other => return Err(anyhow::anyhow!("Unexpected reply to Hello: {:?}", other)),
    };

    // Anything already buffered predates this session: logging was off, so it is left over
    // from an earlier one, and writing it under this header would date it wrongly.
    let discarded = discard_buffered(pump.minder())?;
    if discarded > 0 {
        println!("discarded {discarded} records left from an earlier session");
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.output)?;
    file.write_all(
        keyminder::logfile::session_header(boot_id, keyminder::layouts_fingerprint(), &info)
            .as_bytes(),
    )?;

    println!("Recording to {}.  Interrupt to stop.", args.output.display());
    let reply: Reply = pump.minder().call(&Request::SetLogging {
        enabled: true,
        watermark: args.watermark,
    })?;
    if !matches!(reply, Reply::Ok) {
        return Err(anyhow::anyhow!("SetLogging refused: {:?}", reply));
    }

    let mut offset_ms = 0u64;
    let mut batches = args.batches;
    let mut total = 0usize;

    let result = pump.run(|minder, _event| {
        // Drain whether or not an event came: an expired poll still means it is time to
        // look, and the watermark only bounds how long records sit unfetched.
        match drain_all(minder, &mut file, &mut offset_ms, &mut total) {
            Ok(()) => (),
            Err(e) => {
                eprintln!("drain failed: {e}");
                return Flow::Stop;
            }
        }
        match batches {
            Some(0) | Some(1) => Flow::Stop,
            Some(ref mut n) => {
                *n -= 1;
                Flow::Continue
            }
            None => Flow::Continue,
        }
    });

    // Leave the device as it was found, whether the loop ended well or badly.
    let _: Result<Reply> = pump.minder().call(&Request::SetLogging {
        enabled: false,
        watermark: 0,
    });
    println!("\n{total} records written to {}", args.output.display());
    result
}

/// Throw away whatever the device still holds, so a new session starts clean.
///
/// Records survive a `SetLogging{enabled:false}` -- disabling stops recording, it does not
/// discard what was not acked -- which is right, but means a later session would otherwise
/// find them and file them under its own header and wall clock.
fn discard_buffered(minder: &mut VendorMinder) -> Result<usize> {
    let mut total = 0;
    loop {
        let reply: Reply = minder.call(&Request::GetEventLog { max_bytes: 3200 })?;
        let Reply::EventLog { seq, records, remaining, .. } = reply else {
            return Err(anyhow::anyhow!("Unexpected reply to GetEventLog: {:?}", reply));
        };
        let count = records.len() / minder::keylog::RECORD_SIZE;
        if count == 0 {
            return Ok(total);
        }
        total += count;
        let _: Reply = minder.call(&Request::EventLogAck {
            through_seq: seq.wrapping_add(count as u32 - 1),
        })?;
        if remaining == 0 {
            return Ok(total);
        }
    }
}

/// Fetch, append and ack, repeating while the device says more is waiting.
///
/// Acking only after the write lands is the point of acking separately: a crash between
/// the fetch and the write refetches the same records rather than losing them.
fn drain_all(
    minder: &mut VendorMinder,
    file: &mut std::fs::File,
    offset_ms: &mut u64,
    total: &mut usize,
) -> Result<()> {
    loop {
        let reply: Reply = minder.call(&Request::GetEventLog { max_bytes: 3200 })?;
        let Reply::EventLog {
            seq,
            dropped,
            records,
            remaining,
            ..
        } = reply
        else {
            return Err(anyhow::anyhow!("Unexpected reply to GetEventLog: {:?}", reply));
        };

        if dropped > 0 {
            file.write_all(keyminder::logfile::gap(dropped, seq).as_bytes())?;
        }
        if records.is_empty() {
            return Ok(());
        }

        let count = records.len() / minder::keylog::RECORD_SIZE;
        let text = keyminder::logfile::format_batch(&records, offset_ms);
        file.write_all(text.as_bytes())?;
        file.flush()?;
        *total += count;

        let through = seq.wrapping_add(count as u32 - 1);
        let ack: Reply = minder.call(&Request::EventLogAck { through_seq: through })?;
        if !matches!(ack, Reply::Ok) {
            return Err(anyhow::anyhow!("EventLogAck refused: {:?}", ack));
        }

        print!("\r{total} records");
        let _ = std::io::stdout().flush();

        if remaining == 0 {
            return Ok(());
        }
    }
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
