import XCTest

@testable import TaipoKit

/// The skill model, against a log whose contents are known.
final class SkillTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func golden(_ name: String, _ ext: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/\(name)", withExtension: ext))
        return try String(contentsOf: url, encoding: .utf8)
    }

    private func logDirectory(_ contents: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("skill-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try (sessionHeader(try Layouts.bundled()) + contents).write(
            to: dir.appendingPathComponent("2026-09-01.txt"), atomically: true, encoding: .utf8)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    /// A log of `count` presses of one key, `gapMs` apart.
    ///
    /// Written out rather than generated through the engine so the expected timing is
    /// arithmetic rather than a second implementation of the thing under test.
    private func repeatedChord(key: String, count: Int, gapMs: UInt32, holdMs: UInt32 = 30)
        -> String
    {
        var lines = [String]()
        for i in 0..<count {
            let at = UInt32(i) * gapMs
            lines.append("\(at) + \(key)")
            lines.append("\(at + holdMs) - \(key)")
        }
        return lines.joined(separator: "\n") + "\n"
    }

    /// Counts, timing and the gap rule, on a log built to have one answer.
    ///
    /// The chord commits when its last key comes up, so the gap between two of them is the
    /// gap between the releases, which is the gap between the presses.
    func testCountsAndTiming() throws {
        let dir = try logDirectory(repeatedChord(key: "L.e", count: 10, gapMs: 400))
        let model = SkillModel.build(logDirectory: dir, layouts: try layouts())

        let e = try XCTUnwrap(model.skill(0x008))
        XCTAssertEqual(e.count, 10)
        // Nine gaps of 400ms; the first chord has nothing before it.
        XCTAssertEqual(e.medianMs, 400)
        XCTAssertEqual(e.deleted, 0)
        XCTAssertEqual(model.chords, 10)
    }

    /// A pause is not slowness.  Gaps past the window are dropped rather than averaged in,
    /// so stopping to think costs nothing.
    func testLongGapsAreNotSlowness() throws {
        var log = repeatedChord(key: "L.e", count: 6, gapMs: 300)
        // One more, ten seconds after the last: the writer went for coffee.
        log += "11500 + L.e\n11530 - L.e\n"
        let dir = try logDirectory(log)
        let model = SkillModel.build(logDirectory: dir, layouts: try layouts())

        let e = try XCTUnwrap(model.skill(0x008))
        XCTAssertEqual(e.count, 7)
        XCTAssertEqual(e.medianMs, 300, "the pause should not count as a slow chord")
    }

    /// A chord seen but never timed is not the fastest thing on the keyboard.
    func testAnUntimedChordIsNotFast() throws {
        let dir = try logDirectory(repeatedChord(key: "L.e", count: 1, gapMs: 400))
        let model = SkillModel.build(logDirectory: dir, layouts: try layouts())

        let e = try XCTUnwrap(model.skill(0x008))
        XCTAssertEqual(e.count, 1)
        XCTAssertEqual(e.medianMs, .max)
        XCTAssertFalse(model.learned(0x008))
    }

    /// The thresholds: enough times, fast enough, and not often taken back.
    func testLearnedNeedsAllThree() throws {
        let options = SkillModel.Options(minSamples: 5, targetMs: 500, maxErrorRate: 0.1)
        func model(count: Int, ms: UInt32, deleted: Int) -> SkillModel {
            SkillModel(
                skills: [
                    0x008: ChordSkill(code: 0x008, count: count, medianMs: ms, deleted: deleted)
                ], sessions: 1, chords: count, options: options)
        }

        XCTAssertTrue(model(count: 20, ms: 300, deleted: 0).learned(0x008))
        XCTAssertFalse(model(count: 3, ms: 300, deleted: 0).learned(0x008), "too few")
        XCTAssertFalse(model(count: 20, ms: 900, deleted: 0).learned(0x008), "too slow")
        XCTAssertFalse(model(count: 20, ms: 300, deleted: 5).learned(0x008), "too wrong")
    }

    /// Confidence orders the weakest first, and a chord never typed is weakest of all.
    func testConfidenceOrdersTheWeakest() throws {
        let options = SkillModel.Options(minSamples: 10, targetMs: 500, maxErrorRate: 0.2)
        let model = SkillModel(
            skills: [
                0x001: ChordSkill(code: 0x001, count: 40, medianMs: 300, deleted: 0),
                0x002: ChordSkill(code: 0x002, count: 40, medianMs: 1200, deleted: 0),
                0x004: ChordSkill(code: 0x004, count: 3, medianMs: 300, deleted: 0),
            ], sessions: 1, chords: 83, options: options)

        let ranked = [0x001, 0x002, 0x004, 0x008].sorted {
            model.confidence(UInt16($0)) < model.confidence(UInt16($1))
        }
        XCTAssertEqual(ranked.first, 0x008, "a chord never typed is the least known")
        XCTAssertEqual(ranked.last, 0x001, "fast, accurate and often typed is the best known")
        XCTAssertEqual(model.confidence(0x008), 0)
        XCTAssertEqual(model.confidence(0x001), 1, accuracy: 0.0001)
    }

    /// The three parts multiply to the confidence, and each says what it is about.
    ///
    /// Worth pinning together: the parts exist so a screen can say what is holding a chord
    /// back, and they would be a lie if they did not agree with the number the ladder
    /// ranks by.
    func testPartsExplainTheConfidence() throws {
        let options = SkillModel.Options(minSamples: 10, targetMs: 500, maxErrorRate: 0.2)
        func model(count: Int, ms: UInt32, deleted: Int) -> SkillModel {
            SkillModel(
                skills: [
                    0x008: ChordSkill(code: 0x008, count: count, medianMs: ms, deleted: deleted)
                ], sessions: 1, chords: count, options: options)
        }

        // Hardly typed, but quick and clean when it was.
        let new = model(count: 2, ms: 300, deleted: 0)
        XCTAssertEqual(new.parts(0x008).exposure, 0.2, accuracy: 0.0001)
        XCTAssertEqual(new.parts(0x008).speed, 1)
        XCTAssertEqual(new.parts(0x008).accuracy, 1)
        XCTAssertFalse(new.parts(0x008).complete)

        // Typed plenty and never taken back, but slow.
        let slow = model(count: 40, ms: 1000, deleted: 0)
        XCTAssertEqual(slow.parts(0x008).exposure, 1)
        XCTAssertEqual(slow.parts(0x008).speed, 0.5, accuracy: 0.0001)
        XCTAssertFalse(slow.parts(0x008).complete)

        // Corrections inside the allowance are not a shortfall.  This is the case the
        // first cut got wrong: accuracy counted down from a clean sheet, so a chord with
        // any correction at all read as short on the screen while the ladder had already
        // finished with it.
        let tidy = model(count: 40, ms: 300, deleted: 4)  // a tenth, which is the allowance
        XCTAssertEqual(tidy.parts(0x008).accuracy, 1)
        XCTAssertTrue(tidy.parts(0x008).complete)
        XCTAssertTrue(tidy.learned(0x008))

        let messy = model(count: 40, ms: 300, deleted: 16)  // twice the allowance
        XCTAssertEqual(messy.parts(0x008).accuracy, 0.5, accuracy: 0.0001)
        XCTAssertFalse(messy.learned(0x008))

        // Every part agrees with the number the ladder ranks by, and with `learned`.
        for m in [new, slow, tidy, messy, model(count: 40, ms: 300, deleted: 0)] {
            XCTAssertEqual(m.parts(0x008).confidence, m.confidence(0x008), accuracy: 0.0001)
            XCTAssertEqual(m.parts(0x008).complete, m.learned(0x008))
        }

        // Never typed at all: nothing to show on any of the three.
        XCTAssertEqual(new.parts(0x001), SkillModel.Parts(exposure: 0, speed: 0, accuracy: 0))
    }

    /// Practice buys patience at the gate, but only so much of it.
    ///
    /// A chord being drilled is measured in the writer's worst context, while the bar is
    /// set by chords they meet in ordinary work -- so a hard one can sit just above it for
    /// as long as it is being worked on.  The apostrophe and `b` both plateaued at 11%
    /// against a bar of 10% and never once dipped under, over ninety-six and sixty-three
    /// uses.  Neither was running ahead of the writer at 89% right.
    func testExposureBuysPatienceAtTheGate() throws {
        let options = SkillModel.Options(minSamples: 12, maxErrorRate: 0.1, patience: 60)

        // Just above the bar, and not yet much practice: still short of the gate.
        XCTAssertEqual(options.allowedErrorRate(after: 20), 0.1)
        // The same rate with the practice behind it is through.
        XCTAssertEqual(options.allowedErrorRate(after: 96), 0.2)

        /// A chord taken back every `every` uses, starting with the first, so that the
        /// opening window is already over the bar and the rate never dips under it.  That
        /// is the shape the apostrophe had: a flat 11%, and not one moment below 10% in
        /// ninety-six uses.
        func model(uses: Int, undoneEvery every: Int, _ options: SkillModel.Options)
            -> SkillModel
        {
            var samples = ChordSamples()
            for i in 0..<uses {
                samples.note(gap: 300, deleted: i % every == 0, options: options)
            }
            var collector = SkillCollector()
            collector.samples = ["taipo": [0x008: samples]]
            collector.sessions = 1
            return collector.model(variant: "taipo", options: options)
        }

        // Every ninth taken back is 11%, just over the bar.  Thirty uses in, that is still
        // a chord the writer has not shown they can do.
        XCTAssertFalse(model(uses: 30, undoneEvery: 9, options).reached(0x008))
        // At ninety-six, the same rate is through -- on the practice, not the accuracy.
        XCTAssertTrue(model(uses: 96, undoneEvery: 9, options).reached(0x008))

        // And without the patience there is no way through at all, however long it goes
        // on.  This is the case that was stuck.
        let strict = SkillModel.Options(
            minSamples: 12, maxErrorRate: 0.1, patience: .max)
        XCTAssertFalse(model(uses: 400, undoneEvery: 9, strict).reached(0x008))

        // Patience is not indulgence: a third taken back is short of the gate whatever the
        // practice behind it.
        XCTAssertFalse(model(uses: 400, undoneEvery: 3, options).reached(0x008))
    }

    /// Corrections are charged to the chord the backspace took back.
    func testDeletionsAreCounted() throws {
        let dir = try logDirectory(try golden("slips", "log"))
        let model = SkillModel.build(logDirectory: dir, layouts: try layouts())

        // The golden log's confusions are the same corrections seen from the other end,
        // so every pair's chords must show deletions here.
        let confusions = ConfusionModel.build(logDirectory: dir, layouts: try layouts())
        let blamed = model.skills.values.filter { $0.deleted > 0 }
        XCTAssertFalse(blamed.isEmpty)
        XCTAssertEqual(blamed.map(\.deleted).reduce(0, +), confusions.corrections)
    }

    /// A session recorded against other tables is not replayed against these ones.
    ///
    /// This is the case that made the rule necessary rather than tidy: when Dosh's letters
    /// moved, the chord that had been typing `a` began typing `s`.  Replaying the old log
    /// against the new table credits every one of those uses to `s`, and nothing about the
    /// result looks wrong.
    func testSessionsFromOtherTablesAreSkipped() throws {
        let layouts = try layouts()
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("stale-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }

        let body = repeatedChord(key: "L.e", count: 10, gapMs: 400)
        func write(_ name: String, _ header: String) throws {
            try (header + body).write(
                to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
        }
        try write("2026-09-01.txt", "# session device=t boot_id=0x1 layout=0xdeadbeefdeadbeef\n")
        try write("2026-09-02.txt", "# session device=t boot_id=0x2 layout=unknown\n")
        try write("2026-09-03.txt", sessionHeader(layouts))

        let model = SkillModel.build(logDirectory: dir, layouts: layouts)

        XCTAssertEqual(model.sessions, 1, "only the one on these tables")
        XCTAssertEqual(model.skipped, 2, "and the others are counted, not lost silently")
        XCTAssertEqual(try XCTUnwrap(model.skill(0x008)).count, 10)
    }

    /// The model is per-variant, like everything else the trainer derives.
    func testModelIsPerVariant() throws {
        let dir = try logDirectory(repeatedChord(key: "L.e", count: 10, gapMs: 400))
        let dosh = SkillModel.build(logDirectory: dir, layouts: try layouts(), variant: "dosh")

        XCTAssertEqual(dosh.chords, 0)
        XCTAssertNil(dosh.skill(0x008))
    }
}
