//! The modifier indicator, without the hardware.
//!
//! Each LED on the board is a slot, and each slot is about one Taipo modifier,
//! in [`Mods`] bit order: slot 0 is control, then shift, alt and GUI.  A slot
//! shows its modifier held or latched, and otherwise falls back to the mode's
//! image underneath: for each mode, the slots it lights.  This is the rule
//! only; what a [`Level`] looks like -- a color on a ws2812, a duty cycle on a
//! PWM LED -- is up to each firmware, so the two can't drift apart on what is
//! shown when.
//!
//! The slot count follows the hardware and is not fixed here.  A slot past
//! the last modifier only ever shows the mode image, and a board with fewer
//! slots than modifiers shows the leading ones.
//!
//! How long the mode image shows is the [`ModeDisplay`]: always, or only for a
//! moment after the mode changes.  The second is for a battery-powered board,
//! where no state may keep an LED lit indefinitely.  The indicator keeps no
//! time; the caller schedules [`Indicator::end_flash`] for [`MODE_FLASH_MS`]
//! after [`Indicator::set_mode`] says a flash has started.

use crate::{LayoutMode, Mods};

/// How long a mode flash lasts, in milliseconds.
pub const MODE_FLASH_MS: u64 = 1000;

/// When the mode's image is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModeDisplay {
    /// Always, under the modifiers.  Only Orsy has an image, so Taipo is
    /// dark.
    Steady,
    /// Only for [`MODE_FLASH_MS`] after the mode is set, including the
    /// initial announcement, and dark the rest of the time, apart from held
    /// modifiers.  Every mode has an image, as a switch that shows nothing
    /// looks the same as one that didn't happen.
    Flash,
}

/// What one slot shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// Nothing.
    Off,
    /// The mode's image, with the slot's modifier not held.
    Mode,
    /// The slot's modifier is held, one-shot.
    Held,
    /// The slot's modifier is latched (sticky).
    Latched,
}

/// The state the indicator shows: the mode and the Taipo modifiers.
pub struct Indicator {
    display: ModeDisplay,
    /// A mode flash is showing.
    flashing: bool,
    /// The layout mode, or `None` until the layout has announced it.
    mode: Option<LayoutMode>,
    /// Every modifier held.
    oneshot: Mods,
    /// The subset of `oneshot` that survives a keypress.
    sticky: Mods,
}

impl Indicator {
    /// An indicator with nothing to show yet.
    pub const fn new(display: ModeDisplay) -> Self {
        Indicator {
            display,
            flashing: false,
            mode: None,
            oneshot: Mods::empty(),
            sticky: Mods::empty(),
        }
    }

    /// Show the layout mode.
    ///
    /// Returns true if this starts a flash, or restarts one already showing,
    /// in which case the caller should call [`end_flash`](Self::end_flash)
    /// [`MODE_FLASH_MS`] from now.  That is every call with
    /// [`ModeDisplay::Flash`], and never with [`ModeDisplay::Steady`].
    pub fn set_mode(&mut self, mode: LayoutMode) -> bool {
        self.mode = Some(mode);
        self.flashing = self.display == ModeDisplay::Flash;
        self.flashing
    }

    /// End the mode flash, if one is showing.
    pub fn end_flash(&mut self) {
        self.flashing = false;
    }

    /// Whether a mode flash is showing.
    pub fn flashing(&self) -> bool {
        self.flashing
    }

    /// The mode last set, if any.
    pub fn mode(&self) -> Option<LayoutMode> {
        self.mode
    }

    /// Show the modifier state, as reported by
    /// [`LayoutActions::set_mod_state`](crate::layout::LayoutActions::set_mod_state).
    pub fn set_mods(&mut self, oneshot: Mods, sticky: Mods) {
        self.oneshot = oneshot;
        self.sticky = sticky;
    }

    /// What one slot shows.
    ///
    /// The slot's modifier, if it is held, is drawn over the mode image.
    /// `sticky` is always a subset of `oneshot`, so a held modifier that is
    /// also sticky is latched.
    pub fn level(&self, slot: usize) -> Level {
        if let Some(modifier) = slot_modifier(slot) {
            if self.oneshot.contains(modifier) {
                return if self.sticky.contains(modifier) {
                    Level::Latched
                } else {
                    Level::Held
                };
            }
        }
        let image = match (self.mode, self.display) {
            (None, _) => 0,
            (Some(mode), ModeDisplay::Steady) => steady_image(mode),
            (Some(mode), ModeDisplay::Flash) if self.flashing => flash_image(mode),
            (Some(_), ModeDisplay::Flash) => 0,
        };
        if slot < 32 && image & (1 << slot) != 0 {
            Level::Mode
        } else {
            Level::Off
        }
    }

    /// What each of `N` slots shows.
    pub fn levels<const N: usize>(&self) -> [Level; N] {
        core::array::from_fn(|slot| self.level(slot))
    }
}

/// The modifier a slot is about, if any: the `slot`th bit of [`Mods`].
fn slot_modifier(slot: usize) -> Option<Mods> {
    let bit = 1u8.checked_shl(u32::try_from(slot).ok()?)?;
    Mods::from_bits(bit)
}

/// The slots a mode lights when no modifier is in the way, as a mask, for
/// [`ModeDisplay::Steady`].
///
/// Taipo, whichever chord table it is on, is dark.  Orsy lights everything:
/// Orsy and Dosh are the same keys and produce the same kind of output, so
/// nothing else says which one a chord is about to be read as.
fn steady_image(mode: LayoutMode) -> u32 {
    match mode {
        #[cfg(feature = "orsy")]
        LayoutMode::Orsy => !0,
        _ => 0,
    }
}

/// The slots a mode lights during a flash, for [`ModeDisplay::Flash`].
///
/// Provisional: which slots each mode should light depends on the wireless
/// board's LEDs, which aren't settled.  All this has to do for now is make
/// every mode visible and tell Orsy from Taipo: Orsy lights everything as it
/// does when steady, and every other mode lights the first slot.
fn flash_image(mode: LayoutMode) -> u32 {
    match mode {
        #[cfg(feature = "orsy")]
        LayoutMode::Orsy => !0,
        _ => 1,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MODS: [Mods; 4] = [Mods::CONTROL, Mods::SHIFT, Mods::ALT, Mods::GUI];

    /// Each modifier lights its own slot, and only that one.
    #[test]
    fn test_modifier_slots() {
        let mut ind = Indicator::new(ModeDisplay::Steady);
        ind.set_mode(LayoutMode::Taipo);
        for (slot, &modifier) in MODS.iter().enumerate() {
            ind.set_mods(modifier, Mods::empty());
            let levels: [Level; 5] = ind.levels();
            for (other, level) in levels.iter().enumerate() {
                let want = if other == slot { Level::Held } else { Level::Off };
                assert_eq!(*level, want, "{modifier:?} in slot {other}");
            }
        }
    }

    /// A sticky modifier is latched; the others held stay held.
    #[test]
    fn test_latched_over_held() {
        let mut ind = Indicator::new(ModeDisplay::Steady);
        ind.set_mode(LayoutMode::Taipo);
        ind.set_mods(Mods::SHIFT | Mods::ALT, Mods::ALT);
        assert_eq!(
            ind.levels(),
            [Level::Off, Level::Held, Level::Latched, Level::Off]
        );
    }

    /// Nothing shows before the layout announces its mode, and Taipo is dark.
    #[test]
    fn test_dark() {
        let mut ind = Indicator::new(ModeDisplay::Steady);
        assert_eq!(ind.levels(), [Level::Off; 4]);
        ind.set_mode(LayoutMode::Taipo);
        assert_eq!(ind.levels(), [Level::Off; 4]);
    }

    /// Orsy lights every slot, and modifiers are drawn over it.
    #[cfg(feature = "orsy")]
    #[test]
    fn test_modifiers_over_mode() {
        let mut ind = Indicator::new(ModeDisplay::Steady);
        ind.set_mode(LayoutMode::Orsy);
        assert_eq!(ind.levels(), [Level::Mode; 4]);
        ind.set_mods(Mods::CONTROL | Mods::GUI, Mods::GUI);
        assert_eq!(
            ind.levels(),
            [Level::Held, Level::Mode, Level::Mode, Level::Latched]
        );
        // A slot with no modifier of its own only ever shows the mode.
        assert_eq!(ind.level(4), Level::Mode);
        assert_eq!(ind.level(100), Level::Off);
    }

    /// A flash shows the mode's image, and ends when told to, leaving the
    /// board dark.
    #[test]
    fn test_flash() {
        let mut ind = Indicator::new(ModeDisplay::Flash);
        assert_eq!(ind.levels(), [Level::Off; 4]);
        assert!(ind.set_mode(LayoutMode::Taipo));
        assert!(ind.flashing());
        assert_eq!(ind.levels(), [Level::Mode, Level::Off, Level::Off, Level::Off]);
        ind.end_flash();
        assert!(!ind.flashing());
        assert_eq!(ind.levels(), [Level::Off; 4]);
        // Ending a flash that isn't showing does nothing.
        ind.end_flash();
        assert_eq!(ind.levels(), [Level::Off; 4]);
    }

    /// Modifiers are drawn over a flash, and stay once it has ended.
    #[test]
    fn test_modifiers_over_flash() {
        let mut ind = Indicator::new(ModeDisplay::Flash);
        ind.set_mode(LayoutMode::Taipo);
        ind.set_mods(Mods::CONTROL | Mods::SHIFT, Mods::SHIFT);
        assert_eq!(ind.levels(), [Level::Held, Level::Latched, Level::Off, Level::Off]);
        ind.end_flash();
        assert_eq!(ind.levels(), [Level::Held, Level::Latched, Level::Off, Level::Off]);
    }

    /// Every mode change starts a flash, including one during a flash, which
    /// shows the new mode.  Orsy flashes every slot, and is dark after.
    #[cfg(feature = "orsy")]
    #[test]
    fn test_flash_restarts() {
        let mut ind = Indicator::new(ModeDisplay::Flash);
        assert!(ind.set_mode(LayoutMode::Taipo));
        assert!(ind.set_mode(LayoutMode::Orsy));
        assert!(ind.flashing());
        assert_eq!(ind.levels(), [Level::Mode; 4]);
        ind.end_flash();
        assert_eq!(ind.levels(), [Level::Off; 4]);
        assert!(ind.set_mode(LayoutMode::Orsy));
        assert_eq!(ind.levels(), [Level::Mode; 4]);
    }

    /// A steady indicator never flashes.
    #[test]
    fn test_steady_never_flashes() {
        let mut ind = Indicator::new(ModeDisplay::Steady);
        assert!(!ind.set_mode(LayoutMode::Taipo));
        assert!(!ind.flashing());
    }

    /// The rule jolt-embassy-rp's `LedManager::render` used before this
    /// module, in terms of levels: a modifier not held shows the mode's
    /// background, which was only lit in Orsy.
    fn old_render(mode: LayoutMode, oneshot: Mods, sticky: Mods) -> [Level; 4] {
        #[cfg(feature = "orsy")]
        let base = if mode == LayoutMode::Orsy { Level::Mode } else { Level::Off };
        #[cfg(not(feature = "orsy"))]
        let base = {
            let _ = mode;
            Level::Off
        };
        core::array::from_fn(|slot| {
            let modifier = MODS[slot];
            if !oneshot.contains(modifier) {
                base
            } else if sticky.contains(modifier) {
                Level::Latched
            } else {
                Level::Held
            }
        })
    }

    /// Every modifier state, in every mode, renders as it did before.
    #[test]
    fn test_same_as_old_render() {
        let modes = [
            LayoutMode::Taipo,
            #[cfg(feature = "orsy")]
            LayoutMode::Orsy,
            #[cfg(feature = "qwerty")]
            LayoutMode::Qwerty,
            #[cfg(feature = "steno")]
            LayoutMode::Steno,
        ];
        for mode in modes {
            for oneshot in 0..16 {
                let oneshot = Mods::from_bits(oneshot).unwrap();
                for sticky in 0..16 {
                    let sticky = Mods::from_bits(sticky).unwrap();
                    if !oneshot.contains(sticky) {
                        continue;
                    }
                    let mut ind = Indicator::new(ModeDisplay::Steady);
                    ind.set_mode(mode);
                    ind.set_mods(oneshot, sticky);
                    assert_eq!(
                        ind.levels(),
                        old_render(mode, oneshot, sticky),
                        "{mode:?} {oneshot:?} {sticky:?}"
                    );
                }
            }
        }
    }
}
