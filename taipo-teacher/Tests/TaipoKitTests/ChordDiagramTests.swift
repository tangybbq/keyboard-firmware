import XCTest

@testable import TaipoKit

final class ChordDiagramTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    /// The hand has all ten keys, four fingers over two rows plus two thumbs.
    func testTheHandIsComplete() throws {
        let diagram = ChordDiagram(layouts: try layouts())

        XCTAssertEqual(diagram.keys.count, 10)
        for column in [ChordDiagram.Column.index, .middle, .ring, .pinky, .thumb] {
            XCTAssertEqual(
                diagram.keys.filter { $0.column == column }.count, 2, "\(column)")
        }
        XCTAssertEqual(diagram.keys.filter(\.top).count, 5)
    }

    /// The right hand's order, taken from the mesa2's matrix: the far row is `i n s r`
    /// with the backspace thumb, the near row `e t o a` with the space thumb.
    func testTheRightHandReadsInwardsOut() throws {
        let diagram = ChordDiagram(layouts: try layouts())

        XCTAssertEqual(
            diagram.keys.filter(\.top).map(\.name), ["i", "n", "s", "r", "Bk"])
        XCTAssertEqual(
            diagram.keys.filter { !$0.top }.map(\.name), ["e", "t", "o", "a", "Sp"])
    }

    /// A chord lights exactly the keys its code names, and no others.
    func testPressedKeysAreTheChord() throws {
        let layouts = try layouts()
        let diagram = ChordDiagram(layouts: layouts)

        // Dosh's `1` is `a+e+Bk` -- the lower pinky, the lower index, and a thumb.
        let one = try XCTUnwrap(
            layouts.variants["dosh"]?.chords.first { $0.action.types == "1" })
        XCTAssertEqual(Set(diagram.pressed(one.code).map(\.name)), ["a", "e", "Bk"])
        XCTAssertEqual(diagram.spell(one.code), "a+e+Bk")

        // And the same chord code in Taipo is a different letter but the same keys, which
        // is exactly what a picture is for.
        let taipo = try XCTUnwrap(layouts.chord(one.code, variant: "taipo"))
        XCTAssertEqual(Set(diagram.pressed(taipo.code).map(\.name)), ["a", "e", "Bk"])
    }

    /// Every chord in both tables draws, and draws something.
    func testEveryChordHasAShape() throws {
        let layouts = try layouts()
        let diagram = ChordDiagram(layouts: layouts)

        for variant in ["taipo", "dosh"] {
            for chord in layouts.variants[variant]?.chords ?? [] {
                XCTAssertEqual(
                    diagram.pressed(chord.code).count, chord.keys.count,
                    "\(variant) \(String(format: "0x%03x", chord.code))")
                XCTAssertFalse(diagram.pressed(chord.code).isEmpty)
            }
        }
    }
}
