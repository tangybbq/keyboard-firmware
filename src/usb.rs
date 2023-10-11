// Usb HID management.

use crate::hal::usb::UsbBus;
use crate::Event;
use arraydeque::ArrayDeque;
use defmt::{info, warn};
use frunk::{HNil, HCons};
use usb_device::{class_prelude::{UsbBusAllocator, UsbClass}, prelude::{UsbDeviceBuilder, UsbVidPid, UsbDevice, UsbDeviceState}};
use usbd_human_interface_device::{usb_class::{UsbHidClassBuilder, UsbHidClass}, device::{keyboard::{NKROBootKeyboardConfig, NKROBootKeyboard}, DeviceClass}, page::Keyboard, UsbHidError};

// Type of the device list, which is internal to usbd_human_interface_device.
type InterfaceList<'a> = HCons<NKROBootKeyboard<'a, UsbBus>, HNil>;

pub struct UsbHandler<'a> {
    dev: UsbDevice<'a, UsbBus>,
    hid: UsbHidClass<'a, UsbBus, InterfaceList<'a>>,
    state: Option<UsbDeviceState>,
    keys: ArrayDeque<Event, 128>,
}

impl<'a> UsbHandler<'a> {
    pub fn new<'aa>(usb_bus : &'aa UsbBusAllocator<UsbBus>) -> UsbHandler<'aa> {
        let keyboard = UsbHidClassBuilder::new()
            .add_device(
                NKROBootKeyboardConfig::default(),
            )
            .build(usb_bus);
        let usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x003))
            .manufacturer("https://github.com/tangybbq/")
            .product("Proto 2")
            .serial_number("development")
            .device_class(0)
            .max_power(500)
            .build();
        UsbHandler {
            hid: keyboard,
            dev: usb_dev,
            state: None,
            keys: ArrayDeque::new(),
        }
    }

    /// Add a sequence of events to be shipped off to the USB host.  If the
    /// deque is full, log a message, but discard.
    pub(crate) fn enqueue<I: Iterator<Item = Event>>(&mut self, events: I) {
        for key in events {
            if self.keys.push_back(key).is_err() {
                info!("Key event queue full.");
            }
        }
    }

    /// Perform a 1khz tick operation.
    pub fn tick(&mut self) {
        match self.hid.device().tick() {
            Ok(()) => (),
            Err(_) => info!("tick error"),
        }

        // If we have keys to queue up, try to do that here.
        if let Some(key) = self.keys.front() {
            let ks = match key {
                Event::KeyPress(k) => [*k],
                Event::KeyRelease(_) => [Keyboard::NoEventIndicated],
            };
            match self.hid.device().write_report(ks) {
                Ok(()) => {
                    // Successful queue, so remove.
                    let _ = self.keys.pop_front();
                }
                Err(UsbHidError::WouldBlock) => (),
                Err(UsbHidError::Duplicate) => warn!("Duplicate key seen"),
                Err(UsbHidError::UsbError(_)) => warn!("USB error"),
                Err(UsbHidError::SerializationError) => warn!("SerializationError"),
            }
        }
    }

    /// Perform a periodic poll.  Ideally, this would be interrupt driven, but
    /// calling sufficiently fast should also work.
    /// The docs suggest this can be called on say a 1ms tick, but this seems to
    /// break device identification.
    pub fn poll(&mut self) {
        if self.dev.poll(&mut [&mut self.hid]) {
            self.hid.poll();
            match self.hid.device().read_report() {
                Ok(l) => info!("Report: {}", l.caps_lock),
                _ => (),
            }
        }

        // Check for state changes.
        let new_state = self.dev.state();
        let update = match self.state {
            None => true,
            Some(s) if s == new_state => false,
            _ => true,
        };
        if update {
            match new_state {
                UsbDeviceState::Addressed => info!("State: Addressed"),
                UsbDeviceState::Configured => info!("State: Configured"),
                UsbDeviceState::Default => info!("State: Default"),
                UsbDeviceState::Suspend => info!("State: Suspend"),
            }
            self.state = Some(new_state);
        }
    }
}
