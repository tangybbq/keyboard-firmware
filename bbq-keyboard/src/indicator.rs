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

use crate::{LayoutMode, Mods};

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
    /// The layout mode, or `None` until the layout has announced it.
    mode: Option<LayoutMode>,
    /// Every modifier held.
    oneshot: Mods,
    /// The subset of `oneshot` that survives a keypress.
    sticky: Mods,
}

impl Default for Indicator {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator {
    /// An indicator with nothing to show yet.
    pub const fn new() -> Self {
        Indicator {
            mode: None,
            oneshot: Mods::empty(),
            sticky: Mods::empty(),
        }
    }

    /// Show the layout mode.
    pub fn set_mode(&mut self, mode: LayoutMode) {
        self.mode = Some(mode);
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
        let image = self.mode.map_or(0, mode_image);
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

/// The slots a mode lights when no modifier is in the way, as a mask.
///
/// Taipo, whichever chord table it is on, is dark.  Orsy lights everything:
/// Orsy and Dosh are the same keys and produce the same kind of output, so
/// nothing else says which one a chord is about to be read as.
fn mode_image(mode: LayoutMode) -> u32 {
    match mode {
        #[cfg(feature = "orsy")]
        LayoutMode::Orsy => !0,
        _ => 0,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const MODS: [Mods; 4] = [Mods::CONTROL, Mods::SHIFT, Mods::ALT, Mods::GUI];

    /// Each modifier lights its own slot, and only that one.
    #[test]
    fn test_modifier_slots() {
        let mut ind = Indicator::new();
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
        let mut ind = Indicator::new();
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
        let mut ind = Indicator::new();
        assert_eq!(ind.levels(), [Level::Off; 4]);
        ind.set_mode(LayoutMode::Taipo);
        assert_eq!(ind.levels(), [Level::Off; 4]);
    }

    /// Orsy lights every slot, and modifiers are drawn over it.
    #[cfg(feature = "orsy")]
    #[test]
    fn test_modifiers_over_mode() {
        let mut ind = Indicator::new();
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
                    let mut ind = Indicator::new();
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
