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

    /// With nothing typed the first lesson is out, one item per reading, and the focus is
    /// its weakest items.
    func testLadderStartsAtTheFirstLesson() throws {
        let (_, theory, words) = try fixtures()
        let empty = OrsySkillModel(skills: [:], sessions: 0, strokes: 0)
        let ladder = OrsyLadder(words: words, theory: theory, skill: empty)
        let keysInLessonOne = words.lessons[0].items.reduce(0) { $0 + $1.count }
        XCTAssertEqual(ladder.unlockedCount, keysInLessonOne)
        // A shape is two adjacent items, one per reading.
        XCTAssertEqual(ladder.items[0].key, "s1:S")
        XCTAssertEqual(ladder.items[0].label, "s")
        XCTAssertEqual(ladder.items[0].stage, .onset)
        XCTAssertEqual(ladder.items[1].key, "s4:S")
        XCTAssertEqual(ladder.items[1].stage, .coda)
        XCTAssertEqual(ladder.focus.count, 2)
        XCTAssertEqual(ladder.lesson?.name, "transfer")
        // The shape whose readings differ is two items, labelled apart.
        XCTAssertTrue(ladder.items.contains { $0.key == "s1:FC" && $0.label == "h" })
        XCTAssertTrue(ladder.items.contains { $0.key == "s4:FC" && $0.label == "st" })
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
            skills[key] = PatternSkill(name: key, count: 50, medianMs: 400, deleted: 0)
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)
        let keysInLessonOne = words.lessons[0].items.reduce(0) { $0 + $1.count }
        XCTAssertGreaterThan(ladder.unlockedCount, keysInLessonOne)
        // The focus is the newly unlocked, unreached items.
        XCTAssertTrue(ladder.focus.allSatisfy { !model.reached($0.key) })
        var rng = DrillRandom(seed: 1)
        let line = OrsyLadderMaker(words: words).line(ladder, words: 6, using: &rng)
        XCTAssertFalse(line.isEmpty)
        let unlocked = ladder.unlockedKeys
        for word in line.split(separator: " ") {
            let entry = try XCTUnwrap(words.word(String(word)), String(word))
            XCTAssertTrue(entry.patterns.isSubset(of: unlocked), String(word))
        }
    }

    /// A line works the reading the ladder is waiting on: with the coda `s` known and the
    /// onset not, every line has a word with an onset `s`.
    func testLinesDrillTheWeakReading() throws {
        let (_, theory, words) = try fixtures()
        var skills = [String: PatternSkill]()
        for key in words.lessons[0].items.flatMap({ $0 }) where key != "s1:S" {
            skills[key] = PatternSkill(name: key, count: 50, medianMs: 400, deleted: 0)
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)
        XCTAssertTrue(ladder.focus.contains { $0.key == "s1:S" })
        var rng = DrillRandom(seed: 7)
        for _ in 0..<8 {
            let line = OrsyLadderMaker(words: words).line(ladder, words: 6, using: &rng)
            let onsetS = line.split(separator: " ").filter {
                words.word(String($0))?.patterns.contains("s1:S") ?? false
            }.count
            XCTAssertGreaterThanOrEqual(onsetS, OrsyLadderMaker.perFocus, line)
        }
    }

    /// An item no word in the pool can exercise is not held against the ladder: with
    /// everything through the `x` lesson reached but the onset `x`, whose one word needs
    /// the last lesson's rule, the ladder moves on and the item takes no focus place.
    func testUnreachableReadingDoesNotStallTheLadder() throws {
        let (_, theory, words) = try fixtures()
        let l = try XCTUnwrap(words.lessons.firstIndex { $0.name == "x" })
        var skills = [String: PatternSkill]()
        for lesson in words.lessons[...l] {
            for key in lesson.items.flatMap({ $0 }) where key != "s1:SZN" {
                skills[key] = PatternSkill(name: key, count: 50, medianMs: 400, deleted: 0)
            }
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)
        let through = words.lessons[...l].reduce(0) { total, lesson in
            total + lesson.items.reduce(0) { $0 + $1.count }
        }
        XCTAssertGreaterThan(ladder.unlockedCount, through, "the ladder should move past x")
        XCTAssertFalse(ladder.focus.contains { $0.key == "s1:SZN" })
        // No line asks for it either.
        let pool = OrsyLadderMaker(words: words).pool(ladder)
        XCTAssertFalse(pool.contains { $0.patterns.contains("s1:SZN") })
    }

    /// The marks are right-handed strokes on their own, and the output stage attaches
    /// them, owes the next word a space, and capitalises after a sentence-ender.
    func testPunctuation() throws {
        let (_, theory, _) = try fixtures()
        let marks = theory.tables.punctuation
        XCTAssertEqual(marks.first?.text, ".")
        for mark in marks {
            // Nothing else claims the chord, on either reading.
            XCTAssertEqual(theory.outcome(left: 0, right: mark.bits), .punct(mark), mark.text)
            XCTAssertNil(theory.translate(left: 0, right: mark.bits), mark.text)
            XCTAssertEqual(mark.keys.last, "Bk", mark.text)
        }

        func find(_ text: String) throws -> Layouts.Orsy.Punctuation {
            try XCTUnwrap(marks.first { $0.text == text })
        }
        var output = OrsyOutput()
        let ten = try XCTUnwrap(theory.translate(left: 0x004, right: 0x248))
        let tenOpen = try XCTUnwrap(theory.translate(left: 0x004, right: 0x048))
        XCTAssertEqual(output.stroke(ten), "ten")
        XCTAssertEqual(output.mark(try find(".")), ".")
        // The next word gets its space back, and its capital.
        XCTAssertEqual(output.stroke(ten), " Ten")
        // A mark after a word left open attaches and still owes the space.
        XCTAssertEqual(output.stroke(tenOpen), " ten")
        XCTAssertEqual(output.mark(try find(",")), ",")
        XCTAssertEqual(output.stroke(ten), " ten")
        // And it is on the record, so undo takes it back.
        XCTAssertEqual(output.undo(), 4)
        XCTAssertEqual(output.undo(), 1)
        XCTAssertEqual(String(output.recent), "ten. Ten ten")
    }

    /// The apostrophe and the hyphen bind forward: what follows joins the same word.
    func testBindingMarks() throws {
        let (_, theory, _) = try fixtures()
        let marks = theory.tables.punctuation
        let apostrophe = try XCTUnwrap(marks.first { $0.text == "'" })
        XCTAssertFalse(apostrophe.spaceAfter)
        var output = OrsyOutput()
        // `ten` with a plain vowel, an apostrophe, then a coda-only s.
        let open = try XCTUnwrap(theory.translate(left: 0x004, right: 0x048))
        let codaS = try XCTUnwrap(theory.translate(left: 0, right: 0x020))
        _ = output.stroke(open)
        _ = output.mark(apostrophe)
        _ = output.stroke(codaS)
        XCTAssertEqual(String(output.recent), "ten's")
        // And the coda closed the word, so the next one gets its space.
        XCTAssertEqual(output.stroke(open), " ten")
    }

    /// A stroke's skill key names the mark, so the model can measure it.
    func testPunctuationSkillKeys() throws {
        let (_, theory, _) = try fixtures()
        let stroke = Stroke(
            timeMs: 100, left: 0, right: 0x204, firstKeyMs: 70, lastKeyMs: 80,
            outcome: theory.outcome(left: 0, right: 0x204))
        XCTAssertEqual(OrsySamples.keys(stroke), ["punct:."])
    }

    /// The mnemonics come out of the tables: the voicing rule, the ending form, the
    /// two-letter vowels, and what Dosh does with the same keys.
    func testMnemonics() throws {
        let (layouts, theory, _) = try fixtures()
        func of(_ key: String) -> String? {
            OrsyMnemonic.of(key, tables: theory.tables, layouts: layouts)
        }
        // d is t plus the pinky, and Dosh types q there.
        XCTAssertEqual(of("s1:SCP"), "t + pinky · Dosh: q")
        // The coda reading takes the coda of the same base shape.
        XCTAssertTrue(of("s4:SCP")?.hasPrefix("t + pinky") ?? false)
        // s transfers from Dosh untouched.
        XCTAssertEqual(of("s1:S"), "as in Dosh")
        // An ending vowel is the plain one plus the space thumb.
        XCTAssertTrue(of("s3:ue")?.hasPrefix("e + Bk") ?? false)
        // And ea is e plus a.
        XCTAssertTrue(of("s3:ea")?.hasPrefix("e + a") ?? false)
        // A rule has no mnemonic to derive.
        XCTAssertNil(of("rule:mirrored"))
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

    /// The ending form in the middle of a word closes it early: flagged at once, and the
    /// space it puts on the next stroke is explained.
    func testWordClosedEarly() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "tennis", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        // ten with the ending form.
        session.feed(stroke(theory, 0x004, 0x248, at: 1000))
        XCTAssertTrue(session.onTrack)
        XCTAssertTrue(session.wordClosedEarly)
        XCTAssertFalse(session.wordOpen)
        // nis, correct in itself, arrives with a space.
        session.feed(stroke(theory, 0x040, 0x2a0, at: 1500))
        XCTAssertEqual(session.typed, "ten nis")
        XCTAssertFalse(session.onTrack)
        guard case .wrong(_, let got, let series) = session.events.last else {
            return XCTFail("not a wrong event")
        }
        XCTAssertEqual(got, " nis")
        XCTAssertEqual(series.first, "space")
        XCTAssertTrue(session.diagnosis?.hasPrefix("space: the stroke before closed the word early") ?? false)
        XCTAssertTrue(session.diagnosis?.contains("this stroke was right") ?? false)
        // Undo both, plain ten, then nis: right.
        session.feed(stroke(theory, 0x067, 0, at: 2000))
        session.feed(stroke(theory, 0x067, 0, at: 2500))
        session.feed(stroke(theory, 0x004, 0x048, at: 3000))
        XCTAssertFalse(session.wordClosedEarly)
        session.feed(stroke(theory, 0x040, 0x2a0, at: 3500))
        XCTAssertTrue(session.finished)
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
