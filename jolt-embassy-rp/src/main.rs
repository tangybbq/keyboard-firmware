//! Jolt Keyboard Firmware - Embassy/rp2040 version
//!
//! Most of the logic behind the keyboard firmware lives in a few other crates, include
//! `bbq-keyboard` for the main keyboard code, and `bbq-steno` for the code related to Steno.
//!
//! This crate contains a main program, for various rp2040 keyboards, along with 'dispatch', which
//! is specifically written for embassy-sync.  It is intended to be more general, and eventually
//! made into its own crate, so that the HW-specific code is just that: hardware specific.

#![no_std]
#![no_main]
#![cfg_attr(feature = "nightly", feature(impl_trait_in_assoc_type))]

extern crate alloc;

use core::mem::MaybeUninit;

use bbq_keyboard::boardinfo::BoardInfo;
use bbq_keyboard::dict::Dict;
use bbq_keyboard::{Event, EventQueue, Side, Timable};
use bbq_steno::dict::Joined;
use bbq_steno::Stroke;
use board::Board;
use cortex_m_rt::{self, entry};
use dispatch::Dispatch;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::{FLASH, PIO0};
use embassy_rp::pio::InterruptHandler;
use embassy_rp::uart::BufferedInterruptHandler;
use embassy_rp::{bind_interrupts, i2c, install_core0_stack_guard, interrupt};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use embassy_time::{Duration, Instant, Ticker};
// use embedded_alloc::TlsfHeap as Heap;
use embedded_alloc::LlffHeap as Heap;
use static_cell::StaticCell;

mod board;
mod dispatch;
mod leds;
mod inter;
mod inter_uart;
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
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<embassy_rp::peripherals::USB>;
    I2C1_IRQ => i2c::InterruptHandler<embassy_rp::peripherals::I2C1>;
    UART0_IRQ => BufferedInterruptHandler<embassy_rp::peripherals::UART0>;
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

    // The steno lookup runs in the lowest priority executor.  On the rp2040, typical steno
    // dictionary lookup take around 1ms, depending on what else is happening.
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
