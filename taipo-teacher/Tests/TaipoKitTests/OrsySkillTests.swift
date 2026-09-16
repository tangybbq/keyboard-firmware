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

    /// Blame goes to the Series that differ from the retype, or to the whole stroke.
    func testBlame() throws {
        let dir = try logDirectory(try golden("orsy-sloppy", "log"))
        let model = OrsySkillModel.build(logDirectory: dir, layouts: try layouts())
        XCTAssertEqual(model.dead, 1)
        // `mais` for `main`, backspaced away: only the coda was wrong.
        XCTAssertEqual(model.skill("s4:S")?.deleted, 1)
        XCTAssertEqual(model.skill("s1:SZP")?.deleted, 0, "the m of mais was right")
        XCTAssertEqual(model.skill("s3:i")?.deleted, 0)
        // `pain` typed where `s` was meant, and undone: nothing in it was right, its
        // coda included.
        XCTAssertEqual(model.skill("s1:P")?.deleted, 1)
        XCTAssertEqual(model.skill("s3:ui")?.deleted, 1)
        XCTAssertEqual(model.skill("s4:N")?.deleted, 1)
        XCTAssertEqual(model.skill("s4:N")?.count, 7)
        // The commands are counted too.
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
