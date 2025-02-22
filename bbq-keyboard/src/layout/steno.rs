//! Steno key handling.

use crate::KeyEvent;

pub use bbq_steno::Stroke;
use bbq_steno_macros::stroke;

use super::{taipo_map, LayoutActions};

// Normal steno mode operates in what is known as "last up", where when all keys
// have finally been released, we send a stroke containing all of the keys that
// were pressed since the first press.
//
// "First up" works differently. As soon as a key is released, we send the
// stroke of everything that was pressed. If an additional key is pressed, we
// start recording new keys for possible additional strokes. This relies on good
// debouncing to avoid seeing sprious interleaved events.

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RawStenoHandler {
    // Keys that are still pressed.
    down: Stroke,

    // Have we enabled first-up mode.
    // first_up: bool,

    /// Bits for the taipo keys that are pressed currently.
    taipo_down: u8,

    /// Latched flag indicating the next event send is actual taipo, and shouldn't be sent.
    is_taipo: bool,

    // Toggle between pressing, and releasing.
    pressing: bool,
}

// The steno handler goes through these states. In Up indicates nothing is
// pressed. Down indicates we are pressing keys. Releasing indicates keys are
// coming up. Additional releases will be removed from the 'pressed' mask. If
// down events come in while in Releasing, we will use the current pressed keys
// as the 'seen' and re-enter Down mode. Once everything is released, we will
// return to Up state.

impl RawStenoHandler {
    pub fn new() -> Self {
        RawStenoHandler {
            down: Stroke::empty(),
            pressing: true,
            taipo_down: 0,
            is_taipo: false,
        }
    }

    // For now, we don't do anything with the tick, but it will be needed when
    // trying to implement the hold modes.
    pub fn tick(&mut self, _ticks: usize) {}
    pub fn poll(&mut self) {}

    // Handle a single event.
    pub async fn handle_event<ACT: LayoutActions>(&mut self, event: KeyEvent, actions: &ACT) {
        let key = event.key();
        if key as usize >= STENO_KEYS.len() {
            return;
        }
        // info!("Pre key {:?}: {:?}", event, self);
        if let Some(bit) = taipo_map(key) {
            if event.is_press() {
                self.taipo_down |= bit;
                self.is_taipo = true;
            }
            if event.is_release() {
                self.taipo_down &= !bit;
                if !self.pressing && self.down == Stroke::empty() {
                    self.is_taipo = false;
                }
            }
        } else if let Some(st) = STENO_KEYS[key as usize] {
            match (event.is_press(), self.pressing) {
                // We are expecting keys to be pressed.  Add to those seen.
                (true, true) => {
                    self.down |= st;
                }
                // Expecting press, and got a release. This is our first
                // release, so send what is seen.
                (false, true) => {
                    if !self.is_taipo {
                        actions.send_raw_steno(self.down).await;
                    }
                    self.is_taipo = self.taipo_down != 0;
                    self.down &= !st;
                    self.pressing = false;
                }
                // Expecting releases, if we see a down here, switch back to pressing mode.
                (true, false) => {
                    self.down |= st;
                    self.pressing = true;
                }
                // Expecting release, and got one, just use the release.
                (false, false) => {
                    self.down &= !st;
                }
            }
        }
        // info!("Post key: {:?}", self);
    }
}

#[cfg(feature = "proto2")]
static STENO_KEYS: &[Option<Stroke>] = &[
    // Left side
    Some(stroke!("O")),
    Some(stroke!("A")),
    Some(stroke!("#")),
    Some(stroke!("^")),
    Some(stroke!("^")),
    Some(stroke!("R")),
    Some(stroke!("H")),
    Some(stroke!("W")),
    Some(stroke!("P")),
    Some(stroke!("T")),
    Some(stroke!("K")),
    Some(stroke!("*")),
    Some(stroke!("S")),
    Some(Stroke::empty()),
    Some(stroke!("#")),

    // Right side
    Some(stroke!("E")),
    Some(stroke!("U")),
    Some(stroke!("#")),
    Some(stroke!("+")),
    Some(stroke!("+")),
    Some(stroke!("-R")),
    Some(stroke!("-F")),
    Some(stroke!("-B")),
    Some(stroke!("-P")),
    Some(stroke!("-L")),
    Some(stroke!("-G")),
    Some(stroke!("-T")),
    Some(stroke!("-S")),
    Some(stroke!("-D")),
    Some(stroke!("-Z")),
];

#[cfg(feature = "proto3")]
static STENO_KEYS: &[Option<Stroke>] = &[
    // Left
    // 0
    Some(Stroke::empty()),
    Some(stroke!("#")),
    Some(Stroke::empty()),
    Some(Stroke::empty()),

    // 4
    Some(stroke!("*")),
    Some(stroke!("S")),
    Some(stroke!("#")),
    Some(Stroke::empty()),

    // 8
    Some(stroke!("T")),
    Some(stroke!("K")),
    Some(stroke!("#")),
    Some(Stroke::empty()),

    // 12
    Some(stroke!("P")),
    Some(stroke!("W")),
    Some(stroke!("#")),
    Some(stroke!("#")),

    // 16
    Some(stroke!("H")),
    Some(stroke!("R")),
    Some(stroke!("#")),
    Some(stroke!("A")),

    // 20
    None,
    Some(stroke!("^")),
    Some(stroke!("^")),
    Some(stroke!("O")),

    // Right
    // 24
    Some(stroke!("-D")),
    Some(stroke!("-Z")),
    Some(stroke!("#")), // What should this be?
    Some(Stroke::empty()),

    // 28
    Some(stroke!("-T")),
    Some(stroke!("-S")),
    Some(stroke!("#")),
    Some(Stroke::empty()),

    // 32
    Some(stroke!("-L")),
    Some(stroke!("-G")),
    Some(stroke!("#")),
    Some(Stroke::empty()),

    // 36
    Some(stroke!("-P")),
    Some(stroke!("-B")),
    Some(stroke!("#")),
    Some(stroke!("#")),

    // 40
    Some(stroke!("-F")),
    Some(stroke!("-R")),
    Some(stroke!("#")),
    Some(stroke!("U")),

    // 44
    None,
    Some(stroke!("+")),
    Some(stroke!("+")),
    Some(stroke!("E")),

];
