import XCTest
@testable import TaipoKit

/// The table a log means when it never says which table.
///
/// A `variant` marker records a change, so a stretch with none in it is the keyboard's
/// power-on table -- a fact about the firmware, not a preference of the reader's.  Both
/// sides therefore read the one constant `layouts.json` carries, and this is what catches
/// them coming apart: a reader that assumed the wrong one folded a morning of practice
/// into the other table's measurements with nothing about the replay looking wrong.
final class DefaultVariantTests: XCTestCase {
    func testTheEngineStartsInTheTablesDeclaredDefault() throws {
        let layouts = try Layouts.bundled()
        XCTAssertNotNil(
            layouts.variants[layouts.defaultVariant],
            "default_variant names \(layouts.defaultVariant), which is not a table here")
        XCTAssertEqual(ChordEngine(layouts: layouts).variant, layouts.defaultVariant)
    }

    /// And it is the *replay* that has to honour it, not just the engine's initial value.
    func testASessionWithNoVariantMarkerReplaysInTheDefault() throws {
        let layouts = try Layouts.bundled()
        let log = """
            # session device=mesa2 boot_id=0x1 layout=\(layouts.fingerprint)
            1000 + L.Sp
            1003 + L.s
            1252 - L.Sp
            1270 - L.s
            """
        let sessions = KeyLogFile.sessions(from: log + "\n", layouts: layouts)
        let engine = ChordEngine(layouts: layouts)
        var chords: [Chord] = []
        for entry in sessions.flatMap(\.entries) {
            switch entry {
            case .marker(let m): engine.marker(m.name, value: m.value)
            case .key(let e): chords += engine.feed(key: e.key, press: e.press, timeMs: e.timeMs)
            }
        }
        chords += engine.finish()
        XCTAssertFalse(chords.isEmpty, "the fixture should produce at least one chord")
        XCTAssertTrue(chords.allSatisfy { $0.variant == layouts.defaultVariant })
    }
}
