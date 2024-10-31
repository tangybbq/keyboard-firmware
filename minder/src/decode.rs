//! Packet decoding.

mod hid;
pub use hid::HidDecoder;

mod serial;
pub use serial::SerialDecoder;
