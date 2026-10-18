import XCTest

@testable import TaipoKit

/// The word ladder: what is out, what is worked on, and the lines it makes.
final class OrsyWordLadderTests: XCTestCase {
    private func words() throws -> OrsyWords { try OrsyWords.bundled() }

    /// A model in which each word has the best and current medians given, from a full
    /// clean window.
    private func model(_ times: [String: (best: UInt32, now: UInt32)]) -> OrsySkillModel {
        var skills = [String: WordSkill]()
        for (text, t) in times {
            skills[text] = WordSkill(
                text: text, count: 40, medianMs: t.now, errorRate: 0, bestMs: t.best)
        }
        return OrsySkillModel(skills: [:], sessions: 1, strokes: 100, words: skills)
    }

    /// The time a word is allowed is the time per stroke for each of its strokes, so a
    /// two-stroke word gets twice what a one-stroke word the same length does.
    func testTargets() throws {
        let words = try words()
        func target(_ text: String) throws -> UInt32 {
            OrsyWordLadder.targetMs(try XCTUnwrap(words.word(text)), strokeMs: 1500)
        }
        XCTAssertEqual(try target("the"), 1500)
        XCTAssertEqual(try target("all"), 3000)
        XCTAssertEqual(try target("don't"), 4500, "don, the apostrophe, t")
        XCTAssertEqual(OrsyWordLadder.equivalentWpm(strokeMs: 1500), 25.8, accuracy: 0.1)

        // `all` at 2279ms is 1140ms a stroke, and passes where `the` at 1600 does not.
        let ladder = OrsyWordLadder(
            words: words, skill: model(["all": (2279, 2279), "the": (1600, 1600)]))
        XCTAssertEqual(ladder.item("all")?.passed, true)
        XCTAssertEqual(ladder.item("all")?.strokeMs, 1139)
        XCTAssertEqual(ladder.item("the")?.passed, false)
    }

    /// Before anything is typed, the first words are out and the first of them are the
    /// focus.
    func testStartsWithTheFirstWords() throws {
        let ladder = OrsyWordLadder(
            words: try words(), skill: OrsySkillModel(skills: [:], sessions: 0, strokes: 0))
        XCTAssertEqual(ladder.unlockedCount, 10)
        XCTAssertEqual(ladder.focus.map(\.text), ["the", "to", "and", "you"])
        XCTAssertEqual(ladder.next?.text, ladder.words[10].text)
        XCTAssertEqual(ladder.below, 10)
    }

    /// Once the first words have passed, the next ones come out, up to three below the
    /// target at once, and those are the focus, with the slowest word already passed
    /// for polishing.
    func testUnlocksAsWordsPass() throws {
        let words = try words()
        let first = words.ladder.prefix(10).map(\.text)
        var times = [String: (best: UInt32, now: UInt32)]()
        for text in first { times[text] = (500, 500) }
        // The slowest of them now, though it passed.
        times[first[3]] = (500, 3000)
        let ladder = OrsyWordLadder(words: words, skill: model(times))
        XCTAssertEqual(ladder.unlockedCount, 13)
        XCTAssertEqual(ladder.below, 3)
        let newest = words.ladder[10..<13].map(\.text)
        XCTAssertEqual(Set(ladder.focus.prefix(3).map(\.text)), Set(newest))
        XCTAssertEqual(ladder.focus.last?.text, first[3])
    }

    /// Passing is the best the word has been, so a slow sitting since does not take it
    /// back; a faster target does.
    func testPassingIsStickyButFollowsTheTarget() throws {
        let words = try words()
        var times = [String: (best: UInt32, now: UInt32)]()
        for text in words.ladder.prefix(10).map(\.text) { times[text] = (900, 4000) }
        let slowNow = OrsyWordLadder(words: words, skill: model(times))
        XCTAssertEqual(slowNow.unlockedCount, 13, "still passed at 1500ms a stroke")
        XCTAssertEqual(slowNow.item("the")?.passed, true)

        let faster = OrsyWordLadder(
            words: words, skill: model(times), options: OrsyWordLadder.Options(strokeMs: 800))
        // The first ten are one stroke each, so each has 800ms, and 900 is not under it.
        XCTAssertEqual(faster.item("the")?.passed, false)
        XCTAssertEqual(faster.unlockedCount, 10)
    }

    /// A line is unlocked words only, every focus word in it, no word twice running, the
    /// opener not a focus word, and short enough for the screen.
    func testLines() throws {
        let words = try words()
        var times = [String: (best: UInt32, now: UInt32)]()
        for text in words.ladder.prefix(40).map(\.text) { times[text] = (500, 500) }
        let ladder = OrsyWordLadder(words: words, skill: model(times))
        let unlocked = Set(ladder.unlocked.map(\.text))
        let focus = ladder.focus.map(\.text)
        let drill = OrsyWordLadderMaker().drill(ladder, lines: 50, seed: 7)
        XCTAssertEqual(drill.lines.count, 50)
        for line in drill.lines {
            let tokens = line.split(separator: " ").map(String.init)
            XCTAssertTrue(tokens.allSatisfy(unlocked.contains), line)
            XCTAssertTrue(focus.allSatisfy(tokens.contains), line)
            XCTAssertFalse(focus.contains(tokens[0]), line)
            XCTAssertFalse(zip(tokens, tokens.dropFirst()).contains { $0 == $1 }, line)
            XCTAssertLessThanOrEqual(line.count, OrsyWordLadderMaker.maxCharacters, line)
        }
        XCTAssertGreaterThan(Set(drill.lines).count, 40, "the lines vary")
    }
}
