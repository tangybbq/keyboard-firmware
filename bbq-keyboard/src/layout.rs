//! Handle layout related concerns, such as:
//!
//! - Keyboard layout layers and such.
//! - Steno dictionary conversion
//! - All of the interaction between these.

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

/// The layout manager.
pub struct LayoutManager {
    raw: steno::RawStenoHandler,
}

impl LayoutManager {
    pub fn new() -> Self {
        LayoutManager {
            raw: RawStenoHandler::new(),
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
        self.raw.handle_event(event, events);
    }
}
