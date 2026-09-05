import XCTest
@testable import TaipoKit

/// Drill material derived from a log whose mistakes are known in advance.
final class ConfusionDrillTests: XCTestCase {
    private func layouts() throws -> Layouts { try Layouts.bundled() }

    private func golden(_ name: String, _ ext: String) throws -> String {
        let url = try XCTUnwrap(
            Bundle.module.url(forResource: "golden/\(name)", withExtension: ext))
        return try String(contentsOf: url, encoding: .utf8)
    }

    /// A directory holding one log file, named as the collector names them.
    private func logDirectory(_ contents: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("drills-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try (sessionHeader(try Layouts.bundled()) + contents).write(
            to: dir.appendingPathComponent("2026-09-01.txt"), atomically: true, encoding: .utf8)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    /// The model counts the pairs the golden log's corrections say it should.
    ///
    /// `slips.corrections` has the answer written out: the middle finger's two keys are
    /// confused three times, the ring finger's twice, and the pairs that are neither a row
    /// nor a finger slip are not a drill's business at all.
    func testModelCountsTheGoldenPairs() throws {
        let dir = try logDirectory(try golden("slips", "log"))
        let model = ConfusionModel.build(logDirectory: dir, layouts: try layouts())

        XCTAssertEqual(model.sessions, 1)
        XCTAssertEqual(model.corrections, 13)

        let top = try XCTUnwrap(model.pairs.first)
        XCTAssertEqual(top.a, 0x004)  // `t`, bottom middle
        XCTAssertEqual(top.b, 0x040)  // `n`, top middle
        XCTAssertEqual(top.shape, .wrongRow)
        XCTAssertEqual(top.count, 3)

        let ring = try XCTUnwrap(model.pairs.first { $0.a == 0x002 && $0.b == 0x020 })
        XCTAssertEqual(ring.count, 2)

        // The two `unrelated` corrections in that log are not pairs anyone can drill.
        XCTAssertFalse(model.pairs.contains { $0.shape == .unrelated })
        XCTAssertEqual(model.pairs.map(\.count).reduce(0, +), 11)
    }

    /// Corrections made in one chord table say nothing about the other, so a model asked
    /// for the variant the log is not in finds nothing at all.
    func testModelIgnoresTheOtherVariant() throws {
        let dir = try logDirectory(try golden("slips", "log"))
        let taipo = ConfusionModel.build(
            logDirectory: dir, layouts: try layouts(), variant: "taipo")
        let dosh = ConfusionModel.build(
            logDirectory: dir, layouts: try layouts(), variant: "dosh")

        XCTAssertEqual(taipo.corrections, 13)
        XCTAssertEqual(dosh.corrections, 0)
        XCTAssertTrue(dosh.pairs.isEmpty)
    }

    /// A log that switches tables partway counts each stretch against its own table.
    func testModelSplitsAtAVariantMarker() throws {
        // The `slips` log, replayed twice: once as taipo and once as dosh, with a marker
        // between.  Times run on so the two do not read as separate sessions.
        let source = try golden("slips", "log")
        var lines = [String]()
        var lastTime: UInt32 = 0
        for pass in 0..<2 {
            if pass == 1 { lines.append("\(lastTime + 1000) = variant 1") }
            for line in source.split(separator: "\n") {
                let fields = line.split(separator: " ")
                // The golden's own `variant` marker is dropped: this test says which
                // table each pass is in, and a second opinion halfway through would
                // put both passes in the same one.
                if fields.count == 4, fields[1] == "=" { continue }
                guard let time = UInt32(fields[0]) else { continue }
                let shifted = time + UInt32(pass) * 100_000
                lastTime = shifted
                lines.append(
                    ([String(shifted)] + fields.dropFirst().map(String.init))
                        .joined(separator: " "))
            }
        }
        let dir = try logDirectory(lines.joined(separator: "\n") + "\n")

        // Each table sees exactly the corrections made while it was the live one: one
        // pass each, never both.  The counts differ because Dosh has the thumbs swapped,
        // so the same keystrokes read as backspaces in one table and spaces in the other
        // -- what matters here is that neither table saw the other's pass.
        for (variant, want) in [("taipo", 13), ("dosh", 9)] {
            let model = ConfusionModel.build(
                logDirectory: dir, layouts: try layouts(), variant: variant)
            XCTAssertEqual(model.corrections, want, "\(variant)")
        }
    }

    /// The warmup is the two chords against each other, and every group uses both.
    func testWarmupUsesBothChords() throws {
        let maker = DrillMaker(layouts: try layouts(), words: [])
        let pair = ConfusionPair(a: 0x002, b: 0x020, shape: .wrongRow, count: 9)
        let warmup = try XCTUnwrap(maker.warmup(pair))

        for group in warmup.split(separator: " ") {
            XCTAssertTrue(group.contains("o"), "\(group) should use o")
            XCTAssertTrue(group.contains("s"), "\(group) should use s")
        }
        // Doubles are in there on purpose: `oo` before `s` is where the wrong row gets
        // pressed.
        XCTAssertTrue(warmup.split(separator: " ").contains { $0.contains("oo") })
        XCTAssertTrue(warmup.split(separator: " ").contains { $0.contains("ss") })
    }

    /// A chord that types nothing has no warmup, and says so rather than inventing one.
    func testAChordThatTypesNothingHasNoWarmup() throws {
        let maker = DrillMaker(layouts: try layouts(), words: [])
        // `[Bk]` is a backspace: it types no characters at all.
        let pair = ConfusionPair(a: 0x200, b: 0x002, shape: .wrongFinger, count: 1)
        XCTAssertNil(maker.warmup(pair))
    }

    /// Every drill word really does ask for both chords.
    func testWordsUseBothChords() throws {
        let layouts = try layouts()
        let maker = DrillMaker(layouts: layouts)
        let pair = ConfusionPair(a: 0x002, b: 0x020, shape: .wrongRow, count: 143)
        let lines = maker.words(pair, count: 24)
        let words = lines.flatMap { $0.split(separator: " ").map(String.init) }
        XCTAssertGreaterThan(words.count, 12, "there should be plenty of o/s words")

        for word in words {
            let codes = DrillTarget(text: word, layouts: layouts).units.map(\.code)
            XCTAssertTrue(codes.contains(pair.a), "\(word) should need o")
            XCTAssertTrue(codes.contains(pair.b), "\(word) should need s")
            // Capitals want a thumb chord as well, which is a different skill; a
            // discrimination drill should not be teaching two things at once.
            XCTAssertEqual(word, word.lowercased(), "\(word) is not lower case")
        }
    }

    /// The same model produces the same drill twice.
    ///
    /// The pairs come out of a dictionary, so without an explicit tie break the order would
    /// depend on the hashing.  A practice list that reshuffles itself between runs is one
    /// nobody can tell they have finished.
    func testTheProgrammeIsStable() throws {
        let dir = try logDirectory(try golden("slips", "log"))
        let layouts = try layouts()
        let maker = DrillMaker(layouts: layouts)

        let first = maker.programme(ConfusionModel.build(logDirectory: dir, layouts: layouts))
        let second = maker.programme(ConfusionModel.build(logDirectory: dir, layouts: layouts))
        XCTAssertEqual(first, second)
        XCTAssertFalse(first.isEmpty)
    }

    /// An empty log directory is not an error; there is simply nothing to practise yet.
    func testNoLogsMeansNoDrills() throws {
        let dir = try logDirectory("")
        let layouts = try layouts()
        let model = ConfusionModel.build(logDirectory: dir, layouts: layouts)
        XCTAssertEqual(model.corrections, 0)
        XCTAssertTrue(DrillMaker(layouts: layouts, words: []).programme(model).isEmpty)
    }
}
