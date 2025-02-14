//! USB interface

use alloc::{boxed::Box, vec::Vec};
use bbq_keyboard::{KeyAction, Keyboard, Mods};
use embassy_futures::join::join;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Receiver};
use embassy_usb::{class::hid::{HidReaderWriter, HidWriter, ReportId, RequestHandler, State}, control::OutResponse, Builder, Config, Handler};
use static_cell::StaticCell;
use usbd_hid::descriptor::KeyboardReport;

use crate::Irqs;
use crate::logging::{info, warn};

/// Channel for receipt of key events to be sent over HID.
pub type KeyReceiver = Receiver<'static, CriticalSectionRawMutex, KeyAction, 8>;

/// Setup the USB driver.  We'll make things heap allocated just to simplify things, and because
/// there is no particular reason to go out of our way to avoid allocation.
#[embassy_executor::task]
pub async fn setup_usb(usb: USB, unique: &'static str, keys_rec: KeyReceiver) {
    let driver = Driver::new(usb, Irqs);

    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("TangyBBQ");
    config.product = Some("Jolt Keyboard");
    config.serial_number = Some(unique);
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let config_descriptor_buf = Box::new([0; 256]);
    let bos_descriptor_buf = Box::new([0; 256]);
    let msos_descriptor_buf = Box::new([0; 256]);
    let control_buf = Box::new([0; 64]);
    let mut request_handler = JoltRequestHandler::new();
    static DEVICE_HANDLER: StaticCell<JoltDeviceHandler> = StaticCell::new();
    let device_handler = DEVICE_HANDLER.init(JoltDeviceHandler::new());

    let mut builder = Builder::new(
        driver,
        config,
        Box::leak(config_descriptor_buf),
        Box::leak(bos_descriptor_buf),
        Box::leak(msos_descriptor_buf),
        Box::leak(control_buf),
    );
    builder.handler(device_handler);

    let config = embassy_usb::class::hid::Config {
        // report_descriptor: KeyboardReport::desc(),
        report_descriptor: USB_KEYB_HID_DESC,
        request_handler: None,
        poll_ms: 10,
        max_packet_size: 64,
    };
    // info!("Descriptor: {=[u8]:#02x}", config.report_descriptor);
    let state = Box::leak(Box::new(State::new()));
    let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, state, config);

    // Add a bulk endpoint.
    let mut function = builder.function(0xFF, 0, 0);
    let mut interface = function.interface();
    let mut alt = interface.alt_setting(0xff, 0, 0, None);
    let read_ep = alt.endpoint_bulk_out(64);
    let write_ep = alt.endpoint_bulk_in(64);
    drop(function);

    let _ = read_ep;
    let _ = write_ep;

    let mut usb = builder.build();

    let usb_fut = usb.run();

    let (reader, mut writer) = hid.split();

    let in_fut = async {
        loop {
            send_usb(keys_rec.receive().await, &mut writer).await;
        }
        /*
        let mut ticker = Ticker::every(Duration::from_secs(15));
        loop {
            ticker.next().await;

            if false {
                let report = KeyboardReport {
                    keycodes: [4, 0, 0, 0, 0, 0],
                    leds: 0,
                    modifier: 0,
                    reserved: 0,
                };
                match writer.write_serialize(&report).await {
                    Ok(()) => (),
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }

                // Just send the key up immediately.
                let report = KeyboardReport {
                    keycodes: [0, 0, 0, 0, 0, 0],
                    leds: 0,
                    modifier: 0,
                    reserved: 0,
                };
                match writer.write_serialize(&report).await {
                    Ok(()) => (),
                    Err(e) => warn!("Failed to send report: {:?}", e),
                }
            }
        }
        */
    };

    let out_fut = async {
        reader.run(false, &mut request_handler).await;
    };

    join(usb_fut, join(in_fut, out_fut)).await;
}

// This is the standard BOOT keyboard report descriptor.
static USB_KEYB_HID_DESC: &[u8] = &[
    0x05, 0x01,   // Usage Page (Generic Desktop)
    0x09, 0x06,   // Usage (Keyboard)
    0xA1, 0x01,   // Collection (Application)
    0x05, 0x07,   // Usage Page (Key Codes)
    0x19, 0xE0,   // Usage Minimum (Left Control)
    0x29, 0xE7,   // Usage Maximum (Right GUI)
    0x15, 0x00,   // Logical Minimum (0)
    0x25, 0x01,   // Logical Maximum (1)
    0x75, 0x01,   // Report Size (1)
    0x95, 0x08,   // Report Count (8)
    0x81, 0x02,   // Input (Data, Variable, Absolute) - Modifier keys
    0x95, 0x01,   // Report Count (1)
    0x75, 0x08,   // Report Size (8)
    0x81, 0x01,   // Input (Constant) - Reserved byte
    0x95, 0x06,   // Report Count (6)
    0x75, 0x08,   // Report Size (8)
    0x15, 0x00,   // Logical Minimum (0)
    0x25, 0x65,   // Logical Maximum (101)
    0x05, 0x07,   // Usage Page (Key Codes)
    0x19, 0x00,   // Usage Minimum (0)
    0x29, 0x65,   // Usage Maximum (101)
    0x81, 0x00,   // Input (Data, Array)
    0xC0          // End Collection
];

async fn send_usb(key: KeyAction, writer: &mut HidWriter<'static, Driver<'static, USB>, 8>) {
    let report = match key {
        KeyAction::KeyPress(code, mods) => {
            KeyboardReport {
                keycodes: [code as u8, 0, 0, 0, 0, 0],
                modifier: mods.bits(),
                leds: 0,
                reserved: 0,
            }
        }
        KeyAction::KeyRelease => {
            KeyboardReport {
                keycodes: [0, 0, 0, 0, 0, 0],
                modifier: 0,
                leds: 0,
                reserved: 0,
            }
        }
        KeyAction::ModOnly(mods) => {
            KeyboardReport {
                keycodes: [0, 0, 0, 0, 0, 0],
                modifier: mods.bits(),
                leds: 0,
                reserved: 0,
            }
        }
        KeyAction::KeySet(keys) => keyset_to_hid(keys),
        KeyAction::Stall => return,
    };

    info!("Report: {:?} {:x}", report.keycodes, report.modifier);
    match writer.write_serialize(&report).await {
        Ok(()) => (),
        Err(e) => warn!("Failed to send HID report: {:?}", e),
    }
}

fn keyset_to_hid(keys: Vec<Keyboard>) -> KeyboardReport {
    let mut codes: heapless::Vec<u8, 6> = heapless::Vec::new();
    let mut mods = Mods::empty();
    for key in keys {
        match key {
            Keyboard::LeftControl => mods |= Mods::CONTROL,
            Keyboard::LeftShift => mods |= Mods::SHIFT,
            Keyboard::LeftAlt => mods |= Mods::ALT,
            Keyboard::LeftGUI => mods |= Mods::GUI,
            key => {
                // Explicitly ignore more than 6 keys.
                let _ = codes.push(key as u8);
            }
        }
    }

    // Fill the rest of the report with zeros.
    while codes.len() < 6 {
        let _ = codes.push(0);
    }

    KeyboardReport {
        keycodes: codes.into_array().unwrap(),
        modifier: mods.bits(),
        leds: 0,
        reserved: 0,
    }
}

struct JoltRequestHandler;

impl JoltRequestHandler {
    fn new() -> JoltRequestHandler {
        JoltRequestHandler
    }
}

impl RequestHandler for JoltRequestHandler {
    fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
        info!("HID get_report: id:{:?}, buf: {:?}", id, buf);
        None
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("HID set_report: id:{:?}, data: {:?}", id, data);
        OutResponse::Rejected
    }
}

struct JoltDeviceHandler;

impl JoltDeviceHandler {
    fn new() -> JoltDeviceHandler {
        JoltDeviceHandler
    }
}

impl Handler for JoltDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        info!("USB enabled: {:?}", enabled);
    }

    fn reset(&mut self) {
        info!("USB Reset");
    }

    fn addressed(&mut self, addr: u8) {
        info!("USB Addressed: {:x}", addr);
    }

    fn configured(&mut self, configured: bool) {
        info!("USB configured: {:?}", configured);
    }

    fn suspended(&mut self, suspended: bool) {
        info!("USB suspended: {:?}", suspended);
    }

    fn remote_wakeup_enabled(&mut self, enabled: bool) {
        info!("USB remote wakeup enabled: {:?}", enabled);
    }

    // Control messages can be handled as well.
}
