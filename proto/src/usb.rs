// Usb HID management.

use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;
use bbq_keyboard::usb_typer::ActionHandler;
use bbq_keyboard::{Event, KeyAction, Mods};
use defmt::{info, warn};
use frunk::{HCons, HNil};
use rtic_sync::channel::Sender;
use usb_device::{
    class_prelude::{UsbBus, UsbBusAllocator, UsbClass},
    prelude::{UsbDevice, UsbDeviceBuilder, UsbDeviceState, UsbVidPid},
};
use usbd_human_interface_device::{
    device::{
        keyboard::{NKROBootKeyboard, NKROBootKeyboardConfig},
        DeviceClass,
    },
    page::Keyboard,
    usb_class::{UsbHidClass, UsbHidClassBuilder},
    UsbHidError,
};
use usbd_serial::SerialPort;

// Type of the device list, which is internal to usbd_human_interface_device.
type InterfaceList<'a, Bus> = HCons<NKROBootKeyboard<'a, Bus>, HNil>;

// Enqueued bytes for the gemini protocol.
type SerialBytes = ArrayDeque<u8, 24>;

pub struct UsbHandler<'a, Bus: UsbBus> {
    dev: UsbDevice<'a, Bus>,
    hid: UsbHidClass<'a, Bus, InterfaceList<'a, Bus>>,
    serial: SerialPort<'a, Bus>,
    serial_to_send: SerialBytes,
    state: Option<UsbDeviceState>,
    keys: ArrayDeque<KeyAction, 128>,
    stall: usize,
}

impl<'a, Bus: UsbBus> UsbHandler<'a, Bus> {
    pub fn new(usb_bus: &'a UsbBusAllocator<Bus>) -> Self {
        let keyboard = UsbHidClassBuilder::new()
            .add_device(NKROBootKeyboardConfig::default())
            .build(usb_bus);
        let serial = SerialPort::new(usb_bus);
        let usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x003))
            .manufacturer("https://github.com/tangybbq/")
            .product("Proto 2")
            .serial_number("development")
            .device_class(0)
            .max_power(500)
            .supports_remote_wakeup(true)
            .max_packet_size_0(64)
            .composite_with_iads()
            .build();
        UsbHandler {
            hid: keyboard,
            serial: serial,
            serial_to_send: ArrayDeque::new(),
            dev: usb_dev,
            state: None,
            keys: ArrayDeque::new(),
            stall: 0,
        }
    }

    /// Add a sequence of events to be shipped off to the USB host.  If the
    /// deque is full, log a message, but discard.
    pub(crate) fn enqueue<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        for key in events {
            if self.keys.push_back(key).is_err() {
                info!("Key event queue full.");
            }
        }
    }

    /// Add bytes to be sent over the Gemini protocol.
    pub(crate) fn enqueue_serial(&mut self, bytes: &[u8]) {
        if self.serial_to_send.capacity() - self.serial_to_send.len() < bytes.len() {
            warn!("USB Serial bytes dropped");
            return;
        }
        for b in bytes {
            self.serial_to_send.push_back(*b).unwrap();
        }
    }

    /// Perform a 1khz tick operation.
    pub fn tick(&mut self) {
        match self.hid.device().tick() {
            Ok(()) => (),
            Err(_) => {
                // info!("tick error");
            }
        }

        self.key_tick();
        self.serial_tick();
    }

    fn key_tick(&mut self) {
        // If we are stalling, decrement, and return, waiting for the next tick.
        if self.stall > 0 {
            self.stall -= 1;
            return;
        }

        // If we have keys to queue up, try to do that here.
        if let Some(key) = self.keys.front() {
            let mut keys = ArrayVec::<_, 5>::new();

            // Capture all of the keys that should be down for this press.
            let iter = match key {
                KeyAction::KeyPress(k, m) => {
                    if m.contains(Mods::SHIFT) {
                        keys.push(Keyboard::LeftShift);
                    }
                    if m.contains(Mods::CONTROL) {
                        keys.push(Keyboard::LeftControl);
                    }
                    if m.contains(Mods::ALT) {
                        keys.push(Keyboard::LeftAlt);
                    }
                    if m.contains(Mods::GUI) {
                        keys.push(Keyboard::LeftGUI);
                    }
                    keys.push(*k);
                    None
                }
                KeyAction::ModOnly(m) => {
                    // TODO: This doesn't need to be redundant like this.
                    if m.contains(Mods::SHIFT) {
                        keys.push(Keyboard::LeftShift);
                    }
                    if m.contains(Mods::CONTROL) {
                        keys.push(Keyboard::LeftControl);
                    }
                    if m.contains(Mods::ALT) {
                        keys.push(Keyboard::LeftAlt);
                    }
                    if m.contains(Mods::GUI) {
                        keys.push(Keyboard::LeftGUI);
                    }
                    keys.push(Keyboard::NoEventIndicated);
                    None
                }
                KeyAction::KeyRelease => {
                    // Unclear if this is needed, or just empty is fine.
                    keys.push(Keyboard::NoEventIndicated);
                    None
                }
                KeyAction::KeySet(keys) => Some(keys.iter().cloned()),
                // When a stall is requested, return immediately, and wait for
                // the next tick.
                KeyAction::Stall => {
                    let _ = self.keys.pop_front();
                    self.stall = 50;
                    return;
                }
            };

            let status = match iter {
                None => self.hid.device().write_report(keys.iter().cloned()),
                Some(iter) => self.hid.device().write_report(iter),
            };
            match status {
                Ok(()) => {
                    // Successful queue, so remove.
                    let _ = self.keys.pop_front();
                }
                Err(UsbHidError::WouldBlock) => (),
                Err(UsbHidError::Duplicate) => {
                    warn!("Duplicate key seen");
                    // Duplicate keys should also unqueue.  This shouldn't
                    // happen, but don't get stuck in a queue loop if it does.
                    let _ = self.keys.pop_front();
                }
                Err(UsbHidError::UsbError(_)) => warn!("USB error"),
                Err(UsbHidError::SerializationError) => warn!("SerializationError"),
            }
        }
    }

    fn serial_tick(&mut self) {
        // Try queueing up any bytes for serial.
        while let Some(byte) = self.serial_to_send.pop_front() {
            let buf = [byte];
            match self.serial.write(&buf) {
                Ok(_) => (),
                Err(usb_device::UsbError::WouldBlock) => {
                    let _ = self.serial_to_send.push_front(byte);
                    break;
                }
                Err(_e) => warn!("Error writing to serial device"),
            }
        }
    }

    /// Perform a periodic poll.  Ideally, this would be interrupt driven, but
    /// calling sufficiently fast should also work.
    /// The docs suggest this can be called on say a 1ms tick, but this seems to
    /// break device identification.
    pub(crate) fn poll(
        &mut self,
        events: &mut Sender<'static, Event, { crate::app::EVENT_CAPACITY }>,
    ) {
        if self.dev.poll(&mut [&mut self.hid, &mut self.serial]) {
            self.hid.poll();
            self.serial.poll();
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
            if events.try_send(Event::UsbState(new_state)).is_err() {
                warn!("USB IRQ: Event queue full");
            }
        }
    }
}

/// The remote wakeup is only available for this specific hal.
impl<'a> UsbHandler<'a, sparkfun_pro_micro_rp2040::hal::usb::UsbBus> {
    /// Inform the host that we'd like to request they wake up.  This should be
    /// called only from suspend state.
    pub fn wakeup(&mut self) {
        if self.dev.remote_wakeup_enabled() {
            self.dev.bus().remote_wakeup();
        }
    }
}

impl<'a, Bus: UsbBus> ActionHandler for UsbHandler<'a, Bus> {
    fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I) {
        self.enqueue(events)
    }
}
