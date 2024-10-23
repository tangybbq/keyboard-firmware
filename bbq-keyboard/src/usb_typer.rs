//! USB keyboard typer
//!
//! Accept strings and simulate typing them on a USB HID keyboard.

// The keytable represents the keys as u16's, with the low 8 bits corresponding
// to the Keyboard enum value, and the upper bits indicating modifiers.

use usbd_human_interface_device::page::Keyboard;

use crate::{KeyAction, Mods};

/// A shift modifier.
const SHIFT: u16 = 0x100;

/// An empty character, one we don't support sending.
const NONE: u16 = 0xffff;

/// Encode a single character as a keypress with no modification.
const fn n(ch: Keyboard) -> u16 {
    ch as u16
}

/// Encode a single keypress, indicating that it needs to be shifted.
const fn s(ch: Keyboard) -> u16 {
    SHIFT | (ch as u16)
}

static KEY_TABLE: [u16; 128] = [
    NONE,  // 0x00, Null character (often represented as NUL)
    NONE,  // 0x01, Start of Heading (often represented as SOH)
    NONE,  // 0x02, Start of Text (often represented as STX)
    NONE,  // 0x03, End of Text (often represented as ETX)
    NONE,  // 0x04, End of Transmission (often represented as EOT)
    NONE,  // 0x05, Enquiry (often represented as ENQ)
    NONE,  // 0x06, Acknowledge (often represented as ACK)
    NONE,  // 0x07, Bell (often represented as BEL)
    NONE,  // 0x08, Backspace (often represented as BS)
    NONE,  // 0x09, Horizontal Tab (often represented as HT)
    n(Keyboard::ReturnEnter),  // 0x0A, Line feed/New line (often represented as LF)
    NONE,  // 0x0B, Vertical Tab (often represented as VT)
    NONE,  // 0x0C, Form feed (often represented as FF)
    NONE,  // 0x0D, Carriage return (often represented as CR)
    NONE,  // 0x0E, Shift Out (often represented as SO)
    NONE,  // 0x0F, Shift In (often represented as SI)
    NONE,  // 0x10, Data Link Escape (often represented as DLE)
    NONE,  // 0x11, Device Control 1 (often considered as XON)
    NONE,  // 0x12, Device Control 2
    NONE,  // 0x13, Device Control 3 (often considered as XOFF)
    NONE,  // 0x14, Device Control 4
    NONE,  // 0x15, Negative Acknowledge (often represented as NAK)
    NONE,  // 0x16, Synchronous Idle (often represented as SYN)
    NONE,  // 0x17, End of Transmission Block (often represented as ETB)
    NONE,  // 0x18, Cancel (often represented as CAN)
    NONE,  // 0x19, End of Medium (often represented as EM)
    NONE,  // 0x1A, Substitute (often represented as SUB)
    NONE,  // 0x1B, Escape (often represented as ESC)
    NONE,  // 0x1C, File Separator
    NONE,  // 0x1D, Group Separator
    NONE,  // 0x1E, Record Separator
    NONE,  // 0x1F, Unit Separator
    n(Keyboard::Space), // 0x20, Space
    s(Keyboard::Keyboard1), // 0x21, !
    s(Keyboard::Apostrophe), // 0x22, "
    s(Keyboard::Keyboard3), // 0x23, #
    s(Keyboard::Keyboard4), // 0x24, $
    s(Keyboard::Keyboard5), // 0x25, %
    s(Keyboard::Keyboard7), // 0x26, &
    n(Keyboard::Apostrophe), // 0x27, '
    s(Keyboard::Keyboard9), // 0x28, (
    s(Keyboard::Keyboard0), // 0x29, )
    s(Keyboard::Keyboard8), // 0x2a, *
    s(Keyboard::Equal), // 0x2b, +
    n(Keyboard::Comma), // 0x2c, ,
    n(Keyboard::Minus), // 0x2d, -
    n(Keyboard::Dot), // 0x2e, .
    n(Keyboard::ForwardSlash), // 0x2f, /
    n(Keyboard::Keyboard0), // 0x30, 0
    n(Keyboard::Keyboard1), // 0x31, 1
    n(Keyboard::Keyboard2), // 0x32, 2
    n(Keyboard::Keyboard3), // 0x33, 3
    n(Keyboard::Keyboard4), // 0x34, 4
    n(Keyboard::Keyboard5), // 0x35, 5
    n(Keyboard::Keyboard6), // 0x36, 6
    n(Keyboard::Keyboard7), // 0x37, 7
    n(Keyboard::Keyboard8), // 0x38, 8
    n(Keyboard::Keyboard9), // 0x39, 9
    s(Keyboard::Semicolon), // 0x3A, :
    n(Keyboard::Semicolon), // 0x3B, ;
    s(Keyboard::Comma), // 0x3C, <
    n(Keyboard::Equal), // 0x3D, =
    s(Keyboard::Dot), // 0x3E, >
    s(Keyboard::ForwardSlash), // 0x3F, ?
    s(Keyboard::Keyboard2), // 0x40, @
    s(Keyboard::A), // 0x41, A
    s(Keyboard::B), // 0x42, B
    s(Keyboard::C), // 0x43, C
    s(Keyboard::D), // 0x44, D
    s(Keyboard::E), // 0x45, E
    s(Keyboard::F), // 0x46, F
    s(Keyboard::G), // 0x47, G
    s(Keyboard::H), // 0x48, H
    s(Keyboard::I), // 0x49, I
    s(Keyboard::J), // 0x4a, J
    s(Keyboard::K), // 0x4b, K
    s(Keyboard::L), // 0x4c, L
    s(Keyboard::M), // 0x4d, M
    s(Keyboard::N), // 0x4e, N
    s(Keyboard::O), // 0x4f, O
    s(Keyboard::P), // 0x50, P
    s(Keyboard::Q), // 0x51, Q
    s(Keyboard::R), // 0x52, R
    s(Keyboard::S), // 0x53, S
    s(Keyboard::T), // 0x54, T
    s(Keyboard::U), // 0x55, U
    s(Keyboard::V), // 0x56, V
    s(Keyboard::W), // 0x57, W
    s(Keyboard::X), // 0x58, X
    s(Keyboard::Y), // 0x59, Y
    s(Keyboard::Z), // 0x5a, Z
    n(Keyboard::LeftBrace), // 0x5B, [
    n(Keyboard::Backslash), // 0x5C, \
    n(Keyboard::RightBrace), // 0x5D, ]
    s(Keyboard::Keyboard6), // 0x5E, ^
    s(Keyboard::Minus), // 0x5F, _
    n(Keyboard::Grave), // 0x60, `
    n(Keyboard::A), // 0x61, a
    n(Keyboard::B), // 0x62, b
    n(Keyboard::C), // 0x63, c
    n(Keyboard::D), // 0x64, d
    n(Keyboard::E), // 0x65, e
    n(Keyboard::F), // 0x66, f
    n(Keyboard::G), // 0x67, g
    n(Keyboard::H), // 0x68, h
    n(Keyboard::I), // 0x69, i
    n(Keyboard::J), // 0x6a, j
    n(Keyboard::K), // 0x6b, k
    n(Keyboard::L), // 0x6c, l
    n(Keyboard::M), // 0x6d, m
    n(Keyboard::N), // 0x6e, n
    n(Keyboard::O), // 0x7f, o
    n(Keyboard::P), // 0x70, p
    n(Keyboard::Q), // 0x71, q
    n(Keyboard::R), // 0x72, r
    n(Keyboard::S), // 0x73, s
    n(Keyboard::T), // 0x74, t
    n(Keyboard::U), // 0x75, u
    n(Keyboard::V), // 0x76, v
    n(Keyboard::W), // 0x77, w
    n(Keyboard::X), // 0x78, x
    n(Keyboard::Y), // 0x79, y
    n(Keyboard::Z), // 0x7a, z
    s(Keyboard::LeftBrace), // 0x7B, {
    s(Keyboard::Backslash), // 0x7C, |
    s(Keyboard::RightBrace), // 0x7D, }
    s(Keyboard::Grave), // 0x7E, ~
    NONE, // 0x7F, Delete (often represented as DEL)
];

/// An ActionHandler is something that is able to take actions.
pub trait ActionHandler {
    fn enqueue_actions<I: Iterator<Item = KeyAction>>(&mut self, events: I);
}

/// Enqueue an action as keypresses.
pub fn enqueue_action<H: ActionHandler>(usb: &mut H, text: &str) {
    for ch in text.chars() {
        if ch < (128 as char) {
            let code = KEY_TABLE[ch as usize];
            if code == NONE {
                continue;
            }
            let shifted = (code & SHIFT) != 0;
            let code: Keyboard = ((code & 0xFF) as u8).into();
            let action = KeyAction::KeyPress(code, if shifted {Mods::SHIFT} else {Mods::empty()});

            usb.enqueue_actions([action.clone()].iter().cloned());
            usb.enqueue_actions([KeyAction::KeyRelease].iter().cloned());
        }
    }
}
