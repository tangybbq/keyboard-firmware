import XCTest
@testable import TaipoKit

/// The Swift correction finder has to agree with the Rust one, correction for correction.
///
/// `taipo-analyze/tests/golden/*.corrections` are generated from the same synthetic logs
/// the chord engine's goldens use, and copied here verbatim.  The trainer builds its drill
/// material out of what the writer keeps getting wrong, so a second implementation of
/// "what went wrong" is exactly the drift `taipo-teacher.md` says to pin with files rather
/// than with care.
final class ConfusionTests: XCTestCase {

    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func golden(_ name: String, _ ext: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/\(name)", withExtension: ext),
            "golden/\(name).\(ext) missing")
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// Replay a golden log and render its corrections the way the Rust golden spells them.
    private func correctionLines(_ name: String) throws -> [String] {
        let layouts = try layouts()
        let sessions = KeyLogFile.sessions(from: try golden(name, "log"), layouts: layouts)
        let scanner = CorrectionScanner(layouts: layouts)

        var out = [String]()
        for session in sessions {
            let engine = ChordEngine(layouts: layouts)
            var codes = [UInt16]()
            for entry in session.entries {
                switch entry {
                case .marker(let m):
                    engine.marker(m.name, value: m.value)
                case .key(let e):
                    codes += engine.feed(key: e.key, press: e.press, timeMs: e.timeMs).map(\.code)
                }
            }
            codes += engine.finish().map(\.code)

            for c in scanner.scan(codes) {
                let code = { (v: UInt16?) in v.map { String(format: "0x%03x", $0) } ?? "-" }
                out.append(
                    "\(code(c.deleted)) \(code(c.replacement)) \(c.kind.rawValue) "
                        + "\(c.confusion?.rawValue ?? "-")")
            }
        }
        return out
    }

    private func compare(_ name: String) throws {
        let want = try golden(name, "corrections")
            .split(separator: "\n").map(String.init)
        let got = try correctionLines(name)
        XCTAssertEqual(got.count, want.count, "\(name): correction count differs")
        for (i, (g, w)) in zip(got, want).enumerated() {
            XCTAssertEqual(g, w, "\(name): correction \(i) differs")
        }
    }

    /// Misfingerings, wrong chords and dead chords, with the corrections that follow.
    func testSloppyCorrectionsMatchRust() throws {
        try compare("sloppy")
    }

    /// The two shapes real corrections are mostly made of.
    func testSlipsCorrectionsMatchRust() throws {
        try compare("slips")
    }

    /// The classification itself, on chords chosen so the answer is not in doubt.
    ///
    /// The same cases the Rust unit test uses.  The golden files would catch a divergence
    /// on the shapes the generator happens to produce; these say what the rule is.
    func testConfusionShapes() {
        // One finger on the wrong row, and the same on two fingers at once.
        XCTAssertEqual(Confusion.of(typed: 0x002, meant: 0x020), .wrongRow)
        XCTAssertEqual(Confusion.of(typed: 0x020, meant: 0x002), .wrongRow)
        XCTAssertEqual(Confusion.of(typed: 0x009, meant: 0x090), .wrongRow)
        XCTAssertEqual(Confusion.of(typed: 0x009, meant: 0x081), .wrongRow)

        // The right row, the wrong finger -- including the whole shape shifting a column
        // over, which leaves the ring finger holding a key in both chords.
        XCTAssertEqual(Confusion.of(typed: 0x020, meant: 0x040), .wrongFinger)
        XCTAssertEqual(Confusion.of(typed: 0x012, meant: 0x024), .wrongFinger)
        XCTAssertEqual(Confusion.of(typed: 0x012, meant: 0x022), .wrongFinger)

        XCTAssertEqual(Confusion.of(typed: 0x002, meant: 0x006), .keyDropped)
        XCTAssertEqual(Confusion.of(typed: 0x006, meant: 0x002), .keyAdded)

        XCTAssertEqual(Confusion.of(typed: 0x002, meant: 0x102), .wrongLayer)
        XCTAssertEqual(Confusion.of(typed: 0x102, meant: 0x202), .wrongLayer)

        XCTAssertEqual(Confusion.of(typed: 0x002, meant: 0x120), .unrelated)
        XCTAssertEqual(Confusion.of(typed: 0x002, meant: 0x044), .unrelated)
    }

    /// A log file holds several timelines, and the reader has to find all of them.
    func testSessionsSplitTheSameWayRustDoes() throws {
        let layouts = try layouts()
        let body = try golden("slips", "log")

        // Announced and unannounced breaks both split; the unannounced one carries the
        // tables across, since it is a rollover inside one session.
        let text =
            "# session device=mesa1 boot_id=0x1 layout=0xabc\n" + body
            + "# device reset\n"
            + "# session device=mesa1 boot_id=0x2 layout=unknown\n" + body
            + body  // no header at all: a step backwards
        let sessions = KeyLogFile.sessions(from: text, layouts: layouts)
        XCTAssertEqual(sessions.count, 3)
        XCTAssertEqual(sessions[0].layout, 0xabc)
        XCTAssertNil(sessions[1].layout, "`unknown` is absent, not a fingerprint")
        XCTAssertNil(sessions[2].layout, "carried across the unannounced break")
        XCTAssertEqual(sessions[0].keys.count, sessions[1].keys.count)
    }
}
