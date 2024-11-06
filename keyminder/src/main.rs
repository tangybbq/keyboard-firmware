//! Keyminder.

use std::{io::{Error, Write}, time::Duration};

use anyhow::Result;
use minder::{Reply, Request, SerialDecoder, SerialWrite};

fn main() -> Result<()> {
    // let mut port = serialport::new("/dev/cu.usbmodem11404", 115200).open()?;
    // let mut port = serialport::new("/dev/cu.usbmodem112104", 115200).open()?;
    let mut port = serialport::new("/dev/cu.usbmodem104", 115200).open()?;
    // let mut port = serialport::new("/dev/cu.usbmodem1104", 115200).open()?;

    let req = Request::Hello {
        version: minder::VERSION.to_string(),
    };

    minder::serial_encode(&req, WritePort(&mut port), true)?;

    port.set_timeout(Duration::from_secs(120 * 60 * 60 * 24))?;

    let mut dec = SerialDecoder::new();

    let mut buffer = vec![0u8; 256];
    loop {
        let count = match port.read(&mut buffer) {
            Ok(count) => count,
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(e) => Err(e)?
        };

        for &byte in &buffer[..count] {
            if let Some(packet) = dec.add_decode::<Reply>(byte) {
                // println!("{:?}", packet);
                show(&packet);
            }
        }
    }

    Ok(())
}

fn show(msg: &Reply) {
    match msg {
        Reply::Hello { version, info } => {
            println!("Hello: {}, {}", version, info);
        }
        Reply::Log { message } => {
            println!("{}", message);
        }
    }
}

// Write wrapper.
struct WritePort<W>(W);

impl<W: Write> SerialWrite for WritePort<W> {
    type Error = Error;

    fn write_all(&mut self, buf: &[u8]) -> std::result::Result<(), Self::Error> {
        self.0.write_all(buf)
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
