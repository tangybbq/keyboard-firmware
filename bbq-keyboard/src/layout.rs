//! Handle layout related concerns, such as:
//!
//! - Keyboard layout layers and such.
//! - Steno dictionary conversion
//! - All of the interaction between these.

use crate::KeyEvent;

use self::qwerty::QwertyManager;
use self::steno::RawStenoHandler;
use self::taipo::TaipoManager;

mod artsey;
mod qwerty;
mod steno;
mod taipo;

/// The mode key is the general key to switch modes.
const MODE_KEY: u8 = 2;

// Keyboards are complicated things, and small keyboards are even more
// complicated. We support numerous different ways of seeing the keyboard, ways
// that are traditionally called "layers" in keyboard firmware. That term isn't
// quite right here, because when in these different layers, how multiple
// keypresses are interpreted changes. Typically, there are a few different ways
// of interpreting multiple keypresses:
//
// - Traditional steno. All keys that are pressed together are registered as a
//   chord, which is processed when all of the keys have been released.
// - 1st up. Similar to traditional steno, but as soon as we start seeing
//   something being lifted, we send that stroke. Once new keys start coming down,
//   we start over, possibly registering multiple strokes without all of the keys
//   being lifted. This allows things like fingerspelling where a modifier will be
//   held down with one hand, while the other spells out with individual strokes.
// - Traditional keyboard. Each key pressed registers as a "down" to the host,
//   and each released registers as a release.
// - Combined. With traditional, pairs of keys that are pressed together will
//   register as if they were a different key.
// - Tap/hold keys (or combined keys) that are pressed and released will
//   register differently than keys that are held down, either for a period of
//   time, or held down and used as modifiers.  This is currently not implemented
//   in bbq-keyboard, and I'm trying to design my layouts to not need them. I
//   find them frustrating to use.
// - Mostly chord. The Artsey layout (see the artsey module) mostly works with
//   chords, but also has some keys that can be held down to work kind of like
//   shift keys. These will generally be distinguished by small amounts of time
//   passing.
//
// This module is responsible for coordinating between all of these different
// ways of viewing the keyboard. The work of decoding each mode is handled by
// submodules. There is one special key detection here that is used to switch
// between some of the major modes. This is frequent enough that I've given it a
// dedicated key. If that key is pressed by itself, or as a chord with a small
// number of other keys, the mode will be set to the specified mode.
//
// There is a mode switch key that is handled specially by this layer. It can be
// pressed by itself, which will cycle through the modes. Or, there will be a
// key that can be pressed after it (hold and tap) to select a specific mode.
// Keys sent between the first tap of the mode key and it's release aren't sent
// to the lower layers. Press mode select while other keys are pressed will have
// noeffect.

mod async_traits {
    // This is generally warned because it makes the API fragile.  This makes the API fragile, as
    // Send is not propagated as a requirement.
    #![allow(async_fn_in_trait)]

    use bbq_steno::Stroke;

    use crate::{KeyAction, MinorMode};

    use super::LayoutMode;

    /// The actions the layout manager is able to use.
    ///
    /// These were originally events from the event queue, but are now called directly.
    /// It is intentional that these do not take a mutable self.  The handler is expected to be shared,
    /// and will be responsible for protecting the data.
    pub trait LayoutActions {
        /// Set the LayoutMode.
        ///
        /// This generally will update indicators to show the current mode.
        async fn set_mode(&self, mode: LayoutMode);

        /// Indicate a mode is being selected.
        ///
        /// Update the indicators, but in a way to indicate the mode is being selected, for example, by
        /// flashing the LED.
        async fn set_mode_select(&self, mode: LayoutMode);

        /// Send a keypress to the HID layer.
        async fn send_key(&self, key: KeyAction);

        /// A sub-mode indicator.
        async fn set_sub_mode(&self, submode: MinorMode);

        /// Send a RawSteno stroke.
        async fn send_raw_steno(&self, stroke: Stroke);
    }
}
pub use async_traits::LayoutActions;

/// The layout manager.
///
/// Some of the entrypoints take an EventQueue.  In the process of gradually separating out the
/// events, the LayoutManager only sends the following events:
/// - Mode
/// - ModeSelect
/// - KeyAction
/// - RawSteno
pub struct LayoutManager {
    raw: steno::RawStenoHandler,
    artsey: artsey::ArtseyManager,
    qwerty: qwerty::QwertyManager,
    taipo: taipo::TaipoManager,

    // Global mode.  This indicates what mode we are in.
    mode: ModeSelector,

    // Set to true for the first tick.
    first_tick: bool,

    // Flag indicating this is a two-row keyboard.  Skips qwerty mode when selected.
    two_row: bool,
}

impl LayoutManager {
    pub fn new(two_row: bool) -> Self {
        LayoutManager {
            raw: RawStenoHandler::new(),
            artsey: artsey::ArtseyManager::default(),
            mode: ModeSelector::new(two_row),
            qwerty: QwertyManager::default(),
            taipo: TaipoManager::default(),
            first_tick: true,
            two_row,
        }
    }

    // For now, just pass everything through.
    pub async fn tick<ACT: LayoutActions>(&mut self, actions: &ACT, ticks: usize) {
        self.raw.tick(ticks);
        self.artsey.tick(actions, ticks).await;
        self.qwerty.tick(actions, ticks).await;
        self.taipo.tick(actions, ticks).await;

        // Inform the upper layer what our initial mode is.
        if self.first_tick {
            actions.set_mode(self.mode.get()).await;
            self.first_tick = false;
        }
    }

    pub fn poll(&mut self) {
        self.raw.poll();
        self.artsey.poll();
        self.taipo.poll();
    }

    /// Handle a single key event.
    pub async fn handle_event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT) {
        if self.mode.event(event, actions, self.two_row).await {
            match self.mode.get() {
                LayoutMode::Artsey => {
                    self.artsey.handle_event(event, actions).await;
                }
                LayoutMode::Taipo => {
                    self.taipo.handle_event(event, actions).await;
                }
                LayoutMode::Steno | LayoutMode::StenoDirect => {
                    self.raw.handle_event(event, actions).await;
                }
                LayoutMode::Qwerty => {
                    self.qwerty.handle_event(event, actions, false).await;
                }
                LayoutMode::NKRO => {
                    self.qwerty.handle_event(event, actions, true).await;
                }
            }
        }
    }
}

/// The global keyboard mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutMode {
    StenoDirect,
    Steno,
    Artsey,
    Taipo,
    Qwerty,
    NKRO,
}

impl Default for LayoutMode {
    /// The initial mode we're starting in.
    fn default() -> Self {
        LayoutMode::Qwerty
    }
}

/// A layout mode manager handles the behavior of the special key.
struct ModeSelector {
    /// The current mode.  If this is None, then we waiting to determine the mode we're in.
    mode: LayoutMode,

    /// Selecting mode.  Once the special key is seen, we start ignoring keys.
    selecting: bool,

    /// Track keys that are currently pressed.
    pressed: u64,

    /// When we see the layout mode, pressed keys will register here.
    seen: u64,
}

impl ModeSelector {
    fn new(two_row: bool) -> Self {
        let mode = if two_row { LayoutMode::Taipo } else { LayoutMode::Qwerty };
        ModeSelector {
            mode,
            selecting: false,
            pressed: 0,
            seen: 0,
        }
    }

    /// Get the current mode.
    fn get(&self) -> LayoutMode {
        self.mode
    }

    /// Handle a keyevent, and return 'true' if the key even should be passed down to lower layers.
    async fn event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT, two_row: bool) -> bool {
        // Update the mask of keys that have been pressed.
        match event {
            KeyEvent::Press(k) => self.pressed |= 1 << k,
            KeyEvent::Release(k) => self.pressed &= !(1 << k),
        }

        // If we've pressed the mode selector, enter the funny mode.
        if let KeyEvent::Press(MODE_KEY) = event {
            // Only do something here if either we are selecting, or no other
            // keys have been pressed.
            if self.selecting || (self.pressed & !(1 << (MODE_KEY as usize))) == 0 {
                // Toggle the mode.
                self.mode = self.mode.next(two_row);
                self.selecting = true;
                actions.set_mode_select(self.mode).await;
            }
        }

        // Special case for selecting.
        if self.selecting {
            // Merge in any keys seen.
            self.seen |= self.pressed;

            // When evertything is released, pick our next mode.
            if self.pressed == 0 {
                if let Some(new_mode) = self.new_mode(two_row) {
                    self.mode = new_mode;
                }

                // TODO: Look at 'seen' to determine fixed mode changes. For
                // now, just do toggle.
                self.seen = 0;
                self.selecting = false;
                actions.set_mode(self.mode).await;
                // info!("Mode change: {:?}", self.mode);
            } else {
                // Check for a specific selection to possibly change the
                // indicator.
                if let Some(new_mode) = self.new_mode(two_row) {
                    if self.mode != new_mode {
                        actions.set_mode_select(new_mode).await;
                    }
                }
            }
            false
        } else {
            // If not selecting, just handle in layer below.
            true
        }
    }

    /// Determine if there is a mode update based on pressed keys while selecting.
    /// TODO: These are based on the 3-row keyboard.
    fn new_mode(&self, two_row: bool) -> Option<LayoutMode> {
        match self.seen & !(1 << (MODE_KEY)) {
            // qwerty 'f' or 'j' select qwerty.
            m if m == (1 << 17) || m == (1 << 41) => {
                if two_row {
                    Some(LayoutMode::Taipo)
                } else {
                    Some(LayoutMode::Qwerty)
                }
            }
            // qwerty 'd' or 'k' select StenoDirect.
            m if m == (1 << 13) || m == (1 << 37) => Some(LayoutMode::StenoDirect),
            // qwerty 's' or 'l' select steno raw.
            m if m == (1 << 9) || m == (1 << 33) => Some(LayoutMode::Steno),
            _ => None,
        }
    }
}

impl LayoutMode {
    /// Move to the next mode.
    fn next(self, two_row: bool) -> Self {
        if two_row {
            match self {
                // Direct cycling is between these modes.
                LayoutMode::Steno => LayoutMode::Taipo,
                LayoutMode::Taipo => LayoutMode::Steno,

                // These move to another mode, but can only be entered directly.
                LayoutMode::Qwerty => LayoutMode::Steno,
                LayoutMode::StenoDirect => LayoutMode::Taipo,
                LayoutMode::Artsey => LayoutMode::Qwerty,
                LayoutMode::NKRO => LayoutMode::Steno,
            }
        } else {
            match self {
                // Direct cycling is between these modes.
                LayoutMode::Steno => LayoutMode::Taipo,
                LayoutMode::StenoDirect => LayoutMode::Taipo,
                LayoutMode::Taipo => LayoutMode::Qwerty,
                LayoutMode::Qwerty => LayoutMode::Steno,

                // These move to another mode, but can only be entered directly.
                LayoutMode::Artsey => LayoutMode::Qwerty,
                LayoutMode::NKRO => LayoutMode::Steno,
            }
        }
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for LayoutMode {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            LayoutMode::Steno => defmt::write!(fmt, "steno"),
            LayoutMode::StenoDirect => defmt::write!(fmt, "StenoDirect"),
            LayoutMode::Artsey => defmt::write!(fmt, "artsey"),
            LayoutMode::Qwerty => defmt::write!(fmt, "qwerty"),
            LayoutMode::NKRO => defmt::write!(fmt, "nkro"),
            LayoutMode::Taipo => defmt::write!(fmt, "taipo"),
        }
    }
}
