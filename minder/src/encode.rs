//! CBOR packetized encoding

mod hid;
pub use hid::{HidWrite, hid_encode};

pub(crate) mod serial;
pub use serial::{SerialWrite, serial_encode};
