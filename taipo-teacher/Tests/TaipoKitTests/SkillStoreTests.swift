import XCTest

@testable import TaipoKit

/// The checkpoint, and the one property that makes it a cache rather than a second source
/// of truth: it has to give the same answer as reading everything.
final class SkillStoreTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private var dir: URL!
    private var cache: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("store-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        cache = dir.appendingPathComponent("cache.json")
        addTeardownBlock { [dir] in try? FileManager.default.removeItem(at: dir!) }
    }

    /// A day of typing: `count` presses of the given keys in turn, `gapMs` apart.
    private func day(_ name: String, keys: [String], count: Int, gapMs: UInt32) throws {
        var out = ""
        var t: UInt32 = 0
        for i in 0..<count {
            let k = keys[i % keys.count]
            out += "\(t) + \(k)\n\(t + 30) - \(k)\n"
            t += gapMs
        }
        try out.write(
            to: dir.appendingPathComponent(name), atomically: true, encoding: .utf8)
    }

    private func append(_ name: String, keys: [String], count: Int, gapMs: UInt32, from: UInt32)
        throws
    {
        let url = dir.appendingPathComponent(name)
        var out = try String(contentsOf: url, encoding: .utf8)
        var t = from
        for i in 0..<count {
            let k = keys[i % keys.count]
            out += "\(t) + \(k)\n\(t + 30) - \(k)\n"
            t += gapMs
        }
        try out.write(to: url, atomically: true, encoding: .utf8)
    }

    /// Every measurement, so a comparison cannot pass by looking at the easy half.
    private func snapshot(_ model: SkillModel) -> [UInt16: ChordSkill] { model.skills }

    private func fromScratch(_ variant: String = "taipo") throws -> SkillModel {
        SkillModel.build(logDirectory: dir, layouts: try layouts(), variant: variant)
    }

    private func cached(_ variant: String = "taipo") throws -> SkillModel {
        SkillStore.model(
            logDirectory: dir, cache: cache, layouts: try layouts(), variant: variant)
    }

    // MARK: - The property

    /// Folding a file at a time gives the same answer as folding everything, at every
    /// point along the way.  This is the whole justification for the cache existing.
    func testIncrementalMatchesFromScratch() throws {
        let keys = ["L.e", "R.t", "L.a", "R.o", "L.i", "R.n", "L.s"]
        for day in 1...5 {
            try self.day(
                String(format: "2026-09-%02d.txt", day), keys: keys, count: 200,
                gapMs: UInt32(250 + day * 40))
            // Rebuilt through the cache after every new day, as the app would.
            XCTAssertEqual(
                snapshot(try cached()), snapshot(try fromScratch()), "after day \(day)")
        }
    }

    /// And deleting the checkpoint loses nothing.
    func testDeletingTheCacheLosesNothing() throws {
        let keys = ["L.e", "R.t", "L.a", "R.o"]
        for day in 1...4 {
            try self.day(
                String(format: "2026-09-%02d.txt", day), keys: keys, count: 150, gapMs: 300)
        }
        let warm = snapshot(try cached())
        try FileManager.default.removeItem(at: cache)
        XCTAssertEqual(snapshot(try cached()), warm)
    }

    /// The newest file is never folded, because it is still being written to.  Appending
    /// to it shows up straight away, with the checkpoint left in place.
    func testTheNewestFileIsAlwaysReread() throws {
        let keys = ["L.e", "R.t"]
        try day("2026-09-01.txt", keys: keys, count: 100, gapMs: 300)
        try day("2026-09-02.txt", keys: keys, count: 100, gapMs: 300)
        let before = try XCTUnwrap(try cached().skill(0x008)).count

        try append("2026-09-02.txt", keys: keys, count: 50, gapMs: 300, from: 100_000)
        let after = try XCTUnwrap(try cached().skill(0x008)).count

        XCTAssertEqual(after, before + 25)
        XCTAssertEqual(snapshot(try cached()), snapshot(try fromScratch()))
    }

    /// Only the settled files are kept, and all of them are.
    func testEverythingButTheNewestIsFolded() throws {
        let keys = ["L.e", "R.t"]
        for day in 1...4 {
            try self.day(
                String(format: "2026-09-%02d.txt", day), keys: keys, count: 100, gapMs: 300)
        }
        _ = try cached()

        let contents = try XCTUnwrap(SkillStore.load(cache))
        XCTAssertEqual(contents.folded.map(\.name), [
            "2026-09-01.txt", "2026-09-02.txt", "2026-09-03.txt",
        ])
    }

    /// A settled file that changes -- a scrub reaching back, or a file removed -- throws
    /// the checkpoint away rather than folding the change on top of what came after it.
    func testAChangedFileStartsAgain() throws {
        let keys = ["L.e", "R.t", "L.a"]
        for day in 1...4 {
            try self.day(
                String(format: "2026-09-%02d.txt", day), keys: keys, count: 150, gapMs: 300)
        }
        _ = try cached()

        // Day two loses most of its contents, as a scrub would have left it.
        try day("2026-09-02.txt", keys: keys, count: 10, gapMs: 300)
        XCTAssertEqual(snapshot(try cached()), snapshot(try fromScratch()))

        // And a settled file going away entirely.
        try FileManager.default.removeItem(at: dir.appendingPathComponent("2026-09-03.txt"))
        XCTAssertEqual(snapshot(try cached()), snapshot(try fromScratch()))
    }

    /// A cache written with a different window is not read back: its gaps are the wrong
    /// length, and a wrong cache is worse than no cache.
    func testAChangedWindowStartsAgain() throws {
        let keys = ["L.e", "R.t"]
        for day in 1...3 {
            try self.day(
                String(format: "2026-09-%02d.txt", day), keys: keys, count: 200, gapMs: 300)
        }
        _ = try cached()

        let narrow = SkillModel.Options(window: 8)
        let model = SkillStore.model(
            logDirectory: dir, cache: cache, layouts: try layouts(), options: narrow)
        let contents = try XCTUnwrap(SkillStore.load(cache))

        XCTAssertEqual(contents.window, 8)
        XCTAssertNotNil(model.skill(0x008))
    }

    // MARK: - What the window is for

    /// The reason the gaps are a window and not the whole history: a chord that used to be
    /// slow and is now fast has to be able to become learned, or the ladder would never
    /// let go of it.
    func testGettingFasterIsNoticed() throws {
        let keys = ["L.e", "R.t"]
        // A long stretch of being slow, then a shorter one of being quick.
        try day("2026-09-01.txt", keys: keys, count: 1000, gapMs: 1400)
        try day("2026-09-02.txt", keys: keys, count: 300, gapMs: 300)

        let model = try cached()
        let e = try XCTUnwrap(model.skill(0x008))

        XCTAssertEqual(e.count, 650, "every use still counts toward exposure")
        // The gap is to the previous chord of any kind, not the previous `e`, so it is
        // the day's own spacing.
        XCTAssertEqual(e.medianMs, 300, "but the median is over the recent ones")
        XCTAssertTrue(model.learned(0x008))

        // With the window off, the same history is a chord that can never be learned.
        let wide = SkillModel.build(
            logDirectory: dir, layouts: try layouts(),
            options: SkillModel.Options(window: 100_000))
        XCTAssertGreaterThan(try XCTUnwrap(wide.skill(0x008)).medianMs, 1000)
        XCTAssertFalse(wide.learned(0x008))
    }

    /// Deletions are not windowed, and that recovers on its own: a ratio over a growing
    /// denominator falls as the chord goes on being typed correctly.
    func testAccuracyRecoversWithUse() throws {
        let keys = ["L.e", "R.t"]
        try day("2026-09-01.txt", keys: keys, count: 200, gapMs: 300)
        let early = try XCTUnwrap(try cached().skill(0x008))

        try day("2026-09-02.txt", keys: keys, count: 2000, gapMs: 300)
        let late = try XCTUnwrap(try cached().skill(0x008))

        XCTAssertEqual(early.deleted, late.deleted, "no new mistakes")
        XCTAssertLessThanOrEqual(late.errorRate, early.errorRate)
    }
}
