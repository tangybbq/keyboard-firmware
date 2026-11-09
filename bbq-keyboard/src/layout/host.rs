//! Commands to the keyboard about its hosts: which Bluetooth profile is
//! active, forgetting a profile's bond, and whether reports go over USB or
//! Bluetooth; and one to put it to sleep.
//!
//! They are reached through a prefix chord in the Dosh table,
//! [`Action::HostCommands`](super::taipo::Action::HostCommands) (`a+o+t+e`),
//! which arms a one-chord layer.  The next chord, on either hand, is looked up
//! in [`HOST_COMMANDS`] instead of the Dosh table.  A chord that is not in it
//! disarms the layer and is then typed as usual.  The shapes are positional:
//!
//! | chord                                 | command                       |
//! |---------------------------------------|-------------------------------|
//! | `e` / `t` / `o` / `a`                 | select profile 1 / 2 / 3 / 4  |
//! | the same, plus the `Sp` thumb         | clear that profile            |
//! | `i` / `n` / `s`                       | output auto / USB / Bluetooth |
//! | the `Bk` thumb                        | sleep now                     |
//! | both thumbs                           | cancel                        |
//!
//! The bit names are Taipo's, as everywhere; in Dosh `Sp` (`0x100`) is the
//! backspace thumb.  Clearing takes the extra key so that it cannot be hit by
//! reaching for a select.
//!
//! The layer is in every build, so that the Dosh table, and with it the
//! fingerprint, is the same for every firmware; a board with no Bluetooth
//! simply does nothing with the commands.  See `jolt-mesa.md`, "Bluetooth on
//! the mesa3w".

/// A command the host layer gives the firmware.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HostCommand {
    /// Make a profile, numbered from 1, the active one: reconnect to its host
    /// if it has a bond, or advertise for a new one if it does not.
    SelectProfile(u8),
    /// Forget a profile's bond, numbered from 1, so that a new host can pair
    /// with it.
    ClearProfile(u8),
    /// Choose where reports go.
    Output(Output),
    /// Sleep now, as the keyboard does after long enough without a key, once
    /// every key is up.  Only where it would sleep by itself: on battery, on a
    /// board that can.
    Sleep,
}

/// Where the keyboard's reports go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Output {
    /// USB when a host has configured it, and Bluetooth otherwise.
    Auto,
    /// USB only.
    Usb,
    /// Bluetooth only, even with USB configured.
    Bluetooth,
}

/// One chord of the host layer.
pub struct HostEntry {
    pub code: u16,
    /// What the chord does; `None` disarms the layer and does nothing else.
    pub command: Option<HostCommand>,
}

/// The chords of the host layer, in Taipo's chord bits.
pub static HOST_COMMANDS: &[HostEntry] = &[
    // Profiles on the bottom row, index first.
    HostEntry { code: 0x008, command: Some(HostCommand::SelectProfile(1)) },
    HostEntry { code: 0x004, command: Some(HostCommand::SelectProfile(2)) },
    HostEntry { code: 0x002, command: Some(HostCommand::SelectProfile(3)) },
    HostEntry { code: 0x001, command: Some(HostCommand::SelectProfile(4)) },
    HostEntry { code: 0x108, command: Some(HostCommand::ClearProfile(1)) },
    HostEntry { code: 0x104, command: Some(HostCommand::ClearProfile(2)) },
    HostEntry { code: 0x102, command: Some(HostCommand::ClearProfile(3)) },
    HostEntry { code: 0x101, command: Some(HostCommand::ClearProfile(4)) },

    // The output on the top row, index first.
    HostEntry { code: 0x080, command: Some(HostCommand::Output(Output::Auto)) },
    HostEntry { code: 0x040, command: Some(HostCommand::Output(Output::Usb)) },
    HostEntry { code: 0x020, command: Some(HostCommand::Output(Output::Bluetooth)) },

    // The `Bk` thumb alone (Dosh's space).
    HostEntry { code: 0x200, command: Some(HostCommand::Sleep) },

    // Both thumbs, the null chord everywhere else, backs out.
    HostEntry { code: 0x300, command: None },
];
