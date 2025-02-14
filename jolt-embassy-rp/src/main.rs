//! This example shows powerful PIO module in the RP2040 chip to communicate with WS2812 LED modules.
//! See (https://www.sparkfun.com/categories/tags/ws2812)

#![no_std]
#![no_main]
#![cfg_attr(feature = "nightly", feature(impl_trait_in_assoc_type))]

extern crate alloc;

use core::mem::MaybeUninit;
use core::sync::atomic::Ordering;

use bbq_keyboard::boardinfo::BoardInfo;
use bbq_keyboard::dict::Dict;
use bbq_keyboard::ser2::Packet;
use bbq_keyboard::{Event, EventQueue, Side, Timable};
use bbq_steno::dict::Joined;
use bbq_steno::Stroke;
use board::Board;
use cortex_m_rt::{self, entry};
use dispatch::Dispatch;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::{FLASH, PIO0, UART0};
use embassy_rp::pio::InterruptHandler;
use embassy_rp::uart::{BufferedInterruptHandler, BufferedUartRx};
use embassy_rp::{bind_interrupts, i2c, install_core0_stack_guard, interrupt};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Instant, Ticker, Timer};
// use embedded_alloc::TlsfHeap as Heap;
use embedded_alloc::LlffHeap as Heap;
use embedded_io_async::BufRead;
use minder::SerialDecoder;
use portable_atomic::AtomicUsize;
use static_cell::StaticCell;

mod board;
mod dispatch;
mod leds;
mod inter;
mod matrix;
mod translate;
mod usb;

#[cfg(not(any(feature = "defmt", feature = "log")))]
compile_error!("One of the features \"defmt\" or \"log\" must be enabled");

#[cfg_attr(feature = "defmt", path = "logging_defmt.rs")]
#[cfg_attr(feature = "log", path = "logging_log.rs")]
mod logging;

use logging::*;

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    UART0_IRQ => BufferedInterruptHandler<UART0>;
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<embassy_rp::peripherals::USB>;
    I2C1_IRQ => i2c::InterruptHandler<embassy_rp::peripherals::I2C1>;
});

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// The High Priority executor.  This runs at P1.
static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();

/// And the thread-mode executor.
static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();

pub const BUILD_ID: u64 = parse_u64_const(env!("BUILD_ID"));

const fn parse_u64_const(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let mut value: u64 = 0;
    let mut i = 0;

    while i < bytes.len() {
        let digit = bytes[i];

        if digit < b'0' || digit > b'9' {
            core::panic!("Invalid character in BUILD_ID, expecting only digits.");
        }

        value = value * 10 + ((digit - b'0') as u64);
        i += 1;
    }

    value
}

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_HIGH.on_interrupt()
}

/*
/// Input a value 0 to 255 to get a color value
/// The colours are a transition r - g - b - back to r.
fn wheel(mut wheel_pos: u8) -> RGB8 {
    wheel_pos = 255 - wheel_pos;
    if wheel_pos < 85 {
        return (255 - wheel_pos * 3, 0, wheel_pos * 3).into();
    }
    if wheel_pos < 170 {
        wheel_pos -= 85;
        return (0, wheel_pos * 3, 255 - wheel_pos * 3).into();
    }
    wheel_pos -= 170;
    (wheel_pos * 3, 255 - wheel_pos * 3, 0).into()
}
*/

#[entry]
fn main() -> ! {
    // When using SystemView, it must be initialized before starting the embassy executor.
    log_init();

    // Initialize the heap.
    {
        const HEAP_SIZE: usize = 65535;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
    }

    info!("TangyBBQ Jolt3 Keyboard Firmware");
    info!("Build: {}", BUILD_ID);

    // Setup the MPU with a stack guard.
    install_core0_stack_guard().expect("MPU already configured)");
    let p = embassy_rp::init(Default::default());

    // The get_unique only briefly uses the flash device.
    let unique = get_unique(unsafe { FLASH::steal() });
    info!("Unique ID: {}", unique);

    let info = get_board_info();
    info!("Board information: {:?}", info);

    interrupt::SWI_IRQ_0.set_priority(Priority::P2);
    let high_spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_0);

    let board = Board::new(p, high_spawner, &info, unique);
    let _ = board;

    // All IRQs default to priority P0.
    // The GPIO and DMA drivers set their priority to P3.  These priorities are reasonable.
    // We will run one executor at P1, for most of the processing, setting up a P2 worker if
    // necessary if there are any slow processing aspects.
    // The steno thread will run in the user executor.
    
    // The general event queue.
    // TODO: This should go away.
    static EVENT_QUEUE: StaticCell<Channel<CriticalSectionRawMutex, Event, 16>> = StaticCell::new();
    let event_queue = EVENT_QUEUE.init(Channel::new());

    // The channel for sending strokes to the steno task.
    static STROKE_QUEUE: StaticCell<Channel<CriticalSectionRawMutex, Stroke, 10>> = StaticCell::new();
    let stroke_queue = STROKE_QUEUE.init(Channel::new());

    // The 'typed' channel sends actions back to dispatch to be typed.
    static TYPED_QUEUE: StaticCell<Channel<CriticalSectionRawMutex, Joined, 2>> = StaticCell::new();
    let typed_queue = TYPED_QUEUE.init(Channel::new());

    let _dispatch = Dispatch::new(
        high_spawner,
        board,
        event_queue.receiver(),
        stroke_queue.sender(),
        typed_queue.receiver(),
    );

    /*
    // Start by determining the priorities already in place.
    info!("PIO0_IRQ_0: {:?}", interrupt::PIO0_IRQ_0.get_priority());
    // info!("UART0_IRQ: {:?}", interrupt::UART0_IRQ.get_priority());
    info!("USBCTRL_IRQ: {:?}", interrupt::USBCTRL_IRQ.get_priority());
    info!("I2C1_IRQ: {:?}", interrupt::I2C1_IRQ.get_priority());
    info!("DMA_IRQ_0: {:?}", interrupt::DMA_IRQ_0.get_priority());
    info!("IO_IRQ_BANK0: {:?}", interrupt::IO_IRQ_BANK0.get_priority());
    info!("TIMER_IRQ_0: {:?}", interrupt::TIMER_IRQ_0.get_priority());
    info!("DMA_IRQ_0: {:?}", interrupt::DMA_IRQ_0.get_priority());
    */

    // For now, just fire up the thread mode executor.
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        if let Some(Side::Right) = info.side {
        } else {
            unwrap!(spawner.spawn(steno_task(spawner, stroke_queue.receiver(), typed_queue.sender(), event_queue.sender())));
        }

        // It should be safe to just exit. We'll sleep if no task got spawned.
    })
}

// TODO: Big one, this is the only use of `Event` remaining.  Improve this here (and in the zephyr
// firmware) to use a callback for events and possibly actions.
#[embassy_executor::task]
async fn steno_task(
    spawner: Spawner,
    strokes: Receiver<'static, CriticalSectionRawMutex, Stroke, 10>,
    typed: Sender<'static, CriticalSectionRawMutex, Joined, 2>,
    events: Sender<'static, CriticalSectionRawMutex, Event, 16>,
) -> ! {
    unwrap!(spawner.spawn(heap_stats()));

    let mut dict = Dict::new();
    let mut eq_send = SendWrap(events);

    loop {
        let stroke = strokes.receive().await;
        for action in dict.handle_stroke(stroke, &mut eq_send, &WrapTimer) {
            typed.send(action).await;
            // info!("Steno action: {:?}", action);
        }
    }
}

struct SendWrap(Sender<'static, CriticalSectionRawMutex, Event, 16>);

impl EventQueue for SendWrap {
    fn push(&mut self, val: Event) {
        // TODO: this is only try, hence the need to have the queue large enough.
        let _ = self.0.try_send(val);
    }
}

struct WrapTimer;

impl Timable for WrapTimer {
    fn get_ticks(&self) -> u64 {
        Instant::now().as_ticks()
    }
}

/// A small task to print out heap usage.
#[embassy_executor::task]
async fn heap_stats() -> ! {
    let mut ticker = Ticker::every(Duration::from_secs(60));
    loop {
        ticker.next().await;
        info!("Heap used: {}, free: {}", HEAP.used(), HEAP.free());
    }
}

/// Retrieve the unique ID from the flash device.  This will need to coordinate with future flash
/// drivers, but for now, it is fine to just consume it.
fn get_unique(flash: FLASH) -> &'static str {
    // https://github.com/knurling-rs/defmt/pull/683 suggests a delay of 10ms to avoid interference
    // between the debug probe and can interfere with flash operations.
    // Delay.delay_ms(10);

    let unique_id = flash::get_unique(flash);

    static UNIQUE: StaticCell<heapless::String<16>> = StaticCell::new();
    let unique = UNIQUE.init(heapless::String::new());

    let mut tmp = unique_id;
    for _ in 0..16 {
        unique.push((b'a' + ((tmp & 0x0f) as u8)) as char).unwrap();
        tmp >>= 4;
    }

    unique.as_str()
}

/// Fetch the fixed location board info.
fn get_board_info() -> BoardInfo {
    extern "C" {
        static _board_info: [u8; 256];
    }

    unsafe { BoardInfo::decode_from_memory(_board_info.as_ptr()) }.expect("Board info not present")
}

/*
#[embassy_executor::task]
async fn main_task(spawner: Spawner) {
    // Get the board info, panicing if not present.
    // SAFETY: This symbol should be in flash. The decoder uses a large specific CBOR tag to ensure
    // this isn't representing something else.
    static INFO: StaticCell<BoardInfo> = StaticCell::new();
    let info = INFO.init(unsafe { BoardInfo::decode_from_memory(_board_info.as_ptr()) }
        .expect("Board into not present"));

    info!("Board info: {:?}", info);

    // Setup a LayoutManager.
    let lm = Arc::new(Mutex::<CriticalSectionRawMutex, _>::new(LayoutManager::new(false)));

    /*
    static COLS: StaticCell<[Output<'static>; 4]> = StaticCell::new();
    */
    let cols = [
        p.PIN_6.degrade(),
        p.PIN_7.degrade(),
        p.PIN_8.degrade(),
        p.PIN_9.degrade(),
    ]
        .map(|p| Output::new(p, Level::Low));
    /*
        .map(|p| Output::new(p, Level::Low));
    let cols = COLS.init(cols);
    */

    /*
    static ROWS: StaticCell<[Input<'static>; 6]> = StaticCell::new();
    */
    let rows = [
        p.PIN_0.degrade(),
        p.PIN_2.degrade(),
        p.PIN_1.degrade(),
        p.PIN_3.degrade(),
        p.PIN_5.degrade(),
        p.PIN_4.degrade(),
    ]
        .map(|p| Input::new(p, Pull::Down));
    /*
        .map(|p| Input::new(p, Pull::Down));
    let rows = ROWS.init(rows);
    */

    unwrap!(spawner.spawn(matrix_scanner(cols, rows, lm.clone(), &info.name)));
    unwrap!(spawner.spawn(layout_ticker(lm.clone())));

    // Setup the uart.
    let mut config = Config::default();
    config.baudrate = 460800;
    config.stop_bits = StopBits::STOP1;
    config.data_bits = DataBits::DataBits8;
    config.parity = Parity::ParityNone;

    static TX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 256])[..];
    static RX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 256])[..];

    let uart = BufferedUart::new(
        p.UART0,
        Irqs,
        p.PIN_12,
        p.PIN_13,
        tx_buf,
        rx_buf,
        config
    );

    let (_tx, rx) = uart.split();

    unwrap!(spawner.spawn(inter_reader(spawner, rx)));

    // Set up the inter-board code, appropriate for the side we are on.
    match info.side {
        None => panic!("TODO: Single sided not yet supported"),
        Some(Side::Left) => {
            let mut config = i2c::Config::default();
            config.frequency = 100_000;
            let device = i2c::I2c::new_async(p.I2C1, p.PIN_11, p.PIN_10, Irqs, config);
            unwrap!(spawner.spawn(inter_controller::task(device)));
        }
        Some(Side::Right) => {
            let mut config = i2c_slave::Config::default();
            config.addr = 0x42;
            let device = i2c_slave::I2cSlave::new(p.I2C1, p.PIN_11, p.PIN_10, Irqs, config);
            unwrap!(spawner.spawn(inter_device::task(device)));
        }
    }

    unwrap!(spawner.spawn(usb::setup_usb(p.USB, unique)));

    let Pio { mut common, sm0, .. } = Pio::new(p.PIO0, Irqs);

    // This is the number of leds in the string. Helpfully, the sparkfun thing plus and adafruit
    // feather boards for the 2040 both have one built in.
    const NUM_LEDS: usize = 2;
    let mut data = [RGB8::default(); NUM_LEDS];

    // Common neopixel pins:
    // Thing plus: 8
    // Adafruit Feather: 16;  Adafruit Feather+RFM95: 4
    let program = PioWs2812Program::new(&mut common);
    let mut ws2812 = PioWs2812::new(&mut common, sm0, p.DMA_CH0, p.PIN_19, &program);

    // Loop forever making RGB values and pushing them out to the WS2812.
    let mut ticker = Ticker::every(Duration::from_millis(11));
    let mut first = true;
    loop {
        for j in 0..(256 * 5) {
            let start = Instant::now();
            debug!("New Colors:");
            for i in 0..NUM_LEDS {
                data[i] = wheel((((i * 256) as u16 / NUM_LEDS as u16 + j as u16) & 255) as u8);
                data[i] /= 32;
                debug!("R: {} G: {} B: {}", data[i].r, data[i].g, data[i].b);
            }
            ws2812.write(&data).await;
            let stop = Instant::now();
            if false {
                info!("LED update: {} us", (stop - start));
            }

            ticker.next().await;

            /*
            // In addition, dim the LEDs by updating them to off.
            data = [RGB8::default(); NUM_LEDS];
            for _ in 0..DIMMING {
                ws2812.write(&data).await;
                ticker.next().await;
            }
            */
        }

        if first {
            // info!("Heap used: {} free: {}", HEAP.used(), HEAP.free());
            first = false;
        }
    }

}
*/

/*
// TODO: This belongs in Dispatch, not here.
#[embassy_executor::task]
async fn layout_ticker(lm: Holder<LayoutManager>) {
    let mut ticker = Ticker::every(Duration::from_millis(10));
    loop {
        lm.lock().await.tick(&LMAction, 10).await;

        ticker.next().await;
    }
}
*/

/*
// Placeholder until we have Dispatch ready.
struct LMAction;

impl LayoutActions for LMAction {

    async fn set_mode(&self, mode: bbq_keyboard::LayoutMode) {
        info!("set mode: {:?}", mode);
    }

    async fn set_mode_select(&self, mode: bbq_keyboard::LayoutMode) {
        info!("set mode select: {:?}", mode);
    }

    async fn send_key(&self, key: bbq_keyboard::KeyAction) {
        info!("Send key: {:?}", key);
        let _ = key;
    }

    async fn set_sub_mode(&self, submode: bbq_keyboard::MinorMode) {
        info!("Set sub mode");
        let _ = submode;
    }

    async fn send_raw_steno(&self, stroke: Stroke) {
        info!("Send raw steno");
        let _ = stroke;
    }
}
*/

// type PacketBuffer = Deque<u8, 32>;

// Inter board UART management.
#[embassy_executor::task]
async fn inter_reader(spawner: Spawner, mut rx: BufferedUartRx<'static, UART0>) {
    static COUNTER: StaticCell<AtomicUsize> = StaticCell::new();
    let counter = COUNTER.init(AtomicUsize::new(0));
    spawner.spawn(inter_stat(counter)).unwrap();
    let mut decoder = SerialDecoder::new();
    loop {
        let buf = match rx.fill_buf().await {
            Ok(buf) => buf,
            Err(err) => {
                info!("Uart error: {:?}", err);
                continue;
            }
        };
        // info!("Read {} bytes", buf.len());
        // info!("   data {:02x}", buf);
        let n = buf.len();

        // TODO: Improve minder to not be byte oriented like this.
        for ch in buf {
            if let Some(packet) = decoder.add_decode::<Packet>(*ch) {
                // For now, just use format, and we can get formatting later.
                // info!("RX: {:?}", packet);
                let _ = packet;
                counter.fetch_add(1, Ordering::AcqRel);
            }
        }

        rx.consume(n);

        // Allow enough of a delay to hold a buffer's worth.  There isn't much reason to not wait
        // for an the keyboard debounce interval to elapse.
        Timer::after(Duration::from_millis(1)).await;
    }
}

// This task prints out the RX packet rate periodically.
#[embassy_executor::task]
async fn inter_stat(counter: &'static AtomicUsize) {
    // Every n seconds, print out how many messages received.
    let mut ticker = Ticker::every(Duration::from_secs(60));
    loop {
        ticker.next().await;

        let n = counter.swap(0, Ordering::AcqRel);
        info!("RX count: {}", n);
    }
}

#[cortex_m_rt::exception(trampoline = false)]
unsafe fn HardFault() -> ! {
    cortex_m::asm::bkpt();
    loop {}
}

mod flash {
    use embassy_rp::{
        flash::{Blocking, Flash},
        peripherals::FLASH,
    };

    // This can actually be quite a bit larger.
    const FLASH_SIZE: usize = 2 * 1024 * 1024;

    // TODO: This is a blocking interface (which I think is always the case anyway).
    pub fn get_unique(flash: FLASH) -> u64 {
        let mut flash = Flash::<_, Blocking, FLASH_SIZE>::new_blocking(flash);

        /*
        let jedec = flash.blocking_jedec_id().unwrap();
        info!("jedec id: 0x{:x}", jedec);
        */

        let mut uid = [0; 8];
        flash.blocking_unique_id(&mut uid).unwrap();
        // info!("unique ID: {=[u8]:#02x}", uid);
        u64::from_le_bytes(uid)
    }
}

/*
mod usb {
    use alloc::boxed::Box;
    use defmt::{info, warn};
    use embassy_futures::join::join;
    use embassy_rp::{peripherals::USB, usb::Driver};
    use embassy_time::{Duration, Ticker};
    use embassy_usb::{class::hid::{HidReaderWriter, ReportId, RequestHandler, State}, control::OutResponse, Builder, Config, Handler};
    use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

    use crate::Irqs;

    /// Setup the USB driver.  We'll make things heap allocated just to simplify things, and because
    /// there is no particular reason to go out of our way to avoid allocation.
    #[embassy_executor::task]
    pub async fn setup_usb(usb: USB, unique: &'static str) {
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
        let mut device_handler = JoltDeviceHandler::new();

        let mut builder = Builder::new(
            driver,
            config,
            Box::leak(config_descriptor_buf),
            Box::leak(bos_descriptor_buf),
            Box::leak(msos_descriptor_buf),
            Box::leak(control_buf),
        );
        builder.handler(&mut device_handler);

        let config = embassy_usb::class::hid::Config {
            report_descriptor: KeyboardReport::desc(),
            request_handler: None,
            poll_ms: 10,
            max_packet_size: 64,
        };
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
            // TODO: channel of keystrokes to send.  For now, just press a key every 15 seconds.
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
        };

        let out_fut = async {
            reader.run(false, &mut request_handler).await;
        };

        join(usb_fut, join(in_fut, out_fut)).await;
    }

    struct JoltRequestHandler;

    impl JoltRequestHandler {
        fn new() -> JoltRequestHandler {
            JoltRequestHandler
        }
    }

    impl RequestHandler for JoltRequestHandler {
        fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
            info!("HID get_report: id:{:?}, buf: {:x}", id, buf);
            None
        }

        fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
            info!("HID set_report: id:{:?}, data: {:x}", id, data);
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
}

mod inter_device {
    use defmt::{error, info};
    use embassy_rp::{i2c_slave::{self, Error}, peripherals::I2C1};

    // TODO: Generalize, so it isn't hard-coded to I2C1.
    #[embassy_executor::task]
    pub async fn task(mut dev: i2c_slave::I2cSlave<'static, I2C1>) -> ! {
        info!("I2C device start");
        let mut buf = [0u8; 16];
        loop {
            match dev.listen(&mut buf).await {
                Ok(i2c_slave::Command::Read) => {
                    read_reply(&mut dev).await.unwrap();
                }
                Ok(i2c_slave::Command::Write(len)) => {
                    info!("Device received write: {:x}", buf[..len]);
                }
                Ok(i2c_slave::Command::WriteRead(len)) => {
                    info!("Device write read: {:x}", buf[..len]);
                    read_reply(&mut dev).await.unwrap();
                }
                Ok(i2c_slave::Command::GeneralCall(len)) => {
                    info!("Device general call: {:x}", buf[..len]);
                }
                Err(e) => error!("{}", e),
            }
        }
    }

    async fn read_reply(dev: &mut i2c_slave::I2cSlave<'static, I2C1>) -> Result<(), Error> {
        let buf = [0x12u8, 0x34, 0x56, 0x78];
        match dev.respond_and_fill(&buf, 0xff).await? {
            i2c_slave::ReadStatus::Done => (),
            i2c_slave::ReadStatus::NeedMoreBytes => unreachable!(),
            i2c_slave::ReadStatus::LeftoverBytes(x) => {
                info!("Tried to write {} extra bytes", x);
            }
        }
        Ok(())
    }
}

mod inter_controller {
    use defmt::{error, info};
    use embassy_rp::{i2c, peripherals::I2C1};
    use embassy_time::{Duration, Ticker};

    #[embassy_executor::task]
    pub async fn task(mut dev: i2c::I2c<'static, I2C1, i2c::Async>) {
        let mut ticker = Ticker::every(Duration::from_secs(10));
        let mut resp_buff = [0u8; 16];
        loop {
            ticker.next().await;

            let message = [0x01u8, 8, 9, 10, 11];
            match dev.write_read_async(0x42u16, message.iter().cloned(), &mut resp_buff).await {
                Ok(_) => {
                    info!("write_read_resp: {:x}", resp_buff);
                }
                Err(e) => error!("Error writing {}", e),
            }
        }
    }
}
*/
