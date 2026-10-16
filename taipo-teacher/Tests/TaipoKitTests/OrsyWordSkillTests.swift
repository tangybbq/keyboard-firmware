import XCTest

@testable import TaipoKit

/// The per-word measurements: which words a stream of strokes typed, how long each took,
/// and whether it was got right first time.
final class OrsyWordSkillTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func golden(_ name: String, _ ext: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/\(name)", withExtension: ext))
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// A log directory holding one file, headed as the collector heads them.
    private func logDirectory(_ contents: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("orsy-words-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let header = "# session device=test boot_id=0x1 layout=\(try layouts().fingerprint)\n"
        try (header + contents).write(
            to: dir.appendingPathComponent("2026-09-01.txt"), atomically: true, encoding: .utf8)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    /// The words of a log, as the fold sees them.
    private func typed(_ log: String) throws -> [TypedWord] {
        let layouts = try layouts()
        var out = [TypedWord]()
        for session in KeyLogFile.sessions(from: log, layouts: layouts) {
            let engine = ChordEngine(layouts: layouts)
            var strokes = [Stroke]()
            engine.onStroke = { strokes.append($0) }
            for entry in session.entries {
                switch entry {
                case .marker(let m): engine.marker(m.name, value: m.value)
                case .key(let e): _ = engine.feed(key: e.key, press: e.press, timeMs: e.timeMs)
                default: break
                }
            }
            _ = engine.finish()
            var tracker = OrsyWordTracker(pauseMs: 4000, layouts: layouts)
            for s in strokes { tracker.feed(s) }
            tracker.forget()
            out += tracker.finished
        }
        return out
    }

    private func theory() throws -> OrsyTheory { OrsyTheory(try XCTUnwrap(try layouts().orsy)) }

    /// A stroke as the engine would report it, at a time.
    private func stroke(_ left: UInt16, _ right: UInt16, at ms: UInt32) throws -> Stroke {
        Stroke(
            timeMs: ms, left: left, right: right, firstKeyMs: ms - 30, lastKeyMs: ms - 20,
            outcome: try theory().outcome(left: left, right: right))
    }

    /// The strokes of a word, as the table divides it, from `ms` on at `step` apart.
    private func strokes(_ text: String, at ms: UInt32, step: UInt32 = 100) throws -> [Stroke] {
        let word = try XCTUnwrap(try OrsyWords.bundled().word(text), text)
        return try word.strokes.enumerated().map { i, s in
            try stroke(s.0, s.1, at: ms + UInt32(i) * step)
        }
    }

    private func track(_ strokes: [Stroke]) throws -> [TypedWord] {
        var tracker = OrsyWordTracker(pauseMs: 4000, layouts: try layouts())
        for s in strokes { tracker.feed(s) }
        tracker.forget()
        return tracker.finished
    }

    /// Every word, timed from the end of the one before; the first has nothing before it.
    func testCleanTypingIsTimedWordByWord() throws {
        let words = try typed(try golden("orsy-clean", "log"))
        XCTAssertEqual(
            words.map(\.text), ["the", "quick", "brown", "fox", "jumps", "over", "the", "lazy", "dog"])
        XCTAssertTrue(words.allSatisfy(\.clean))
        // Strokes are 90ms apart; brown, jumps, over and lazy are two strokes each.
        XCTAssertEqual(words.map(\.ms), [nil, 90, 180, 90, 180, 180, 90, 180, 90])
    }

    /// A word that had to be taken back is not clean, whether the undo took it back
    /// whole -- and the space before it, reopening the word before -- or a run of
    /// escaped backspaces did, or a dead stroke came before it; and the words either side
    /// are untouched.
    func testCorrectionsAreChargedToTheWordBeingAimedAt() throws {
        let words = try typed(try golden("orsy-sloppy", "log"))
        XCTAssertEqual(
            words.map(\.text), ["the", "rain", "in", "spain", "falls", "mainly", "on", "the", "plain"])
        XCTAssertEqual(
            words.map(\.clean), [true, true, true, false, true, false, true, true, false])
        // `in` was reopened by the undo of ` pain` and closed again without being
        // touched, so it keeps its time; `falls` is timed from the end of `spain`.
        XCTAssertEqual(words[2].ms, 301 - 208)
        XCTAssertEqual(words[4].ms, 915 - 701)
    }

    /// A pause anywhere in a word leaves it untimed, but it is still a use.
    func testAPauseLeavesTheWordUntimed() throws {
        let words = try track(
            try strokes("the", at: 1000) + strokes("of", at: 2000) + strokes("and", at: 7000)
                + strokes("in", at: 8000))
        XCTAssertEqual(words.map(\.text), ["the", "of", "and", "in"])
        XCTAssertEqual(words.map(\.ms), [nil, 1000, nil, 1000])
    }

    /// A mark between two words belongs to neither, so the word after it is not timed;
    /// the word before it is.
    func testAMarkBreaksTheTiming() throws {
        let fullStop = try XCTUnwrap(try layouts().orsy?.punctuation.first { $0.text == "." })
        let words = try track(
            try strokes("the", at: 1000) + strokes("of", at: 2000)
                + [stroke(0, fullStop.bits, at: 2500)] + strokes("and", at: 3000))
        XCTAssertEqual(words.map(\.text), ["the", "of", "and"], "a capital is the same word")
        XCTAssertEqual(words.map(\.ms), [nil, 1000, nil])
    }

    /// The space command closes the word before it, so it is part of that word's time.
    func testTheSpaceCommandIsPartOfTheWordItCloses() throws {
        // `the` is closed already, so the space is struck as well, not instead.
        let the = try strokes("the", at: 1000)
        let space = try stroke(0, 0x200, at: 2500)
        XCTAssertEqual(space.outcome, .space)
        let words = try track(the + [space] + strokes("of", at: 3000) + strokes("in", at: 4000))
        XCTAssertEqual(words.map(\.text), ["the", "of", "in"])
        XCTAssertEqual(words.map(\.ms), [nil, 500, 1000])
    }

    /// Enter through the escape ends the record, as the next drill line does, and the
    /// first word after it is not timed.
    func testAKeyThatTypesNothingStartsAgain() throws {
        let layouts = try layouts()
        let enter = try XCTUnwrap(
            layouts.variants["dosh"]?.chords.first {
                $0.action.kind == "key" && $0.action.key == "ReturnEnter"
            }?.code)
        let escape = try stroke(0x380, enter, at: 2500)
        XCTAssertEqual(escape.outcome, .dosh(enter))
        let words = try track(
            try strokes("the", at: 1000) + strokes("of", at: 2000) + [escape]
                + strokes("and", at: 3000) + strokes("in", at: 4000))
        XCTAssertEqual(words.map(\.text), ["the", "of", "and", "in"])
        XCTAssertEqual(words.map(\.ms), [nil, 1000, nil, 1000])
    }

    /// The gate's number is the best the window has been while clean enough, and it
    /// does not get worse: a slow sitting cannot take a word off the ladder.
    func testTheBestMedianIsKeptAndOnlyImproves() {
        var s = WordSamples()
        for _ in 0..<(WordSamples.minUses - 1) { s.note(ms: 1000, clean: true) }
        XCTAssertEqual(s.bestMs, .max, "not until the window is full enough")
        s.note(ms: 1000, clean: true)
        XCTAssertEqual(s.bestMs, 1000)
        for _ in 0..<WordSamples.window { s.note(ms: 3000, clean: true) }
        XCTAssertEqual(s.bestMs, 1000, "slower since, and still counted at its best")
        XCTAssertEqual(SkillModel.median(s.times), 3000)

        // Fast but careless does not count: three slips in twenty is past the bar.
        var careless = WordSamples()
        for i in 0..<WordSamples.window { careless.note(ms: 500, clean: i % 7 != 0) }
        XCTAssertEqual(careless.outcomes.filter { $0 }.count, 3)
        XCTAssertEqual(careless.bestMs, .max)
    }

    /// The fold collects the words into the model, and the cache gives the same answer.
    func testTheModelCarriesTheWords() throws {
        let dir = try logDirectory(try golden("orsy-clean", "log") + "\n")
        let model = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        XCTAssertEqual(model.word("the")?.count, 2)
        XCTAssertEqual(model.word("the")?.medianMs, 90, "the second; the first was untimed")
        XCTAssertEqual(model.word("lazy")?.medianMs, 180)
        XCTAssertEqual(model.word("lazy")?.errorRate, 0)
        XCTAssertNil(model.word("lazy")?.bestMs, "one use is not enough to judge by")
        XCTAssertNil(model.word("cat"))
    }
}
