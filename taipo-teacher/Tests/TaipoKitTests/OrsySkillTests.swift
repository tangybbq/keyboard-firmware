import XCTest

@testable import TaipoKit

/// The Orsy skill model, against logs whose contents are known.
final class OrsySkillTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func golden(_ name: String, _ ext: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/\(name)", withExtension: ext))
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// A log directory holding one file, headed as the collector heads them.
    private func logDirectory(_ contents: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("orsy-skill-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let header = "# session device=test boot_id=0x1 layout=\(try layouts().fingerprint)\n"
        try (header + contents).write(
            to: dir.appendingPathComponent("2026-09-01.txt"), atomically: true, encoding: .utf8)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    /// Every pattern of every syllable is counted, timing is the gap from the stroke
    /// before, and nothing lands in the chord models.
    func testCleanLogCountsPatterns() throws {
        let dir = try logDirectory(try golden("orsy-clean", "log"))
        let model = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        XCTAssertEqual(model.sessions, 1)
        XCTAssertEqual(model.strokes, 13)
        XCTAssertEqual(model.dead, 0)
        // `the` twice: th (FZ) + e ending (ue); `er` uses the vowel too.
        XCTAssertEqual(model.skill("s1:FZ")?.count, 2)
        XCTAssertEqual(model.skill("s3:ue")?.count, 3)
        XCTAssertEqual(model.skill("s1:FZ")?.deleted, 0)
        // Strokes are 90ms apart in the fixture, first key to first key: 60 gap plus
        // 30 hold, measured commit to commit.
        XCTAssertEqual(model.skill("s3:ue")?.medianMs, 90)
        // `quick` uses the cluster rule.
        XCTAssertEqual(model.skill("rule:cluster")?.count, 2, "quick and jumps")
        // Nothing was typed in Dosh.
        var collector = SkillCollector()
        collector.fold(text: try golden("orsy-clean", "log"), layouts: try layouts(),
                       options: SkillModel.Options())
        XCTAssertEqual(collector.model(variant: "dosh", options: SkillModel.Options()).chords, 0)
    }

    /// The drill's line is recorded, so a replay can tell a wrong stroke from a right
    /// one without waiting for the writer to take it back.
    func testTheLineTheDrillAskedFor() throws {
        let clean = try golden("orsy-clean", "log")
        let asked = KeyLogSession.orsyLineMarker + "the quick brown fox jumps over the lazy dog\n"

        // Typed correctly: recording what was asked for changes nothing.
        let plain = OrsySkillModel.build(
            logDirectory: try logDirectory(clean), layouts: try layouts())
        let judged = OrsySkillModel.build(
            logDirectory: try logDirectory(asked + clean), layouts: try layouts())
        XCTAssertEqual(judged.skill("s1:FZ")?.count, plain.skill("s1:FZ")?.count)
        XCTAssertEqual(judged.skill("s3:ue")?.count, plain.skill("s3:ue")?.count)
        XCTAssertEqual(judged.strokes, plain.strokes, "every stroke is still seen")

        // Asked for something else entirely: none of it is practice of what it spelled,
        // though it was never taken back and nothing else could have revealed it.
        let wrong = KeyLogSession.orsyLineMarker + "set ret res ris\n"
        let missed = OrsySkillModel.build(
            logDirectory: try logDirectory(wrong + clean), layouts: try layouts())
        XCTAssertEqual(missed.strokes, plain.strokes, "the strokes are still counted")
        XCTAssertNil(missed.skill("s1:FZ"), "but not as uses of the th it happened to spell")
        XCTAssertNil(missed.skill("s3:ue"))
        XCTAssertEqual(missed.strokeUses(left: 0x42, right: 0x208), 0, "nor as strokes made")
    }

    /// A stroke is counted in its own right, not only as the patterns in it.
    ///
    /// The hint asks whether a stroke has been made before, and every pattern in one is
    /// used at least as often as the stroke itself, so the two counts have to be kept
    /// apart: `the` is struck twice in the fixture while its closing `e` is struck three
    /// times, once more in `er`.
    func testStrokesAreCountedInTheirOwnRight() throws {
        let dir = try logDirectory(try golden("orsy-clean", "log"))
        let model = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        // `the` is FZ + ue: left 0x42, right 0x208.
        XCTAssertEqual(model.strokeUses(left: 0x42, right: 0x208), 2)
        XCTAssertEqual(model.skill("s3:ue")?.count, 3, "the pattern is used once more")
        // A stroke never made is new, and asking costs nothing.
        XCTAssertEqual(model.strokeUses(left: 0x3ff, right: 0x3ff), 0)
    }

    /// The reset marker disowns the Orsy practice before it and nothing else: the same
    /// log twice with a marker between counts as one pass, and the chord models are
    /// untouched.
    func testOrsyResetDiscardsWhatCameBefore() throws {
        let clean = try golden("orsy-clean", "log")
        let once = try logDirectory(clean)
        let twiceWithReset = try logDirectory(
            clean + KeyLogSession.orsyResetMarker + " 1789000000 (unix seconds)\n" + clean)
        let expected = OrsySkillModel.build(logDirectory: once, layouts: try layouts())
        let got = OrsySkillModel.build(logDirectory: twiceWithReset, layouts: try layouts())
        XCTAssertEqual(got.strokes, expected.strokes)
        XCTAssertEqual(got.skill("s1:FZ")?.count, expected.skill("s1:FZ")?.count)
        XCTAssertEqual(got.skills.count, expected.skills.count)

        // Without the marker the same file counts twice, which is what the marker undoes.
        let twice = try logDirectory(clean + clean)
        let both = OrsySkillModel.build(logDirectory: twice, layouts: try layouts())
        XCTAssertEqual(both.strokes, expected.strokes * 2)

        // And the chords are not disowned by it: both halves are still read.
        let header = "# session device=test boot_id=0x1 layout=\(try layouts().fingerprint)\n"
        var collector = SkillCollector()
        collector.fold(
            text: header + clean + KeyLogSession.orsyResetMarker + "\n" + clean,
            layouts: try layouts(), options: SkillModel.Options())
        XCTAssertEqual(collector.sessions, 2)
        XCTAssertEqual(collector.orsy.strokes, expected.strokes)
    }

    /// Blame goes to the pattern that was wanted and missed, not to the one that turned
    /// up in its place.
    func testBlame() throws {
        let dir = try logDirectory(try golden("orsy-sloppy", "log"))
        let model = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        XCTAssertEqual(model.dead, 1)

        // `mais` for `main`, backspaced away: only the coda was wrong, so the coda that
        // was wanted answers for it.
        XCTAssertEqual(model.skill("s4:N")?.deleted, 1, "n was reached for and missed")
        XCTAssertEqual(model.skill("s4:S")?.deleted, 0, "s was never intended")
        XCTAssertEqual(model.skill("s4:S")?.count, 1, "and the misfire is not a use of it")
        // The Series that matched are left alone: the m and the nucleus were struck right.
        XCTAssertEqual(model.skill("s1:SZP")?.deleted, 0)
        XCTAssertEqual(model.skill("s1:SZP")?.count, 2)
        XCTAssertEqual(model.skill("s3:i")?.deleted, 0)

        // `pain` typed where `s` was meant and undone: nothing in it was right, so none
        // of it counts, and the `s` that was wanted carries the error.
        XCTAssertEqual(model.skill("s1:S")?.deleted, 1)
        XCTAssertEqual(model.skill("s1:P")?.deleted, 0, "p was not what was wanted")
        XCTAssertEqual(model.skill("s3:ui")?.deleted, 0)
        XCTAssertEqual(model.skill("s4:N")?.count, 6, "pain's n is not a use of it")

        // Nor is a stroke nobody meant evidence that it has ever been made.
        XCTAssertEqual(model.strokeUses(left: 0x122, right: 0x104), 0, "mais")

        // The commands are counted as ever.
        XCTAssertEqual(model.skill("cmd:undo")?.count, 1)
        XCTAssertEqual(model.skill("cmd:dosh_oneshot")?.count, 5)
    }

    /// The gate and the parts behave as the chord model's do.
    func testReachedAndParts() throws {
        let options = SkillModel.Options(minSamples: 4, targetMs: 500, maxErrorRate: 0.1)
        let model = OrsySkillModel(
            skills: [
                "s1:FP": PatternSkill(name: "s1:FP", count: 10, medianMs: 300, deleted: 0),
                "s1:CP": PatternSkill(name: "s1:CP", count: 2, medianMs: 300, deleted: 0),
                "s1:P": PatternSkill(name: "s1:P", count: 10, medianMs: 900, deleted: 3),
            ], sessions: 1, strokes: 22, options: options)
        XCTAssertTrue(model.reached("s1:FP"))
        XCTAssertTrue(model.learned("s1:FP"))
        XCTAssertFalse(model.reached("s1:CP"), "too few uses")
        XCTAssertFalse(model.reached("s1:P"), "too often taken back")
        XCTAssertEqual(model.parts("s1:CP").exposure, 0.5)
        XCTAssertLessThan(model.parts("s1:P").speed, 1)
        XCTAssertEqual(model.parts("s1:none").confidence, 0)
    }

    /// The checkpoint gives the same answer as a fresh build, and an Orsy table change
    /// throws it away.
    func testStoreAgreesWithBuild() throws {
        let dir = try logDirectory(try golden("orsy-sloppy", "log"))
        // A second file, so that the first is settled and gets folded into the cache.
        try (try golden("orsy-clean", "log")).write(
            to: dir.appendingPathComponent("2026-09-02.txt"), atomically: true, encoding: .utf8)
        let cache = dir.appendingPathComponent("cache.json")
        let fresh = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        let stored = SkillStore.orsyModel(logDirectory: dir, cache: cache, layouts: try layouts())
        let again = SkillStore.orsyModel(logDirectory: dir, cache: cache, layouts: try layouts())
        XCTAssertEqual(stored.skills, fresh.skills)
        XCTAssertEqual(again.skills, fresh.skills)
        XCTAssertEqual(stored.strokes, fresh.strokes)
        let contents = try XCTUnwrap(SkillStore.load(cache))
        XCTAssertEqual(contents.orsyFingerprint, try layouts().orsy?.fingerprint)
        XCTAssertEqual(contents.folded.count, 1)
    }

    /// Days are numbered from the file's name, the same in any time zone.
    func testLogDay() {
        XCTAssertEqual(LogDay.number("1970-01-01"), 0)
        XCTAssertEqual(LogDay.number("2026-09-25"), 20721)
        XCTAssertEqual(LogDay.number("2026-03-01")! - LogDay.number("2026-02-28")!, 1)
        XCTAssertEqual(LogDay.number("2024-03-01")! - LogDay.number("2024-02-28")!, 2)
        XCTAssertNil(LogDay.number("notes"))
        XCTAssertEqual(
            LogDay.number(of: URL(fileURLWithPath: "/logs/2026-09-25.txt")), 20721)
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!
        XCTAssertEqual(LogDay.today(Date(timeIntervalSince1970: 86400 * 3 + 5), calendar: utc), 3)
    }

    /// Three clean looks on the day it is due move an item up; a look before then earns
    /// nothing; a slip sends it back to the first box.
    func testRetentionBoxes() {
        func look(_ r: inout Retention, day: Int, gaps: [UInt32?], bad: Int? = nil) {
            for (i, gap) in gaps.enumerated() {
                r.note(day: day, gap: gap, bad: i == bad) { 1000 }
            }
        }
        var r = Retention(reached: 10)
        XCTAssertFalse(r.due(on: 10), "not the day it was reached")
        XCTAssertTrue(r.due(on: 11))
        look(&r, day: 10, gaps: [900, 900, 900])
        XCTAssertEqual(r.box, 0, "the day it was reached is not a look")

        look(&r, day: 11, gaps: [1200, 1500, 1100])
        XCTAssertEqual(r.box, 1)
        XCTAssertEqual(r.since, 11)
        XCTAssertFalse(r.due(on: 12))
        XCTAssertTrue(r.due(on: 13))

        // Early: clean, but only a day into a two-day wait.
        look(&r, day: 12, gaps: [900, 900, 900])
        XCTAssertEqual(r.box, 1)
        XCTAssertEqual(r.since, 11)

        // Later uses the same day are warm and change nothing, even a slip.
        look(&r, day: 13, gaps: [900, 900, 900, 900], bad: 3)
        XCTAssertEqual(r.box, 2)
        XCTAssertEqual(r.lapses, 0)

        // A slip among the first three is a lapse, back to the first box.
        look(&r, day: 17, gaps: [900, 900, 900], bad: 1)
        XCTAssertEqual(r.box, 0)
        XCTAssertEqual(r.since, 17)
        XCTAssertTrue(r.lapsed)
        XCTAssertEqual(r.lapses, 1)
        XCTAssertTrue(r.due(on: 18))

        // More than twice the warmed-up median is a lapse too; missing timings are not.
        var slow = Retention(reached: 0)
        look(&slow, day: 1, gaps: [2500, 2100, nil])
        XCTAssertTrue(slow.lapsed)
        var untimed = Retention(reached: 0)
        look(&untimed, day: 1, gaps: [nil, nil, 1900])
        XCTAssertEqual(untimed.box, 1)
    }

    /// A day with fewer looks than a full set is graded on what it had once it is over,
    /// and is still due while it is going on.
    func testRetentionPartialDay() {
        var r = Retention(reached: 0)
        r.note(day: 1, gap: 900, bad: false) { 1000 }
        XCTAssertTrue(r.due(on: 1), "one look of three: still due today")
        XCTAssertEqual(r.box, 0)
        XCTAssertEqual(r.settled(on: 2).box, 1, "graded once the day is over")
        XCTAssertFalse(r.due(on: 2))
        // And the next day's use settles it for good.
        r.note(day: 2, gap: 900, bad: false) { 1000 }
        XCTAssertEqual(r.box, 1)
        XCTAssertEqual(r.since, 1)
    }

    /// The fold gives a pattern its box on the day it is reached and grades it from the
    /// next day's file, and the checkpoint agrees.
    func testRetentionFromLogs() throws {
        let clean = try golden("orsy-clean", "log")
        let dir = try logDirectory(clean)
        let header = "# session device=test boot_id=0x1 layout=\(try layouts().fingerprint)\n"
        try (header + clean).write(
            to: dir.appendingPathComponent("2026-09-02.txt"), atomically: true, encoding: .utf8)
        let options = SkillModel.Options(minSamples: 1, pauseMs: 4000)
        let model = OrsySkillModel.build(
            logDirectory: dir, layouts: try layouts(), options: options)
        let day1 = try XCTUnwrap(LogDay.number("2026-09-01"))
        // `e` ending is struck three times a file, at the same pace both days.
        let r = try XCTUnwrap(model.skill("s3:ue")?.retention)
        XCTAssertEqual(r.box, 1)
        XCTAssertEqual(r.since, day1 + 1)
        XCTAssertFalse(model.due("s3:ue", on: day1 + 2))
        XCTAssertTrue(model.due("s3:ue", on: day1 + 3))
        let unreviewed = model.skills.filter { $0.key.hasPrefix("stroke:") || $0.key.hasPrefix("cmd:") }
        XCTAssertFalse(unreviewed.isEmpty)
        XCTAssertTrue(unreviewed.values.allSatisfy { $0.retention == nil }, "strokes and commands are not reviewed")
        XCTAssertFalse(model.due("s1:none", on: day1 + 3))

        let cache = dir.appendingPathComponent("cache.json")
        let stored = SkillStore.orsyModel(
            logDirectory: dir, cache: cache, layouts: try layouts(), options: options)
        XCTAssertEqual(stored.skills, model.skills)
    }

    /// The signatures this writes agree with what the sync script recorded for the
    /// tables in hand.
    func testSignaturesMatchTheRecordedRevision() throws {
        let layouts = try layouts()
        let orsy = try XCTUnwrap(layouts.orsy)
        let history = LayoutHistory.bundled()
        XCTAssertEqual(history.changedPatterns(since: layouts.fingerprintValue, layouts: layouts), [])
        XCTAssertNil(history.changedPatterns(since: 0x1234, layouts: layouts))
        let recorded = try XCTUnwrap(
            history.recordedOrsy(fingerprint: orsy.fingerprint),
            "run taipo-teacher/scripts/sync-layouts.py")
        XCTAssertEqual(recorded, LayoutHistory.orsySignatures(orsy))
    }
}
