import XCTest

@testable import TaipoKit

/// The Orsy word table, ladder and drill.
final class OrsyDrillTests: XCTestCase {
    private func fixtures() throws -> (Layouts, OrsyTheory, OrsyWords) {
        let layouts = try Layouts.bundled()
        let theory = OrsyTheory(try XCTUnwrap(layouts.orsy))
        return (layouts, theory, try OrsyWords.bundled())
    }

    /// A stroke as the engine would report it, at a time.
    private func stroke(_ theory: OrsyTheory, _ left: UInt16, _ right: UInt16, at ms: UInt32)
        -> Stroke
    {
        Stroke(
            timeMs: ms, left: left, right: right, firstKeyMs: ms - 30, lastKeyMs: ms - 20,
            outcome: theory.outcome(left: left, right: right))
    }

    /// The table's divisions translate to their words, and the plan is the drill sheets'.
    func testWordTable() throws {
        let (_, theory, words) = try fixtures()
        XCTAssertGreaterThan(words.words.count, 9000)
        XCTAssertEqual(words.lessons.count, 30)
        XCTAssertEqual(words.lessons[0].name, "transfer")
        for word in words.words.prefix(500) {
            var output = OrsyOutput()
            var text = ""
            for (l, r) in word.strokes {
                let t = try XCTUnwrap(theory.translate(left: l, right: r), word.text)
                text += output.stroke(t)
            }
            XCTAssertEqual(text, word.text)
        }
        let the = try XCTUnwrap(words.word("the"))
        XCTAssertEqual(the.patterns, ["s1:FZ", "s3:ue"])
    }

    /// With nothing typed the first lesson is out, and the focus is its weakest items.
    func testLadderStartsAtTheFirstLesson() throws {
        let (_, theory, words) = try fixtures()
        let empty = OrsySkillModel(skills: [:], sessions: 0, strokes: 0)
        let ladder = OrsyLadder(words: words, theory: theory, skill: empty)
        XCTAssertEqual(ladder.unlockedCount, words.lessons[0].items.count)
        XCTAssertEqual(ladder.items[0].label, "s")
        XCTAssertEqual(ladder.items[0].keys, ["s1:S", "s4:S"])
        XCTAssertEqual(ladder.focus.count, 3)
        XCTAssertEqual(ladder.lesson?.name, "transfer")
        // The h/st shape is labelled by both readings.
        XCTAssertTrue(ladder.items.contains { $0.label == "h/st" })
        // The pool is the transfer lesson's words.
        let pool = OrsyLadderMaker(words: words).pool(ladder).map(\.text)
        XCTAssertTrue(pool.contains("ten"))
        XCTAssertTrue(pool.contains("its"))
        XCTAssertFalse(pool.contains("the"))
    }

    /// Reaching the first lesson's items unlocks more, and a line is drawn from what is out.
    func testLadderAdvances() throws {
        let (_, theory, words) = try fixtures()
        var skills = [String: PatternSkill]()
        for key in words.lessons[0].items.flatMap({ $0 }) {
            skills[key] = PatternSkill(name: key, count: 20, medianMs: 400, deleted: 0)
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)
        XCTAssertGreaterThan(ladder.unlockedCount, words.lessons[0].items.count)
        // The focus is the newly unlocked, unreached items.
        XCTAssertTrue(ladder.focus.allSatisfy { !$0.keys.allSatisfy(model.reached) })
        var rng = DrillRandom(seed: 1)
        let line = OrsyLadderMaker(words: words).line(ladder, words: 6, using: &rng)
        XCTAssertFalse(line.isEmpty)
        let unlocked = ladder.unlockedKeys
        for word in line.split(separator: " ") {
            let entry = try XCTUnwrap(words.word(String(word)), String(word))
            XCTAssertTrue(entry.patterns.isSubset(of: unlocked), String(word))
        }
    }

    /// Typing the target's own division is all correct, with the spacing coming from the
    /// output stage.
    func testTypingTheDivision() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "ten its", words: words, theory: theory)
        XCTAssertEqual(target.units.map(\.text), ["ten", " it", "s"])
        XCTAssertEqual(target.units.map(\.offset), [0, 3, 6])
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        var ms: UInt32 = 1000
        for unit in target.units {
            XCTAssertEqual(session.wantedStroke, unit)
            session.feed(stroke(theory, unit.left, unit.right, at: ms))
            ms += 500
        }
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.correct, 3)
        XCTAssertEqual(session.stats.wrong, 0)
        XCTAssertEqual(session.stats.characters, 7)
        // From the first key of the first stroke to the commit of the last.
        XCTAssertEqual(session.stats.elapsedMs, 1030)
        // t + e+Bk+n, i+t, s.
        XCTAssertEqual(session.stats.keysPerStroke, 7.0 / 3.0, accuracy: 0.01)
    }

    /// A different valid division is right too.
    func testAnotherDivisionIsRight() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "tens", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        // te | ns rather than the table's ten | s: t + e (binds forward), then n + s.
        session.feed(stroke(theory, 0x004, 0x008, at: 1000))
        session.feed(stroke(theory, 0x040, 0x020, at: 1500))
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.wrong, 0)
        XCTAssertEqual(session.stats.textStrokes, 2)
    }

    /// A wrong stroke is diagnosed by Series, and undo takes it back.
    func testWrongStrokeIsDiagnosedAndUndone() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "its", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        // i + t + s on the right: coda s+t is l, so `il`.
        session.feed(stroke(theory, 0, 0x080 | 0x004 | 0x020, at: 1000))
        XCTAssertFalse(session.onTrack)
        XCTAssertEqual(session.stats.wrong, 1)
        XCTAssertEqual(session.divergedAt, 0)
        XCTAssertEqual(session.diagnosis, "coda: l for t")
        guard case .wrong(let expected, let got, let series) = session.events.last else {
            return XCTFail("not a wrong event")
        }
        XCTAssertEqual(expected, "it")
        XCTAssertEqual(got, "il")
        XCTAssertEqual(series, ["coda"])
        // Undo, then the right strokes.
        session.feed(stroke(theory, 0x067, 0, at: 1500))
        XCTAssertTrue(session.onTrack)
        XCTAssertEqual(session.typed, "")
        XCTAssertEqual(session.stats.corrections, 1)
        session.feed(stroke(theory, 0, 0x084, at: 2000))
        session.feed(stroke(theory, 0, 0x020, at: 2500))
        XCTAssertTrue(session.finished)
        XCTAssertNil(session.diagnosis)
    }

    /// A word left open is not a mistake until the next word runs on; undoing back to
    /// it and closing it properly then gets no stray space, because the undo puts the
    /// spacing state back as it was.
    func testUndoRestoresTheSpacingState() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "tennis sit", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        // ten, then nis with the plain vowel: the word is left open.
        session.feed(stroke(theory, 0x004, 0x048, at: 1000))
        XCTAssertFalse(session.wordOpen, "mid-word is not open")
        session.feed(stroke(theory, 0x040, 0x0a0, at: 1500))
        XCTAssertEqual(session.typed, "tennis")
        XCTAssertTrue(session.onTrack)
        XCTAssertTrue(session.wordOpen)
        // The next word runs on: no space.
        session.feed(stroke(theory, 0x020, 0x284, at: 2000))
        XCTAssertEqual(session.typed, "tennissit")
        XCTAssertFalse(session.onTrack)
        // Undo twice, back to `ten`.
        session.feed(stroke(theory, 0x067, 0, at: 2500))
        session.feed(stroke(theory, 0x067, 0, at: 3000))
        XCTAssertEqual(session.typed, "ten")
        XCTAssertEqual(session.wantedStroke?.text, "nis")
        // nis with the ending form: no space before it, and the word closes.
        session.feed(stroke(theory, 0x040, 0x2a0, at: 3500))
        XCTAssertEqual(session.typed, "tennis")
        XCTAssertTrue(session.onTrack)
        XCTAssertFalse(session.wordOpen)
        session.feed(stroke(theory, 0x020, 0x284, at: 4000))
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.wrong, 1)
    }

    /// A dead stroke counts, and the escapes control the line.
    func testDeadAndControls() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "ten", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        session.feed(stroke(theory, 0x380, 0, at: 1000))
        XCTAssertEqual(session.stats.dead, 1)
        XCTAssertTrue(session.stumbled)
        let enter = try XCTUnwrap(
            layouts.variants["dosh"]?.chords.first {
                $0.action.kind == "key" && $0.action.key == "ReturnEnter"
            }?.code)
        XCTAssertEqual(session.control(for: stroke(theory, 0x380, enter, at: 2000)), .next)
        XCTAssertEqual(session.control(for: stroke(theory, 0x380, 0x300, at: 2000)), .restart)
        XCTAssertNil(session.control(for: stroke(theory, 0x004, 0x248, at: 2000)))
    }
}
