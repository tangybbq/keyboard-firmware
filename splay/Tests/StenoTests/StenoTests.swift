import XCTest
@testable import Steno

final class StenoTests: XCTestCase {
    func testCompact() throws {
        let cases: [(UInt32, String)] = [
            (0x800000, "^"),
            (0x400000, "+"),
            (0x200000, "S"),
            (0x100000, "T"),
            (0x080000, "K"),
            (0x040000, "P"),
            (0x020000, "W"),
            (0x010000, "H"),
            (0x008000, "R"),
            (0x004000, "A"),
            (0x002000, "O"),
            (0x001000, "*"),
            (0x000800, "E"),
            (0x000400, "U"),
            (0x000200, "-F"),
            (0x000100, "-R"),
            (0x000080, "-P"),
            (0x000040, "-B"),
            (0x000020, "-L"),
            (0x000010, "-G"),
            (0x000008, "-T"),
            (0x000004, "-S"),
            (0x000002, "-D"),
            (0x000001, "-Z"),

            // Ensure that values with just numbers don't give the
            // number indicator.
            (0x1800000, "#^"),
            (0x1400000, "#+"),
            (0x1200000, "1"),
            (0x1100000, "2"),
            (0x1080000, "#K"),
            (0x1040000, "3"),
            (0x1020000, "#W"),
            (0x1010000, "4"),
            (0x1008000, "#R"),
            (0x1004000, "5"),
            (0x1002000, "0"),
            (0x1001000, "#*"),
            (0x1000800, "#E"),
            (0x1000400, "#U"),
            (0x1000200, "-6"),
            (0x1000100, "#-R"),
            (0x1000080, "-7"),
            (0x1000040, "#-B"),
            (0x1000020, "-8"),
            (0x1000010, "#-G"),
            (0x1000008, "-9"),
            (0x1000004, "#-S"),
            (0x1000002, "#-D"),
            (0x1000001, "#-Z"),

            // Any digit should suppress the number character.
            (0x1010001, "4-Z"),

            // But only non-digits should require it.
            (0x1008001, "#R-Z"),
        ]
        // Validate bits to text.
        for  (num, text) in cases {
            XCTAssertEqual(Stroke(bits: num).ToCompact(), text)
        }
        // Validate text to bits.
        for (num, text) in cases {
            let st = try Stroke(text: text)
            XCTAssertEqual(st.bits, num)
        }
    }

    // This test is really slow, so only execute it if specifically
    // requested.  This probably should also be compiled with
    // "-c release".  Even then, it still is fairly slow.  On an M3
    // Pro, it takes about a minute with release (and about 3.5
    // minutes in a debug build).
    // TODO: This could be done with multiple CPUs, as this is _very_
    // easy to divide into multiple threads.
    func testRoundTrip() throws {
        if let _ = ProcessInfo.processInfo.environment["STENO_SLOW_TESTS"] {
            for bit in UInt32(0)..<0x2000000 {
                let st = Stroke(bits: bit)
                let text = st.ToCompact()
                let st2 = try Stroke(text: text)
                XCTAssertEqual(st, st2)
            }
        }
    }
}
