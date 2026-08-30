//! Handle layout related concerns, such as:
//!
//! - Keyboard layout layers and such.
//! - Steno dictionary conversion
//! - All of the interaction between these.

use crate::{KeyEvent, MinorMode};

#[cfg(feature = "qwerty")]
use self::qwerty::QwertyManager;
#[cfg(feature = "steno")]
use self::steno::RawStenoHandler;
use self::taipo::{TaipoManager, TaipoVariant};

#[cfg(feature = "std")]
pub mod export;
pub mod posh;
#[cfg(feature = "qwerty")]
mod qwerty;
#[cfg(feature = "steno")]
mod steno;
pub mod taipo;

/// The taipo chord window, exposed so that tests can time their chords against
/// the same value the layout uses.
pub use self::taipo::CHORD_TIME as TAIPO_CHORD_TIME;

/// Why a taipo chord was committed, reported through
/// [`LayoutActions::taipo_chord`].
pub use self::taipo::ChordEnd;

/// The mode key is the general key to switch modes.
pub const MODE_KEY: u8 = 2;

// Define the 'upper middle" keys.  This is the top key of '^' and '+', which are the traditional
// '*' keys, but we have used the lower key as these other modifiers, and the upper keys are for
// Taipo escaping.
cfg_if::cfg_if! {
    if #[cfg(feature = "proto2")] {
        const TAIPO_1: u8 = 3;
        const TAIPO_2: u8 = 18;
        #[cfg(feature = "steno")]
        const TAIPO_MASK: u64 = (1u64 << 3) | (1 << 18);
    } else if #[cfg(feature = "proto3")] {
        const TAIPO_1: u8 = 20;
        const TAIPO_2: u8 = 44;
        const TAIPO_3: u8 = 22;
        const TAIPO_4: u8 = 46;
        #[cfg(feature = "steno")]
        const TAIPO_MASK: u64 = (1u64 << 20) | (1 << 44) | (1 << 22) | (1 << 46);
    } else {
        compiler_error!("Must enable one of proto2 or proto3");
    }
}

// The row position toggle.  Taipo and steno are both 2-row layouts (two main
// rows plus thumbs).  On a 3-row board they can sit on either the top two or
// the bottom two rows, selected at runtime by tapping the otherwise dead
// top-left key.  See `RowPosition` and `lower_row_remap` below.
#[cfg(feature = "proto3")]
pub const ROW_TOGGLE_KEY: u8 = 0;

// The Taipo variant toggle.  This is the steno `#` key of the outer left
// column, which has no meaning at all in Taipo mode (`SCAN_MAP` maps it to
// nothing).  Tapping it by itself while in Taipo switches between the Taipo and
// Posh chord tables; see `posh_event` below.  On the 2-row boards this is the
// lower of the two outer left keys, the mode key being the upper one.
#[cfg(feature = "proto3")]
pub const POSH_TOGGLE_KEY: u8 = 1;

/// Which pair of rows the 2-row layouts (Taipo and steno) occupy.
///
/// This only makes sense on a 3-row board; 2-row boards always behave as
/// `Upper`.
#[cfg(feature = "proto3")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum RowPosition {
    /// The layout uses the top two rows of the keyboard, and the bottom row
    /// holds the steno `#` keys.  This is how the boards have always worked.
    #[default]
    Upper,
    /// The layout is shifted down one row, using the bottom two rows, with the
    /// old bottom row (`#`) moving up to the now-free top row.
    Lower,
}

#[cfg(feature = "proto3")]
impl RowPosition {
    fn toggle(self) -> Self {
        match self {
            RowPosition::Upper => RowPosition::Lower,
            RowPosition::Lower => RowPosition::Upper,
        }
    }
}

/// Map a physical scancode to the code the layout tables should see when the
/// layout is in the `Lower` row position.
///
/// Scancodes are column-major, `code = column * 4 + row`, with row 3 being the
/// thumb keys.  Each column's three main-row keys are rotated so that the
/// existing tables can be used unchanged:
///
/// - physical middle (`4k+1`) acts as the layout's top row (`4k+0`),
/// - physical bottom (`4k+2`) acts as the layout's second row (`4k+1`),
/// - physical top (`4k+0`) picks up the old bottom row (`4k+2`), i.e. steno `#`.
///
/// The thumbs never move.  Neither does the outer left column (codes 0, 1, 2):
/// 0 is the toggle key itself, 1 is steno `#`, and 2 is the mode key.  The
/// outer right column does rotate, as it carries real steno letters.
#[cfg(feature = "proto3")]
pub fn lower_row_remap(key: u8) -> u8 {
    if key < 3 {
        return key;
    }
    match key % 4 {
        0 => key + 2,
        1 => key - 1,
        2 => key - 1,
        // The thumb row.
        _ => key,
    }
}

/// If this key is one of the taipo keys, return it's bit, otherwise None.
pub fn taipo_map(key: u8) -> Option<u8> {
    match key {
        TAIPO_1 => Some(1),
        TAIPO_2 => Some(2),
        #[cfg(feature = "proto3")]
        TAIPO_3 => Some(4),
        #[cfg(feature = "proto3")]
        TAIPO_4 => Some(8),
        _ => None,
    }
}

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
//
// Row position:
//
// Taipo and steno are both inherently 2-row layouts, two main rows plus the
// thumbs, but the 3-row boards obviously have a row to spare. Which two rows
// they use is selected at runtime by tapping the otherwise dead top-left key
// (see `ROW_TOGGLE_KEY`), giving the two positions described by `RowPosition`:
// the top two rows (the original behavior), or the bottom two, with the steno
// `#` keys moving up to the freed top row. The two 2-row modes share the
// setting, as they interoperate, and it is lost at power off.
//
// This is implemented purely by remapping incoming scancodes (see
// `lower_row_remap`), so the tables in the submodules only ever describe the
// upper position. Qwerty and boards that physically have two rows are not
// affected.
//
// Taipo variant:
//
// The taipo engine can interpret chords with either of two tables: Taipo
// itself, or Posh, a derivative that leaves the pinkies out (see the `posh`
// module). Which one is in use is selected at runtime by tapping the steno `#`
// key of the outer left column (see `POSH_TOGGLE_KEY`), which is dead in taipo
// mode; `posh_event` handles it, and only in taipo mode, so the key keeps its
// normal meaning everywhere else.
//
// Like the row position, the toggle requires a solo tap, so the table can never
// change in the middle of a chord, and the setting is lost at power off. Unlike
// the row position, it belongs to the taipo engine rather than to this module:
// it survives mode changes, and it applies to the taipo latch in steno mode as
// well. The change is reported as `MinorMode::Posh` for an indicator.
//
// Optional layouts:
//
// Taipo is always built. The other layouts are behind cargo features, and a
// layout that isn't compiled in is gone entirely: its mode disappears from
// `LayoutMode`, from the mode cycle, and from the mode select chords, so there
// is no way to reach it at runtime. A build with none of them enabled is a
// taipo-only keyboard, and the mode key does nothing.

mod async_traits {
    // This is generally warned because it makes the API fragile.  This makes the API fragile, as
    // Send is not propagated as a requirement.
    #![allow(async_fn_in_trait)]

    #[cfg(feature = "steno")]
    use bbq_steno::Stroke;

    use crate::{KeyAction, MinorMode, Mods, Side};

    use super::taipo::ChordEnd;
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

        /// Set one or more sub-mode indicator.
        async fn set_sub_mode(&self, submode: MinorMode);

        /// Clear one of more sub-mode indicators.
        async fn clear_sub_mode(&self, submode: MinorMode);

        /// Send a RawSteno stroke.
        #[cfg(feature = "steno")]
        async fn send_raw_steno(&self, stroke: Stroke);

        /// Report a change in the Taipo modifier state.
        ///
        /// `oneshot` is every modifier currently held, and `sticky` is the subset of those that
        /// survives a keypress.  This is called only when the state changes, and is meant to drive
        /// an indicator; implementations without one can leave it alone.
        async fn set_mod_state(&self, oneshot: Mods, sticky: Mods) {
            let _ = (oneshot, sticky);
        }

        /// Report a taipo chord being committed.
        ///
        /// `side` is the hand it was built on, `code` the chord's ten-bit key
        /// mask, and `end` why it stopped accumulating.  Called for every
        /// chord the engine assembles, before the table is consulted, so a
        /// chord the current table has no entry for -- which types nothing at
        /// all, and is a pure error signal -- is reported like any other.  It
        /// is also reported in steno mode, where the keys may never be sent;
        /// the mode is known separately.
        ///
        /// This exists so that a host replay can derive what was typed from
        /// the engine itself rather than from a second implementation of it.
        /// The firmware has nothing to do here.
        async fn taipo_chord(&self, side: Side, code: u16, end: ChordEnd) {
            let _ = (side, code, end);
        }
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
    #[cfg(feature = "steno")]
    raw: steno::RawStenoHandler,
    #[cfg(feature = "qwerty")]
    qwerty: qwerty::QwertyManager,
    taipo: taipo::TaipoManager,

    // Global mode.  This indicates what mode we are in.
    mode: ModeSelector,

    // Set to true for the first tick.
    first_tick: bool,

    // Flag indicating this is a two-row keyboard.  Skips qwerty mode when selected.
    two_row: bool,

    // Which rows the 2-row layouts occupy.  Meaningless (and never changed) on
    // a two-row board.
    #[cfg(feature = "proto3")]
    row_position: RowPosition,

    // Set when the row toggle key has been pressed by itself, and is a
    // candidate for toggling the row position when it comes back up.
    #[cfg(feature = "proto3")]
    row_arm: bool,

    // The same, for the Taipo variant toggle key.
    #[cfg(feature = "proto3")]
    posh_arm: bool,
}

impl LayoutManager {
    pub fn new(two_row: bool) -> Self {
        LayoutManager {
            #[cfg(feature = "steno")]
            raw: RawStenoHandler::new(),
            mode: ModeSelector::new(two_row),
            #[cfg(feature = "qwerty")]
            qwerty: QwertyManager::default(),
            taipo: TaipoManager::default(),
            first_tick: true,
            two_row,
            #[cfg(feature = "proto3")]
            row_position: RowPosition::default(),
            #[cfg(feature = "proto3")]
            row_arm: false,
            #[cfg(feature = "proto3")]
            posh_arm: false,
        }
    }

    // For now, just pass everything through.
    pub async fn tick<ACT: LayoutActions>(&mut self, actions: &ACT, ticks: usize) {
        #[cfg(feature = "steno")]
        self.raw.tick(ticks);
        #[cfg(feature = "qwerty")]
        self.qwerty.tick(actions, ticks).await;

        self.taipo.tick(actions, ticks, self.mode.is_steno()).await;

        // Inform the upper layer what our initial mode is.
        if self.first_tick {
            actions.set_mode(self.mode.get()).await;
            self.first_tick = false;
        }
    }

    pub fn poll(&mut self) {
        #[cfg(feature = "steno")]
        self.raw.poll();
        self.taipo.poll();
    }

    /// Handle a single key event.
    pub async fn handle_event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT) {
        #[cfg(feature = "proto3")]
        let event = match self.row_event(event) {
            Some(event) => event,
            // The toggle key was consumed.
            None => return,
        };

        let next = self.mode.event(event, actions, self.two_row).await;

        // This runs after the mode selector so that `self.mode.pressed` is up
        // to date, and so that the mode key keeps priority over the toggle.
        #[cfg(feature = "proto3")]
        if self.posh_event(event, next, actions).await {
            return;
        }

        if !matches!(next, ModeNext::Discard) {
            match self.mode.get() {
                LayoutMode::Taipo => {
                    self.taipo.handle_event(event, actions).await;
                }
                #[cfg(feature = "steno")]
                LayoutMode::Steno | LayoutMode::StenoDirect => {
                    self.raw.handle_event(event, actions).await;
                    self.taipo.handle_event(event, actions).await;
                }
                #[cfg(feature = "qwerty")]
                LayoutMode::Qwerty => {
                    self.qwerty.handle_event(event, actions, false).await;
                }
                #[cfg(feature = "qwerty")]
                LayoutMode::NKRO => {
                    self.qwerty.handle_event(event, actions, true).await;
                }
            }
        }

        self.mode.after_event(actions, next).await;
    }

    /// Handle the row position toggle key, and remap events for the current row
    /// position.
    ///
    /// This runs before `ModeSelector::event` so that everything downstream —
    /// the mode selector's taipo tap detection as well as both handlers — sees
    /// a single consistent view of the keyboard.  Returns `None` if the event
    /// was consumed.
    ///
    /// Presses and releases always map the same way.  The position can only
    /// change on the release of the toggle key with nothing else held, and the
    /// remap is only skipped while the mode selector is running, which also
    /// begins and ends with nothing else held.  So no key can be pressed under
    /// one mapping and released under another.
    #[cfg(feature = "proto3")]
    fn row_event(&mut self, event: KeyEvent) -> Option<KeyEvent> {
        // The 2-row modes on a 3-row board are the only place any of this
        // applies.  Qwerty uses all three rows as it is.
        if self.two_row || !self.mode.is_two_row_layout() {
            return Some(event);
        }

        if event.key() == ROW_TOGGLE_KEY {
            // The toggle key is always consumed in these modes; it has no other
            // meaning.  Toggle only on a solo tap, which also keeps the mapping
            // from ever changing in the middle of a chord.
            match event {
                KeyEvent::Press(_) => self.row_arm = self.mode.pressed == 0,
                KeyEvent::Release(_) => {
                    if self.row_arm && self.mode.pressed == 0 {
                        self.row_position = self.row_position.toggle();
                    }
                    self.row_arm = false;
                }
            }
            // The mode selector never sees this key, so its `pressed` mask
            // can't disarm the variant toggle; do it here instead.
            self.posh_arm = false;
            return None;
        }

        // Any other key means the toggle key wasn't pressed by itself.
        self.row_arm = false;

        match self.row_position {
            RowPosition::Upper => Some(event),
            RowPosition::Lower => Some(match event {
                KeyEvent::Press(k) => KeyEvent::Press(lower_row_remap(k)),
                KeyEvent::Release(k) => KeyEvent::Release(lower_row_remap(k)),
            }),
        }
    }

    /// Handle the Taipo variant toggle key, returning true if the event was
    /// consumed.
    ///
    /// The key only means anything while Taipo is the current mode and no mode
    /// is being selected; everywhere else it keeps its normal meaning (steno
    /// `#`, or its qwerty key).  In Taipo it is dead, so consuming it costs
    /// nothing.
    ///
    /// As with the row toggle, only a solo tap counts: pressed with nothing
    /// else down, and released with nothing else down.  That keeps the table
    /// from ever changing in the middle of a chord.
    #[cfg(feature = "proto3")]
    async fn posh_event<ACT: LayoutActions>(
        &mut self,
        event: KeyEvent,
        next: ModeNext,
        actions: &ACT,
    ) -> bool {
        if !matches!(next, ModeNext::Normal) || self.mode.get() != LayoutMode::Taipo {
            self.posh_arm = false;
            return false;
        }

        if event.key() != POSH_TOGGLE_KEY {
            // Any other key means the toggle key wasn't pressed by itself.
            self.posh_arm = false;
            return false;
        }

        match event {
            KeyEvent::Press(_) => {
                self.posh_arm = self.mode.pressed == 1 << POSH_TOGGLE_KEY;
            }
            KeyEvent::Release(_) => {
                if self.posh_arm && self.mode.pressed == 0 {
                    let variant = self.taipo.toggle_variant();
                    match variant {
                        TaipoVariant::Taipo => actions.clear_sub_mode(MinorMode::Posh).await,
                        TaipoVariant::Posh => actions.set_sub_mode(MinorMode::Posh).await,
                    }
                }
                self.posh_arm = false;
            }
        }
        true
    }
}

/// The global keyboard mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayoutMode {
    #[cfg(feature = "steno")]
    StenoDirect,
    #[cfg(feature = "steno")]
    Steno,
    Taipo,
    #[cfg(feature = "qwerty")]
    Qwerty,
    #[cfg(feature = "qwerty")]
    NKRO,
}

impl Default for LayoutMode {
    /// The initial mode we're starting in.
    fn default() -> Self {
        #[cfg(feature = "qwerty")]
        return LayoutMode::Qwerty;
        #[cfg(not(feature = "qwerty"))]
        LayoutMode::Taipo
    }
}

/// Return value from ModeSelector::event.
#[derive(Copy, Clone)]
enum ModeNext {
    /// This key should be handled normally, and given to the next layers.
    Normal,
    /// This key should be discarded.
    Discard,
    /// The key should be handled normally, but after processing, the mode should be set.
    #[cfg(feature = "steno")]
    NewMode(LayoutMode),
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

    /// Set when we have only the taipo key(s) down.
    #[cfg(feature = "steno")]
    taipo: bool,
}

impl ModeSelector {
    fn new(two_row: bool) -> Self {
        #[cfg(feature = "qwerty")]
        let mode = if two_row { LayoutMode::Taipo } else { LayoutMode::Qwerty };
        #[cfg(not(feature = "qwerty"))]
        let mode = {
            let _ = two_row;
            LayoutMode::Taipo
        };
        ModeSelector {
            mode,
            selecting: false,
            pressed: 0,
            seen: 0,
            #[cfg(feature = "steno")]
            taipo: false,
        }
    }

    /// Get the current mode.
    fn get(&self) -> LayoutMode {
        self.mode
    }

    /// Handle a keyevent, and return 'true' if the key even should be passed down to lower layers.
    async fn event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT, two_row: bool) -> ModeNext {
        // Update the mask of keys that have been pressed.
        match event {
            KeyEvent::Press(k) => self.pressed |= 1 << k,
            KeyEvent::Release(k) => self.pressed &= !(1 << k),
        }

        // Detect the taipo key being pressed by itself.
        #[cfg(feature = "steno")]
        if let Some(_) = taipo_map(event.key()) {
            if event.is_press() {
                if self.pressed & !TAIPO_MASK == 0 {
                    // Only the taipo key is pressed, start the detection.
                    self.taipo = true;
                }
            } else {
                if self.taipo && self.pressed == 0 {
                    // Switch between taipo and steno only.  The challenge is that we want to change
                    // the mode, but only after processing this "up" event, otherwise the old mode
                    // doesn't yet see the up.
                    match self.mode {
                        LayoutMode::Steno => {
                            return ModeNext::NewMode(LayoutMode::Taipo);
                        }
                        LayoutMode::Taipo => {
                            return ModeNext::NewMode(LayoutMode::Steno);
                        }
                        _ => (),
                    }
                    self.taipo = false;
                }
            }
        } else {
            // Any other key thwarts taipo mode.
            self.taipo = false;
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
            ModeNext::Discard
        } else {
            // If not selecting, just handle in layer below.
            ModeNext::Normal
        }
    }

    /// When 'event' returns the ModeNext, after processing, this call can be used to handle
    /// NewMode.
    async fn after_event<ACT: LayoutActions>(&mut self, actions: &ACT, next: ModeNext) {
        let _ = (actions, &next);
        #[cfg(feature = "steno")]
        if let ModeNext::NewMode(mode) = next {
            self.mode = mode;
            actions.set_mode(mode).await;
        }
    }

    /// Determine if there is a mode update based on pressed keys while selecting.
    /// TODO: These are based on the 3-row keyboard.
    fn new_mode(&self, two_row: bool) -> Option<LayoutMode> {
        let _ = two_row;
        match self.seen & !(1 << (MODE_KEY)) {
            // qwerty 'f' or 'j' select qwerty.
            m if m == (1 << 17) || m == (1 << 41) => {
                #[cfg(feature = "qwerty")]
                if !two_row {
                    return Some(LayoutMode::Qwerty);
                }
                Some(LayoutMode::Taipo)
            }
            // qwerty 'd' or 'k' select StenoDirect.
            #[cfg(feature = "steno")]
            m if m == (1 << 13) || m == (1 << 37) => Some(LayoutMode::StenoDirect),
            // qwerty 's' or 'l' select steno raw.
            #[cfg(feature = "steno")]
            m if m == (1 << 9) || m == (1 << 33) => Some(LayoutMode::Steno),
            _ => None,
        }
    }

    /// Quick check if we are in steno mode.
    fn is_steno(&self) -> bool {
        #[cfg(feature = "steno")]
        return matches!(self.mode, LayoutMode::Steno);
        #[cfg(not(feature = "steno"))]
        false
    }

    /// Are we in one of the inherently 2-row layouts?
    ///
    /// While a mode is being selected, `mode` holds the tentative new mode, and
    /// every key is discarded anyway, so this reports false.  That keeps the
    /// mode select chords on fixed physical keys, rather than having them
    /// depend on which direction the mode is being cycled.
    #[cfg(feature = "proto3")]
    fn is_two_row_layout(&self) -> bool {
        if self.selecting {
            return false;
        }
        match self.mode {
            LayoutMode::Taipo => true,
            #[cfg(feature = "steno")]
            LayoutMode::Steno | LayoutMode::StenoDirect => true,
            #[cfg(feature = "qwerty")]
            LayoutMode::Qwerty | LayoutMode::NKRO => false,
        }
    }
}

impl LayoutMode {
    /// Move to the next mode.
    ///
    /// The cycle is taipo, then qwerty, then steno, and back around to taipo.
    /// A two-row board has no qwerty in the cycle, so taipo goes straight on to
    /// steno.  The modes that can only be entered directly (`StenoDirect` and
    /// `NKRO`) rejoin the cycle wherever their companion mode leaves it.
    fn next(self, two_row: bool) -> Self {
        match self {
            LayoutMode::Taipo => after_taipo(two_row),
            #[cfg(feature = "qwerty")]
            LayoutMode::Qwerty | LayoutMode::NKRO => after_qwerty(),
            #[cfg(feature = "steno")]
            LayoutMode::Steno | LayoutMode::StenoDirect => LayoutMode::Taipo,
        }
    }
}

/// The mode the cycle moves to after taipo.
fn after_taipo(two_row: bool) -> LayoutMode {
    let _ = two_row;
    #[cfg(feature = "qwerty")]
    if !two_row {
        return LayoutMode::Qwerty;
    }
    after_qwerty()
}

/// The mode the cycle moves to after qwerty.
fn after_qwerty() -> LayoutMode {
    #[cfg(feature = "steno")]
    return LayoutMode::Steno;
    #[cfg(not(feature = "steno"))]
    LayoutMode::Taipo
}

#[cfg(feature = "defmt")]
impl defmt::Format for LayoutMode {
    fn format(&self, fmt: defmt::Formatter) {
        match self {
            #[cfg(feature = "steno")]
            LayoutMode::Steno => defmt::write!(fmt, "steno"),
            #[cfg(feature = "steno")]
            LayoutMode::StenoDirect => defmt::write!(fmt, "StenoDirect"),
            #[cfg(feature = "qwerty")]
            LayoutMode::Qwerty => defmt::write!(fmt, "qwerty"),
            #[cfg(feature = "qwerty")]
            LayoutMode::NKRO => defmt::write!(fmt, "nkro"),
            LayoutMode::Taipo => defmt::write!(fmt, "taipo"),
        }
    }
}
