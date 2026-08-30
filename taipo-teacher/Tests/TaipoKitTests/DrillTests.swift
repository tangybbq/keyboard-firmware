import XCTest
@testable import TaipoKit

final class DrillTests: XCTestCase {

    private func layouts() throws -> Layouts { try Layouts.bundled() }

    /// The segmentation uses the whole table, so a gram with a chord costs one step.
    func testSegmentationUsesGrams() throws {
        let target = DrillTarget(text: "the", layouts: try layouts())
        XCTAssertEqual(target.units.count, 1, "\"the\" has a chord")
        XCTAssertEqual(target.units.first?.text, "the")
    }

    /// Greedy longest-match would be wrong in general, so the segmentation is a DP.  This
    /// checks the cheapest answer rather than the first one found.
    func testSegmentationIsCheapest() throws {
        let l = try layouts()
        for text in ["the quick brown fox", "another", "information", "reformation"] {
            let target = DrillTarget(text: text, layouts: l)
            let covered = target.units.map(\.text).joined()
            XCTAssertEqual(covered, text, "segmentation must cover the whole target")
            XCTAssertLessThanOrEqual(
                target.units.count, text.count,
                "\(text) should never cost more than one chord per letter")
        }
    }

    /// A chord that types a gram counts as one correct step.
    func testChordedGramIsNotFlaggedAsSpelled() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "the", layouts: l), layouts: l)
        let unit = try XCTUnwrap(session.target.units.first)
        session.feed(chord(code: unit.code, side: .left, at: 0))
        XCTAssertEqual(session.typed, "the")
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.spelled, 0)
        XCTAssertEqual(session.stats.correct, 1)
    }

    /// The same text typed letter by letter is flagged, which is the check nothing outside
    /// the keyboard can make.
    func testSpelledGramIsFlagged() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "the", layouts: l), layouts: l)
        var t: UInt32 = 0
        for ch in ["t", "h", "e"] {
            let code = try XCTUnwrap(codeTyping(ch, l))
            session.feed(chord(code: code, side: t.isMultiple(of: 400) ? .left : .right, at: t))
            t += 200
        }
        XCTAssertEqual(session.typed, "the")
        XCTAssertTrue(session.finished, "the text is right")
        XCTAssertEqual(session.stats.spelled, 1, "but it cost three chords instead of one")
    }

    /// A backspace is an error even when the final text is correct.
    func testCorrectionCountsEvenWhenTheTextEndsRight() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        session.feed(chord(code: try XCTUnwrap(codeTyping("a", l)), side: .left, at: 0))
        session.feed(chord(code: try XCTUnwrap(codeTyping("o", l)), side: .right, at: 200))
        session.feed(chord(code: try XCTUnwrap(backspace(l)), side: .left, at: 400))
        session.feed(chord(code: try XCTUnwrap(codeTyping("t", l)), side: .right, at: 600))
        XCTAssertEqual(session.typed, "at", "the text is right in the end")
        XCTAssertEqual(session.stats.corrections, 1)
        XCTAssertEqual(session.stats.wrong, 1)
        XCTAssertLessThan(session.stats.accuracy, 1.0)
    }

    /// Alternation is an error in a drill, but only inside the window: after a pause either
    /// hand is equally correct, and a trainer must never penalise stopping to think.
    func testSameHandOnlyCountsInsideTheWindow() throws {
        let l = try layouts()
        let a = try XCTUnwrap(codeTyping("a", l))

        let quick = DrillSession(target: DrillTarget(text: "aa", layouts: l), layouts: l)
        quick.feed(chord(code: a, side: .left, at: 0))
        quick.feed(chord(code: a, side: .left, at: 200))
        XCTAssertEqual(quick.stats.sameHand, 1)
        XCTAssertEqual(quick.stats.eligiblePairs, 1)

        let paused = DrillSession(target: DrillTarget(text: "aa", layouts: l), layouts: l)
        paused.feed(chord(code: a, side: .left, at: 0))
        paused.feed(chord(code: a, side: .left, at: 5000))
        XCTAssertEqual(paused.stats.sameHand, 0, "a pause exempts the next chord")
        XCTAssertEqual(paused.stats.eligiblePairs, 0, "and it is not in the denominator")
    }

    /// A dead chord types nothing, so nothing advances, but it is still an error.
    func testDeadChord() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "a", layouts: l), layouts: l)
        // Every finger down, which no table entry uses.
        session.feed(chord(code: 0x0ff, side: .left, at: 0))
        XCTAssertEqual(session.typed, "")
        XCTAssertEqual(session.stats.deadChords, 1)
        XCTAssertFalse(session.finished)
    }

    // MARK: helpers

    private func chord(code: UInt16, side: Side, at time: UInt32) -> Chord {
        Chord(timeMs: time, side: side, code: code, variant: "taipo",
              firstKeyMs: time, lastKeyMs: time, end: .allReleased)
    }

    private func codeTyping(_ text: String, _ l: Layouts) -> UInt16? {
        l.variants["taipo"]?.chords.first { $0.action.types == text }?.code
    }

    private func backspace(_ l: Layouts) -> UInt16? {
        l.variants["taipo"]?.chords.first {
            $0.action.kind == "key" && $0.action.key == "DeleteBackspace"
        }?.code
    }
}
