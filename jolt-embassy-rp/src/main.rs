//! This example shows powerful PIO module in the RP2040 chip to communicate with WS2812 LED modules.
//! See (https://www.sparkfun.com/categories/tags/ws2812)

#![no_std]
#![no_main]

extern crate alloc;

use core::mem::MaybeUninit;
use core::sync::atomic::Ordering;

use bbq_keyboard::boardinfo::BoardInfo;
use bbq_keyboard::layout::{LayoutActions, LayoutManager};
use bbq_keyboard::ser2::Packet;
use bbq_keyboard::{KeyEvent, Side};
use bbq_steno::Stroke;
use embassy_executor::{Executor, Spawner};
use embassy_futures::select::select_array;
use embassy_rp::{bind_interrupts, i2c, i2c_slave, install_core0_stack_guard};
use embassy_rp::gpio::{Input, Level, Output, Pin, Pull};
use embassy_rp::peripherals::{PIO0, UART0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::uart::{BufferedInterruptHandler, BufferedUart, BufferedUartRx, Config, DataBits, Parity, StopBits};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Instant, Ticker, Timer};
use embedded_alloc::TlsfHeap as Heap;
use embedded_hal_1::delay::DelayNs;
use embedded_io_async::BufRead;
use minder::SerialDecoder;
use portable_atomic::AtomicUsize;
use portable_atomic_util::Arc;
use smart_leds::RGB8;
use static_cell::StaticCell;
use cortex_m_rt::{self, entry};

mod translate;

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

/// For sharing context between tasks.  For now, we'll use protect with a full-thread-safe
/// abstraction.
type Holder<T> = Arc<Mutex<CriticalSectionRawMutex, T>>;

// const DIMMING: usize = 32;

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

    // For now, just fire up the thread mode executor.
    static EXECUTOR_LOW: StaticCell<Executor> = StaticCell::new();
    let executor = EXECUTOR_LOW.init(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(main_task(spawner)));
    })
}

#[embassy_executor::task]
async fn main_task(spawner: Spawner) {
    info!("Start");

    // Setup the MPU with a stack guard.
    install_core0_stack_guard().expect("MPU already configured)");
    let p = embassy_rp::init(Default::default());

    // https://github.com/knurling-rs/defmt/pull/683 suggests a delay of 10ms to avoid interference
    // between the debug probe and can interfere with flash operations.
    Timer::after_millis(10).await;

    let unique_id = flash::get_unique(p.FLASH);

    static UNIQUE: StaticCell<heapless::String<16>> = StaticCell::new();
    let unique = UNIQUE.init(heapless::String::new());

    let mut tmp = unique_id;
    for _ in 0..16 {
        unique.push((b'a' + ((tmp & 0x0f) as u8)) as char).unwrap();
        // unique.push(b"0123456789abcdef"[(tmp & 0x0f) as usize] as char).unwrap();
        tmp >>= 4;
    }

    let unique = unique.as_str();

    info!("Unique ID: {}", unique);

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

/// By hard-coding the sizes, we can avoid dynamic operations, as well as extra pinning.
const NUM_ROWS: usize = 6;
const NUM_COLS: usize = 4;

/// Idle timeout for the matrix scanner.
///
/// Switching to idle mode does create a set of Futures that wait on the rows.  Not that much
/// overhead, so this doesn't need to be too large.  In idle mode, no scanning happens, and the
/// gpios are configured to interrupt.
const IDLE_TIME_US: usize = 500;

struct Scanner {
    cols: [Output<'static>; NUM_COLS],
    rows: [Input<'static>; NUM_ROWS],

    states: [Debouncer; NUM_ROWS * NUM_COLS],
    layout_manager: Holder<LayoutManager>,
    xlate: fn(u8) -> u8,
}

impl Scanner {
    /// Create a new Scanner, using the given cols and rows.
    fn new(
        cols: [Output<'static>; NUM_COLS],
        rows: [Input<'static>; NUM_ROWS],
        layout_manager: Holder<LayoutManager>,
        xlate: fn(u8) -> u8,
    ) -> Self {
        Self {
            cols,
            rows,
            states: Default::default(),
            layout_manager,
            xlate,
        }
    }

    /// Wait for keys.
    ///
    /// The first phase of the scanner enables all columns, and wants for any row to become high.
    /// This alleviates the need to scan when there are no keys down.
    async fn key_wait(&mut self) {
        // Assert all of the columns.
        for col in self.cols.iter_mut() {
            col.set_high();
        }

        // A short delay so we can avoid an interrupt if something is already pressed.
        Delay.delay_us(5);

        let row_wait = self.rows.each_mut().map(|r| r.wait_for_high());
        select_array(row_wait).await;

        // Desassert all of the columns, and return so we can begin scanning.
        for col in self.cols.iter_mut() {
            col.set_low();
        }
    }

    /// Scan the matrix repeatedly.
    ///
    /// Run a once per ms scan of the matrix, responding to any events.  After a period of time that
    /// everything has settled, returns, assuming the keyboard is idle.
    async fn scan(&mut self) {
        let mut ticker = Ticker::every(Duration::from_millis(1));
        let mut pressed = 0;
        let mut idle_count = 0;

        info!("Scanner: active scanning");
        loop {
            let mut states_iter = self.states.iter_mut().enumerate();

            for col in self.cols.iter_mut() {
                col.set_high();
                Delay.delay_us(5);

                for row in self.rows.iter() {
                    let (code, state) = unwrap!(states_iter.next());
                    match state.react(row.is_high()) {
                        KeyAction::Press => {
                            self.layout_manager.lock().await.handle_event(
                                KeyEvent::Press((self.xlate)(code as u8)),
                                &LMAction).await;
                            info!("Press: {}", code);
                            pressed += 1;
                            idle_count = 0;
                        }
                        KeyAction::Release => {
                            self.layout_manager.lock().await.handle_event(
                                KeyEvent::Release((self.xlate)(code as u8)),
                                &LMAction).await;
                            info!("Release: {}", code);
                            pressed -= 1;
                        }
                        _ => (),
                    }
                }

                col.set_low();
            }

            if pressed == 0 {
                idle_count += 1;
                if idle_count == IDLE_TIME_US {
                    break;
                }
            }

            ticker.next().await;
        }

        info!("Scanner: idle");

        if false {
            self.overflow_stack(1);
        }
    }

    fn overflow_stack(&self, count: usize) -> usize {
        if count == 1_000_000 {
            count
        } else {
            1 + self.overflow_stack(count + 1)
        }
    }
}

#[embassy_executor::task]
async fn matrix_scanner(
    cols: [Output<'static>; NUM_COLS],
    rows: [Input<'static>; NUM_ROWS],
    layout_manager: Holder<LayoutManager>,
    board_name: &'static str,
) {
    let xlate = translate::get_translation(board_name);

    // Put in an Rc to see if the spawn can handle !Send.
    let mut scanner = Scanner::new(cols, rows, layout_manager, xlate);

    loop {
        scanner.key_wait().await;
        scanner.scan().await;
    }
}

// TODO: This belongs in Dispatch, not here.
#[embassy_executor::task]
async fn layout_ticker(lm: Holder<LayoutManager>) {
    let mut ticker = Ticker::every(Duration::from_millis(10));
    loop {
        lm.lock().await.tick(&LMAction, 10).await;

        ticker.next().await;
    }
}

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

/// The state of an individual key.
#[derive(Clone, Copy, Eq, PartialEq)]
enum KeyState {
    /// Key is stable with the given pressed state.
    Stable(bool),
    /// We've detected the start of a transition to the dest, but need to see it stable before
    /// considering it done.
    Debounce(bool),
}

/// The action keys undergo.
#[derive(Clone, Copy)]
enum KeyAction {
    None,
    Press,
    Release,
}

struct Debouncer {
    /// State for this key.
    state: KeyState,
    /// Count how many times we've seen a given debounce state.
    counter: usize,
}

const DEBOUNCE_COUNT: usize = 20;

impl Debouncer {
    fn new() -> Debouncer {
        Debouncer {
            state: KeyState::Stable(false),
            counter: 0,
        }
    }

    fn react(&mut self, pressed: bool) -> KeyAction {
        match self.state {
            KeyState::Stable(cur) => {
                if cur != pressed {
                    self.state = KeyState::Debounce(pressed);
                    self.counter = 0;
                }
                KeyAction::None
            }
            KeyState::Debounce(target) => {
                if target != pressed {
                    // Reset the counter any time the state isn't our goal.
                    self.counter = 0;
                    KeyAction::None
                } else {
                    self.counter += 1;
                    if self.counter == DEBOUNCE_COUNT {
                        self.state = KeyState::Stable(target);
                        if target {
                            KeyAction::Press
                        } else {
                            KeyAction::Release
                        }
                    } else {
                        KeyAction::None
                    }
                }
            }
        }
    }
}

impl Default for Debouncer {
    fn default() -> Self {
        Self::new()
    }
}

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

extern "C" {
    static _board_info: [u8; 256];
}

#[cortex_m_rt::exception(trampoline = false)]
unsafe fn HardFault() -> ! {
    cortex_m::asm::bkpt();
    loop { }
}

mod flash {
    use embassy_rp::{flash::{Blocking, Flash}, peripherals::FLASH};

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

mod usb {
    use alloc::boxed::Box;
    use defmt::info;
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

        let mut usb = builder.build();

        let usb_fut = usb.run();

        let (reader, mut writer) = hid.split();

        let in_fut = async {
            // TODO: channel of keystrokes to send.  For now, just press a key every 15 seconds.
            let mut ticker = Ticker::every(Duration::from_secs(15));
            loop {
                ticker.next().await;

                /*
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
                */
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
