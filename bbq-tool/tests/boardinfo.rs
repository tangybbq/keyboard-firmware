//! Verify that the encoded board info blocks decode the way the firmware reads them.

use bbq_keyboard::boardinfo::BoardInfo;
use bbq_keyboard::Side;

/// Encode a BoardInfo into a 256-byte block, the way it is stored in flash, and decode it back
/// through the same path the firmware uses.
fn round_trip(info: &BoardInfo) -> BoardInfo {
    let mut buf = Vec::new();
    minicbor::encode(info, &mut buf).expect("encode");
    assert!(buf.len() <= 256);
    // The rest of the flash page is erased.
    buf.resize(256, 0xff);

    unsafe { BoardInfo::decode_from_memory(buf.as_ptr()) }.expect("decode")
}

#[test]
fn sideless_board() {
    let info = round_trip(&BoardInfo {
        name: "proto4".to_string(),
        side: None,
    });
    assert_eq!(info.name, "proto4");
    assert!(info.side.is_none());
}

#[test]
fn sided_board() {
    for side in [Side::Left, Side::Right] {
        let info = round_trip(&BoardInfo {
            name: "jolt3".to_string(),
            side: Some(side),
        });
        assert_eq!(info.name, "jolt3");
        assert_eq!(info.side, Some(side));
    }
}
