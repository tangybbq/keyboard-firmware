//! The host commands: selecting and clearing Bluetooth profiles, and choosing where reports go.
//!
//! The layout gives them through `LayoutActions::host_command` (see
//! `bbq_keyboard::layout::host`).  Each shows on the LEDs for a moment what it did: the LED it was
//! about, or every LED red when it could not be done.  The profile work itself happens in ble.c,
//! after the command has been taken, so "taken" means the board has such a profile, not that a
//! host has connected to it.  A host connecting shows its profile's LED the same way, whether
//! it follows a select, a wake, or a link that dropped and came back.

use bbq_keyboard::layout::host::{HostCommand, Output};
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;
use zephyr::printkln;

use crate::hid;
use crate::leds::manager::Feedback;
use crate::with_leds;

#[cfg(CONFIG_BT)]
unsafe extern "C" {
    fn ble_select_profile(profile: u8) -> i32;
    fn ble_clear_profile(profile: u8) -> i32;
}

/// How long a command's feedback shows.
const FEEDBACK_MS: u64 = 1000;

/// The feedback for `feedback_task` to show.
static FEEDBACK: Signal<CriticalSectionRawMutex, Feedback> = Signal::new();

/// Carry out a host command, and have its feedback shown.
pub fn command(command: HostCommand) {
    let feedback = match command {
        HostCommand::SelectProfile(profile) => profile_feedback(profile, select_profile(profile)),
        HostCommand::ClearProfile(profile) => profile_feedback(profile, clear_profile(profile)),
        HostCommand::Output(output) => {
            if hid::set_output(output) {
                Feedback::Taken(match output {
                    Output::Auto => 0,
                    Output::Usb => 1,
                    Output::Bluetooth => 2,
                })
            } else {
                Feedback::Refused
            }
        }
    };
    printkln!("Host command {:?}: {:?}", command, feedback);
    FEEDBACK.signal(feedback);
}

/// A profile command's feedback: the profile's LED, if the board has that profile.
fn profile_feedback(profile: u8, taken: bool) -> Feedback {
    if taken {
        Feedback::Taken(usize::from(profile) - 1)
    } else {
        Feedback::Refused
    }
}

#[cfg(CONFIG_BT)]
fn select_profile(profile: u8) -> bool {
    // SAFETY: ble.c checks the profile and only queues work; it is safe from any thread.
    unsafe { ble_select_profile(profile) == 0 }
}

#[cfg(CONFIG_BT)]
fn clear_profile(profile: u8) -> bool {
    // SAFETY: as for `ble_select_profile`.
    unsafe { ble_clear_profile(profile) == 0 }
}

#[cfg(not(CONFIG_BT))]
fn select_profile(_profile: u8) -> bool {
    false
}

#[cfg(not(CONFIG_BT))]
fn clear_profile(_profile: u8) -> bool {
    false
}

/// Show which profile's host has connected, as a select of it does.  ble.c calls this once the
/// link is encrypted, which is when reports can go to it.
#[cfg(CONFIG_BT)]
#[unsafe(no_mangle)]
extern "C" fn ble_connected_callback(profile: u8) {
    // Identity 0 does not advertise, so cannot connect; red if it somehow did.
    FEEDBACK.signal(profile_feedback(profile, profile != 0));
}

/// Show each command's feedback for `FEEDBACK_MS`, or until the next command's replaces it.
#[embassy_executor::task]
pub async fn feedback_task() -> () {
    loop {
        let mut feedback = FEEDBACK.wait().await;
        loop {
            with_leds(|leds| leds.set_feedback(Some(feedback))).await;
            match select(Timer::after_millis(FEEDBACK_MS), FEEDBACK.wait()).await {
                Either::First(()) => break,
                Either::Second(next) => feedback = next,
            }
        }
        with_leds(|leds| leds.set_feedback(None)).await;
    }
}
