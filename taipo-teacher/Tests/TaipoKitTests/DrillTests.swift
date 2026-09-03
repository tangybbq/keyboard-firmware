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

    // MARK: - What the hint needs

    /// The chord the target wants next is the one the hint draws.
    func testWantedChordFollowsTheCursor() throws {
        let l = try layouts()
        // Not `at`, which is one of taipo's gram chords and so a single unit.
        let session = DrillSession(target: DrillTarget(text: "ax", layouts: l), layouts: l)
        let a = try XCTUnwrap(codeTyping("a", l))
        let x = try XCTUnwrap(codeTyping("x", l))
        XCTAssertEqual(session.target.units.count, 2)

        XCTAssertEqual(session.wantedChord?.code, a)
        session.feed(chord(code: a, side: .left, at: 0))
        XCTAssertEqual(session.wantedChord?.code, x)
        session.feed(chord(code: x, side: .right, at: 200))
        XCTAssertNil(session.wantedChord, "nothing is wanted once the line is done")
    }

    /// Off the target, the chord worth showing is the one that was wanted where it went
    /// wrong -- not whatever is under a cursor that has since moved on.
    func testWantedChordStaysWhereItWentWrong() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "ax", layouts: l), layouts: l)
        let a = try XCTUnwrap(codeTyping("a", l))
        let o = try XCTUnwrap(codeTyping("o", l))
        let bk = try XCTUnwrap(backspace(l))

        session.feed(chord(code: o, side: .left, at: 0))
        XCTAssertFalse(session.onTrack)
        XCTAssertEqual(session.divergedAt, 0)
        XCTAssertEqual(session.wantedChord?.code, a, "still the `a` that was wanted")
        XCTAssertTrue(session.stumbled)

        // Backspacing onto the target ends the divergence, but the stumble stands until
        // something goes right -- which is what keeps the hint up while it is needed.
        session.feed(chord(code: bk, side: .right, at: 200))
        XCTAssertTrue(session.onTrack)
        XCTAssertNil(session.divergedAt)
        XCTAssertTrue(session.stumbled)
        XCTAssertEqual(session.wantedChord?.code, a)

        session.feed(chord(code: a, side: .left, at: 400))
        XCTAssertFalse(session.stumbled, "and it clears once the chord lands")
    }

    /// A dead chord types nothing, so nothing else would mark it -- but reaching for a
    /// chord that does not exist is the clearest case for a hint there is.
    func testADeadChordCountsAsAStumble() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "a", layouts: l), layouts: l)

        XCTAssertFalse(session.stumbled)
        session.feed(chord(code: 0x0ff, side: .left, at: 0))
        XCTAssertTrue(session.stumbled)
        XCTAssertEqual(session.wantedChord?.code, try XCTUnwrap(codeTyping("a", l)))
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

/// Driving the app from the keyboard, and the numbers it reports.
final class DrillControlTests: XCTestCase {

    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func chord(code: UInt16, side: Side, at time: UInt32) -> Chord {
        Chord(timeMs: time, side: side, code: code, variant: "taipo",
              firstKeyMs: time, lastKeyMs: time, end: .allReleased)
    }

    /// The two chords that type nothing are what move between lines, so the trainer never
    /// needs the mouse.
    func testControlChords() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "a", layouts: l), layouts: l)

        // Enter is [o+t+e]; the null chord is both thumbs.
        let enter = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.key == "ReturnEnter"
        }?.code)
        let null = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.kind == "release"
        }?.code)

        XCTAssertEqual(session.control(for: chord(code: enter, side: .left, at: 0)), .next)
        XCTAssertEqual(session.control(for: chord(code: null, side: .left, at: 0)), .restart)

        // A letter is not a control chord.
        let a = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "a" }?.code)
        XCTAssertNil(session.control(for: chord(code: a, side: .left, at: 0)))
    }

    /// Words per minute on the standard five-characters-to-a-word convention, which is the
    /// number a person can compare against anything else.
    func testWordsPerMinute() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "the the", layouts: l), layouts: l)
        let the = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "the" }?.code)
        let space = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == " " }?.code)

        // "the the" is seven characters and three chords, which is the layout doing its
        // job: 1.4 words in four seconds is 21 wpm, and 45 chords a minute produced it.
        session.feed(chord(code: the, side: .left, at: 0))
        session.feed(chord(code: space, side: .right, at: 2000))
        session.feed(chord(code: the, side: .left, at: 4000))
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.characters, 7)
        XCTAssertEqual(session.stats.wordsPerMinute, 21, accuracy: 0.5)
        XCTAssertEqual(session.stats.chordsPerMinute, 45, accuracy: 0.5)
    }

    /// A same-hand pair is recorded against the characters it produced, so it can be shown
    /// where it happened rather than only counted.
    func testSameHandIsAttributedToCharacters() throws {
        let l = try layouts()
        let session = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        let a = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "a" }?.code)
        let t = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "t" }?.code)

        session.feed(chord(code: a, side: .left, at: 0))
        session.feed(chord(code: t, side: .left, at: 200))
        XCTAssertEqual(session.sameHandOffsets, [1], "the second character carries the fault")

        // Backspacing over it takes the mark with it.
        let bk = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.key == "DeleteBackspace"
        }?.code)
        session.feed(chord(code: bk, side: .right, at: 400))
        XCTAssertEqual(session.sameHandOffsets, [])
    }
}

/// Alternation applies to corrections too: correcting is still typing.
final class CorrectionHandTests: XCTestCase {

    private func chord(code: UInt16, side: Side, at time: UInt32) -> Chord {
        Chord(timeMs: time, side: side, code: code, variant: "taipo",
              firstKeyMs: time, lastKeyMs: time, end: .allReleased)
    }

    /// A backspace on the same hand as the chord it deletes is a fault, and is counted
    /// where it can be seen -- there is nothing in the target line to mark, because a
    /// backspace types no character.
    func testBackspaceOnTheSameHandIsAFault() throws {
        let l = try Layouts.bundled()
        let a = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "a" }?.code)
        let bk = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.key == "DeleteBackspace"
        }?.code)

        let bad = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        bad.feed(chord(code: a, side: .left, at: 0))
        bad.feed(chord(code: bk, side: .left, at: 200))
        XCTAssertEqual(bad.stats.sameHand, 1)
        XCTAssertEqual(bad.stats.sameHandCorrections, 1)

        // The same correction taken on the other hand is clean.
        let good = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        good.feed(chord(code: a, side: .left, at: 0))
        good.feed(chord(code: bk, side: .right, at: 200))
        XCTAssertEqual(good.stats.sameHand, 0)
        XCTAssertEqual(good.stats.sameHandCorrections, 0)
        XCTAssertEqual(good.stats.corrections, 1, "still a correction, just a tidy one")
    }

    /// A run of backspaces has to keep alternating, which is where it is easiest to stop.
    func testBackspaceRunMustAlternate() throws {
        let l = try Layouts.bundled()
        let bk = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.key == "DeleteBackspace"
        }?.code)

        let session = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        for i in 0..<4 {
            session.feed(chord(code: bk, side: .left, at: UInt32(i) * 200))
        }
        XCTAssertEqual(session.stats.sameHandCorrections, 3, "three of the four pairs")

        let alternated = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        for i in 0..<4 {
            alternated.feed(
                chord(code: bk, side: i.isMultiple(of: 2) ? .left : .right, at: UInt32(i) * 200))
        }
        XCTAssertEqual(alternated.stats.sameHandCorrections, 0)
    }

    /// And a pause still exempts it: stopping to work out what went wrong is not a fault.
    func testPausedCorrectionIsExempt() throws {
        let l = try Layouts.bundled()
        let a = try XCTUnwrap(l.variants["taipo"]?.chords.first { $0.action.types == "a" }?.code)
        let bk = try XCTUnwrap(l.variants["taipo"]?.chords.first {
            $0.action.key == "DeleteBackspace"
        }?.code)

        let session = DrillSession(target: DrillTarget(text: "at", layouts: l), layouts: l)
        session.feed(chord(code: a, side: .left, at: 0))
        session.feed(chord(code: bk, side: .left, at: 6000))
        XCTAssertEqual(session.stats.sameHandCorrections, 0)
        XCTAssertEqual(session.stats.eligiblePairs, 0)
    }
}
