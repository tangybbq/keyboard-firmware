import XCTest
import MinderKit
import TaipoKit
@testable import TaipoTeacherApp

/// The day rollover, which is the part of the writer that had been wrong.
///
/// A batch was formatted with the closing day's offsets and then filed under the opening
/// one, because the reset lives in `file(for:)` and that runs inside `write`, after the
/// records have already been rendered.  Both of the first corpus's rolled-over files begin
/// with a stray record from the day before and then step backwards, with no session header
/// covering either.
final class LogWriterTests: XCTestCase {
    private var directory: URL!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("LogWriterTests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func contents(_ day: String) throws -> [String] {
        let url = directory.appendingPathComponent("\(day).txt")
        return try String(contentsOf: url, encoding: .utf8)
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
            .filter { !$0.isEmpty }
    }

    /// A key record, `delta` milliseconds after the one before it.
    private func key(_ delta: UInt16, _ code: UInt8, press: Bool = true) -> LogRecord {
        .key(code: code, press: press, delta: Delta(raw: delta))
    }

    func testRollingOverStartsANewTimelineUnderItsOwnHeader() throws {
        var clock = Date(timeIntervalSince1970: 1_788_300_000)  // some afternoon
        let writer = LogWriter(directory: directory, now: { clock })
        let layouts = try Layouts.bundled()

        writer.beginSession(device: "mesa1", bootID: 0x1234, fingerprint: 0xabcd)
        writer.append([key(100, 0), key(50, 0, press: false)], layouts: layouts)

        let firstDay = Self.day(of: clock)
        clock = clock.addingTimeInterval(24 * 3600)
        let secondDay = Self.day(of: clock)
        XCTAssertNotEqual(firstDay, secondDay)

        writer.append([key(200, 1), key(60, 1, press: false)], layouts: layouts)
        writer.close()

        let before = try contents(firstDay)
        let after = try contents(secondDay)

        // Yesterday keeps only yesterday's records.
        XCTAssertEqual(before.filter { !$0.hasPrefix("#") }.count, 2)

        // Today opens with the session header repeated, so a reader still knows which
        // keyboard and which chord tables produced what follows.
        XCTAssertTrue(
            after.first?.hasPrefix("# session device=mesa1") == true,
            "the new day opens with \(after.first ?? "nothing")")
        // ...but not the `# started` line: the session did not start again at midnight.
        XCTAssertFalse(after.contains { $0.hasPrefix("# started") })

        // And the records count from the new file's own zero, forwards.
        let times = after.filter { !$0.hasPrefix("#") }
            .compactMap { UInt64($0.split(separator: " ")[0]) }
        XCTAssertEqual(times, [200, 260])
    }

    private static func day(of date: Date) -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        return f.string(from: date)
    }
}
