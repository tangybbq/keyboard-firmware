import XCTest

@testable import TaipoKit

/// The Orsy word table, ladder and drill.
final class OrsyDrillTests: XCTestCase {
    private func fixtures() throws -> (Layouts, OrsyTheory, OrsyWords) {
        let layouts = try Layouts.bundled()
        let theory = OrsyTheory(try XCTUnwrap(layouts.orsy))
        return (layouts, theory, try OrsyWords.bundled())
    }

    /// The right hand's space thumb: the space chord on its own, and what an ending vowel
    /// adds to the plain one.
    private let bk: UInt16 = 0x200

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
        XCTAssertEqual(words.lessons.count, 29)
        XCTAssertEqual(words.lessons[0].name, "transfer")
        XCTAssertEqual(words.lessons[1].name, "vowels")
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

    /// Reviewing only holds the ladder at what has been reached: the item that would
    /// have come out next stays locked, and the focus is spent on what is already out.
    func testReviewOnlyHoldsTheLadder() throws {
        let (_, theory, words) = try fixtures()
        var skills = [String: PatternSkill]()
        for key in words.lessons[0].items.flatMap({ $0 }) {
            skills[key] = PatternSkill(name: key, count: 50, medianMs: 400, deleted: 0)
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let advancing = OrsyLadder(words: words, theory: theory, skill: model)
        let reviewing = OrsyLadder(
            words: words, theory: theory, skill: model,
            options: OrsyLadder.Options(introduce: false))
        // The advancing ladder is out ahead, and everything reviewing has is reached.
        XCTAssertLessThan(reviewing.unlockedCount, advancing.unlockedCount)
        let pool = OrsyLadderMaker(words: words).pool(reviewing)
        let exercisable = Set(pool.flatMap(\.patterns))
        for item in reviewing.unlocked where exercisable.contains(item.key) {
            XCTAssertTrue(model.reached(item.key), item.key)
        }
        XCTAssertTrue(reviewing.focus.allSatisfy { model.reached($0.key) })
        // And the material still has something to ask for.
        var rng = DrillRandom(seed: 3)
        let line = OrsyLadderMaker(words: words).line(reviewing, words: 6, using: &rng)
        XCTAssertFalse(line.isEmpty)
        // Turning it back on puts the ladder exactly where it was.
        XCTAssertEqual(
            OrsyLadder(words: words, theory: theory, skill: model).unlockedCount,
            advancing.unlockedCount)
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

    /// A line introduces at most one stroke the learner has never made.
    ///
    /// Unlocking one item is not unlocking one stroke: a Series 2 character multiplies
    /// against every vowel and coda, so the pool gains a hundred strokes at once and the
    /// draw, left to itself, spreads them over every word of the line.
    func testALineIntroducesOneNewStrokeAtMost() throws {
        let (_, theory, words) = try fixtures()
        let l = try XCTUnwrap(words.lessons.firstIndex { $0.name == "second-i-t-m" })
        var skills = [String: PatternSkill]()
        for lesson in words.lessons[..<l] {
            for key in lesson.items.flatMap({ $0 }) {
                skills[key] = PatternSkill(name: key, count: 50, medianMs: 900, deleted: 0)
            }
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)

        // Everything the pool could reach before this lesson counts as made.
        let earlier = OrsyLadderMaker(words: words).pool(ladder)
        var made = Set<UInt32>()
        for word in earlier where !word.patterns.contains("s2:I") {
            for (left, right) in word.strokes {
                made.insert(OrsySkillModel.strokeId(left: left, right: right))
            }
        }
        func newStrokes(_ line: String, _ maker: OrsyLadderMaker) -> Int {
            line.split(separator: " ").reduce(0) { total, text in
                total + (words.word(String(text))?.strokes.filter {
                    !made.contains(OrsySkillModel.strokeId(left: $0.0, right: $0.1))
                }.count ?? 0)
            }
        }
        let paced = OrsyLadderMaker(words: words, made: made)
        let blind = OrsyLadderMaker(words: words)
        let a = paced.drill(ladder, lines: 6)
        let b = blind.drill(ladder, lines: 6)
        // One new stroke a line, plus the fallback's allowance: an item just unlocked has
        // no word that is not new, and one of them has to go first.
        let allowed = OrsyLadderMaker.newStrokesPerLine + ladder.focus.count
        for line in a.lines {
            XCTAssertLessThanOrEqual(newStrokes(line, paced), allowed, line)
        }
        // And the pacing is doing something: the blind draw spends more.
        let spentPaced = a.lines.reduce(0) { $0 + newStrokes($1, paced) }
        let spentBlind = b.lines.reduce(0) { $0 + newStrokes($1, blind) }
        XCTAssertLessThan(spentPaced, spentBlind)
        // Knowing nothing leaves the draw exactly as it was.
        XCTAssertEqual(blind.drill(ladder, lines: 6).lines, b.lines)
    }

    /// The rotation holds what the pool starves, and a block gives every one of them a
    /// word.
    ///
    /// Built at the `y` lesson, where the onset `y` has four words in the pool against
    /// `e␣`'s hundred and forty-four, so there is something to rescue.
    func testStarvedItemsGetAWordEachBlock() throws {
        let (_, theory, words) = try fixtures()
        let l = try XCTUnwrap(words.lessons.firstIndex { $0.name == "y" })
        var skills = [String: PatternSkill]()
        for lesson in words.lessons[...l] {
            for key in lesson.items.flatMap({ $0 }) {
                skills[key] = PatternSkill(name: key, count: 50, medianMs: 900, deleted: 0)
            }
        }
        let model = OrsySkillModel(skills: skills, sessions: 1, strokes: 100)
        let ladder = OrsyLadder(words: words, theory: theory, skill: model)
        let maker = OrsyLadderMaker(words: words)
        let rotation = maker.rotation(ladder)
        XCTAssertFalse(rotation.isEmpty)
        XCTAssertLessThanOrEqual(rotation.count, OrsyLadderMaker.rotationSize)

        // What it holds is the starved end: nothing in focus, and nothing the draw
        // already reaches as often as the middling item.
        let expected = OrsyLadderMaker.expectation(maker.pool(ladder))
        for key in rotation {
            XCTAssertFalse(ladder.focus.contains { $0.key == key }, key)
        }
        let middling = expected.values.sorted()[expected.count / 2]
        XCTAssertTrue(rotation.allSatisfy { (expected[$0] ?? 0) < middling }, "\(rotation)")
        // And it is ordered, most starved first.
        XCTAssertEqual(rotation, rotation.sorted { (expected[$0] ?? 0) < (expected[$1] ?? 0) })

        // A block long enough to go round gives each of them at least one word.
        var rng = DrillRandom(seed: 3)
        var seen = [String: Int]()
        for turn in 0..<rotation.count {
            let line = maker.line(
                ladder, words: 6, rotation: rotation, turn: turn, using: &rng)
            for text in line.split(separator: " ") {
                guard let word = words.word(String(text)) else { continue }
                for key in word.patterns where rotation.contains(key) {
                    seen[key, default: 0] += 1
                }
            }
        }
        for key in rotation {
            XCTAssertGreaterThan(seen[key] ?? 0, 0, "\(key) went unasked for in a block")
        }
    }

    /// A pool that spreads itself evenly starves nothing, and the line is as it was.
    func testTheFirstLessonNeedsNoRotation() throws {
        let (_, theory, words) = try fixtures()
        let empty = OrsySkillModel(skills: [:], sessions: 0, strokes: 0)
        let ladder = OrsyLadder(words: words, theory: theory, skill: empty)
        let maker = OrsyLadderMaker(words: words)
        var a = DrillRandom(seed: 11)
        var b = DrillRandom(seed: 11)
        let with = maker.line(ladder, words: 6, using: &a)
        let without = maker.line(ladder, words: 6, rotation: [], turn: 0, using: &b)
        if maker.rotation(ladder).isEmpty { XCTAssertEqual(with, without) }
        XCTAssertFalse(with.isEmpty)
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
        // A second character on a vowel's chord says so, whether it spells the vowel or
        // only reads as it when mirrored.
        XCTAssertTrue(of("s2:RXI")?.hasPrefix("same chord as the vowel o") ?? false)
        XCTAssertTrue(of("s2:R")?.hasPrefix("mirrored a: same chord as the vowel a") ?? false)
        XCTAssertTrue(of("s2:XI")?.hasPrefix("mirrored o: same chord as the ending o") ?? false)
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

    /// The line's last word has to be closed like any other.
    ///
    /// It has no space after it in the target to check the close against, so a word typed
    /// with the plain vowel where the ending form was wanted used to match the text
    /// exactly and count the line as finished -- the one mistake the drill is built to
    /// catch, unmarked.  And since `finished` is what gates `feed`, the writer could not
    /// take the word back and close it either.
    func testLastWordMustBeClosed() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "the", words: words, theory: theory)
        let closing = try XCTUnwrap(target.units.last)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        // The table's own stroke with the space thumb let go: the plain vowel, which
        // spells the same letters and leaves the word open.
        session.feed(stroke(theory, closing.left, closing.right & ~bk, at: 1000))
        XCTAssertEqual(session.typed, "the")
        XCTAssertTrue(session.onTrack)
        XCTAssertFalse(session.finished, "the word is still open")
        XCTAssertTrue(session.wordOpen)
        XCTAssertTrue(session.finalWordOpen)
        XCTAssertFalse(session.wordClosedEarly)
        // Undo and use the ending form: now the line is done.
        session.feed(stroke(theory, 0x067, 0, at: 1500))
        XCTAssertEqual(session.typed, "")
        session.feed(stroke(theory, closing.left, closing.right, at: 2000))
        XCTAssertEqual(session.typed, "the")
        XCTAssertTrue(session.finished)
        XCTAssertFalse(session.wordOpen)
    }

    /// Striking the space closes the last word too, and is not a mistake: it is the same
    /// choice the writer has at every other word boundary.
    func testLastWordClosedByStrikingTheSpace() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "the", words: words, theory: theory)
        let closing = try XCTUnwrap(target.units.last)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        session.feed(stroke(theory, closing.left, closing.right & ~bk, at: 1000))
        session.feed(stroke(theory, 0, bk, at: 1500))
        XCTAssertEqual(session.typed, "the ")
        XCTAssertTrue(session.onTrack)
        XCTAssertTrue(session.finished)
        XCTAssertEqual(session.stats.wrong, 0)
    }

    /// A word the table closes itself needs nothing added: a final coda-only stroke leans
    /// back onto the syllable before it and ends the word.
    func testLastWordClosedByItsOwnCoda() throws {
        let (layouts, theory, words) = try fixtures()
        let target = OrsyDrillTarget(text: "ten its", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        for unit in target.units {
            session.feed(stroke(theory, unit.left, unit.right, at: 1000))
        }
        XCTAssertTrue(session.finished)
        XCTAssertFalse(session.wordOpen)
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

    /// Strike a chord on the stroke engine key by key, as the keyboard reports it: the
    /// left hand's keys, then `fn` if there is one, then the right hand's, all down and
    /// then all up in the same order.
    private func strike(
        _ engine: StrokeEngine, _ layouts: Layouts, left: UInt16, right: UInt16,
        fn: Int? = nil, at ms: UInt32
    ) throws -> Stroke? {
        func keys(_ side: String, _ chord: UInt16) throws -> [Int] {
            try (0..<10).filter { chord & (1 << $0) != 0 }.map { bit in
                try XCTUnwrap(
                    layouts.scanMap.upper.first { $0.side == side && $0.mask == 1 << bit }?.key)
            }
        }
        let all = try keys("left", left) + (fn.map { [$0] } ?? []) + keys("right", right)
        var done: Stroke?
        for key in all {
            XCTAssertNil(engine.feed(key: key, press: true, timeMs: ms, lowerRow: false))
        }
        for key in all {
            if let s = engine.feed(key: key, press: false, timeMs: ms + 30, lowerRow: false) {
                XCTAssertNil(done, "one chord, one stroke")
                done = s
            }
        }
        return done
    }

    /// An Fn key struck with Enter's Dosh chord on the other hand moves on, from either
    /// hand, as the chord form does; alone it is the toggle; and with keys on its own hand
    /// it is dead.
    func testFnKeys() throws {
        let (layouts, theory, words) = try fixtures()
        let fnLeft = try XCTUnwrap(layouts.specialKeys?.fnLeft)
        let fnRight = try XCTUnwrap(layouts.specialKeys?.fnRight)
        let target = OrsyDrillTarget(text: "ten", words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        let engine = StrokeEngine(layouts: layouts, theory: theory)
        let enter = try XCTUnwrap(
            layouts.variants["dosh"]?.chords.first {
                $0.action.kind == "key" && $0.action.key == "ReturnEnter"
            }?.code)

        let viaLeft = try XCTUnwrap(
            strike(engine, layouts, left: 0, right: enter, fn: fnLeft, at: 1000))
        XCTAssertEqual(viaLeft.outcome, .dosh(enter))
        XCTAssertEqual(session.control(for: viaLeft), .next)

        let viaRight = try XCTUnwrap(
            strike(engine, layouts, left: enter, right: 0, fn: fnRight, at: 2000))
        XCTAssertEqual(viaRight.outcome, .dosh(enter))
        XCTAssertEqual(session.control(for: viaRight), .next)

        // The chord form still works.
        let chord = try XCTUnwrap(
            strike(engine, layouts, left: 0x380, right: enter, at: 3000))
        XCTAssertEqual(session.control(for: chord), .next)

        let toggle = try XCTUnwrap(strike(engine, layouts, left: 0, right: 0, fn: fnLeft, at: 4000))
        XCTAssertEqual(toggle.outcome, .doshToggle)

        let dead = try XCTUnwrap(
            strike(engine, layouts, left: enter, right: 0, fn: fnLeft, at: 5000))
        XCTAssertEqual(dead.outcome, .dead)
    }
}
