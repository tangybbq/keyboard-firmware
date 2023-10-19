//! Handle layout related concerns, such as:
//!
//! - Keyboard layout layers and such.
//! - Steno dictionary conversion
//! - All of the interaction between these.

use crate::log::info;

use crate::{KeyEvent, EventQueue};

use self::steno::RawStenoHandler;

mod steno;

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

/// The layout manager.
pub struct LayoutManager {
    raw: steno::RawStenoHandler,

    // Global mode.  This indicates what mode we are in.
    mode: ModeSelector,
}

impl LayoutManager {
    pub fn new() -> Self {
        LayoutManager {
            raw: RawStenoHandler::new(),
            mode: ModeSelector::default(),
        }
    }

    // For now, just pass everything through.
    pub fn tick(&mut self) {
        self.raw.tick();
    }

    pub fn poll(&mut self) {
        self.raw.poll();
    }

    /// Handle a single key event.
    pub fn handle_event(&mut self, event: KeyEvent, events: &mut EventQueue) {
        if self.mode.event(event) {
            match self.mode.get() {
                LayoutMode::Artsey => info!("TODO: Artsey"),
                LayoutMode::Steno => {
                    self.raw.handle_event(event, events);
                }
            }
        }
    }
}

/// The global keyboard mode.
#[derive(Clone, Copy)]
enum LayoutMode {
    Steno,
    Artsey,
}

impl Default for LayoutMode {
    /// The initial mode we're starting in.
    fn default() -> Self {
        LayoutMode::Steno
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

impl Default for ModeSelector {
    fn default() -> Self {
        ModeSelector {
            mode: LayoutMode::default(),
            selecting: false,
            pressed: 0,
            seen: 0,
        }
    }
}

impl ModeSelector {
    /// Get the current mode.
    fn get(&self) -> LayoutMode {
        self.mode
    }

    /// Handle a keyevent, and return 'true' if the key even should be passed down to lower layers.
    fn event(&mut self, event: KeyEvent) -> bool {
        // Update the mask of keys that have been pressed.
        match event {
            KeyEvent::Press(k) => self.pressed |= 1 << k,
            KeyEvent::Release(k) => self.pressed &= !(1 << k),
        }

        // If we've pressed the mode selector, enter the funny mode.
        if let KeyEvent::Press(13) = event {
            // Only do something here if either we are selecting, or no other
            // keys have been pressed.
            if self.selecting || (self.pressed & !(1 << 13)) == 0 {
                // Toggle the mode.
                self.mode = self.mode.next();
                self.selecting = true;
            }
        }

        // Special case for selecting.
        if self.selecting {
            // Merge in any keys seen.
            self.seen |= self.pressed;

            // When evertything is released, pick our next mode.
            if self.pressed == 0 {
                // TODO: Look at 'seen' to determine fixed mode changes. For
                // now, just do toggle.
                self.seen = 0;
                self.selecting = false;
                info!("Mode change: {:?}", self.mode);
            }
            false
        } else {
            // If not selecting, just handle in layer below.
            true
        }
    }
}

impl LayoutMode {
    /// Move to the next mode.
    fn next(self) -> Self {
        match self {
            LayoutMode::Steno => LayoutMode::Artsey,
            LayoutMode::Artsey => LayoutMode::Steno,
        }
    }
}

impl defmt::Format for LayoutMode {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            LayoutMode::Steno => defmt::write!(fmt, "steno"),
            LayoutMode::Artsey => defmt::write!(fmt, "artsey"),
        }
    }
}