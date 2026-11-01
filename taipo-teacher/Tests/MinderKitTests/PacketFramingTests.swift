import XCTest
@testable import MinderKit

/// The packet framing the Bluetooth link uses, against what `minder`'s `hid_encode` and
/// `HidDecoder` do: 63 bytes of message a packet, the sequence number first, the top bit on
/// the last.  The Rust side pads the last packet to 64; this does not, which the keyboard
/// and the Rust decoder both take.
final class PacketFramingTests: XCTestCase {
    private func message(_ count: Int) -> [UInt8] {
        (0..<count).map { UInt8(truncatingIfNeeded: $0 * 7 + 1) }
    }

    func testPacketBoundaries() {
        // A message of exactly 63 bytes fills one packet, and needs no second.
        XCTAssertEqual(PacketFraming.encode(message(63)).map(\.count), [64])
        XCTAssertEqual(PacketFraming.encode(message(64)).map(\.count), [64, 2])
        XCTAssertEqual(PacketFraming.encode(message(126)).map(\.count), [64, 64])
        // An empty message is still a packet, so the other end sees it end.
        XCTAssertEqual(PacketFraming.encode([]), [[0x80]])
    }

    func testSequenceNumbers() {
        let packets = PacketFraming.encode(message(200))
        XCTAssertEqual(packets.map { $0[0] }, [0x00, 0x01, 0x02, 0x83])
        XCTAssertEqual(Array(packets.map { $0.dropFirst() }.joined()), message(200))
    }

    func testRoundTrip() {
        for count in [0, 1, 62, 63, 64, 127, 4200] {
            var decoder = PacketDecoder()
            var got: [UInt8]?
            for packet in PacketFraming.encode(message(count)) {
                XCTAssertNil(got, "a message ended before its last packet")
                got = decoder.add(packet)
            }
            XCTAssertEqual(got, message(count), "\(count) bytes")
        }
    }

    func testOutOfSequenceDiscardsToNextMessage() {
        let first = PacketFraming.encode(message(150))
        var decoder = PacketDecoder()
        XCTAssertNil(decoder.add(first[0]))
        // Packet 1 lost: the rest of this message goes.
        XCTAssertNil(decoder.add(first[2]))
        // And the next message, starting at packet 0, comes through whole.
        var got: [UInt8]?
        for packet in PacketFraming.encode(message(70)) {
            got = decoder.add(packet)
        }
        XCTAssertEqual(got, message(70))
    }

    func testPacketZeroRestarts() {
        // A message cut off part way (a client killed mid-reply), then a new one.
        var decoder = PacketDecoder()
        XCTAssertNil(decoder.add(PacketFraming.encode(message(100))[0]))
        XCTAssertEqual(decoder.add(PacketFraming.encode(message(5))[0]), message(5))
    }
}
