import Foundation
import MinderKit
import TaipoKit

/// Appends the key log to disk, in the format `keyminder log` writes.
///
/// Byte-identical to the Rust collector's output on purpose: `taipo-analyze` reads these
/// files, and a second format would mean a second parser and two things to keep in step.
/// The Rust side stays the reference; this follows it.
///
/// One file a day.  A day is a natural unit for looking at typing, the files stay small
/// enough to grep, and rotating on a date needs no bookkeeping that could go wrong.
final class LogWriter {
    private let directory: URL
    private var handle: FileHandle?
    private var openedDay: String?
    /// Running offset within the current file, so a day reads as one timeline.
    private var offsetMs: UInt64 = 0

    /// Where the logs live.
    static var defaultDirectory: URL {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("TaipoTeacher/logs", isDirectory: true)
    }

    init(directory: URL = LogWriter.defaultDirectory) {
        self.directory = directory
    }

    private static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    private func file(for date: Date) throws -> FileHandle {
        let day = Self.dayFormatter.string(from: date)
        if day == openedDay, let handle { return handle }

        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("\(day).txt")
        if !FileManager.default.fileExists(atPath: url.path) {
            FileManager.default.createFile(atPath: url.path, contents: nil)
        }
        let handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd()
        self.handle = handle
        openedDay = day
        // A new file starts a new timeline; the session header that follows says when.
        offsetMs = 0
        return handle
    }

    /// Open a session: a header saying which keyboard, which boot, and which tables.
    ///
    /// The fingerprint is what makes a replay checkable.  Records replayed through chord
    /// tables other than the ones that produced them are plausible and wrong, so a reader
    /// that finds an unfamiliar fingerprint should stop rather than derive something.
    func beginSession(device: String, bootID: UInt64?, fingerprint: UInt64?) {
        let boot = bootID.map { String(format: "%#018llx", $0) } ?? "unknown"
        let layout = fingerprint.map { String(format: "%#018llx", $0) } ?? "unknown"
        let now = UInt64(Date().timeIntervalSince1970)
        write(
            "# session device=\(device) boot_id=\(boot) layout=\(layout)\n"
            + "# started \(now) (unix seconds)\n")
    }

    /// Note that the device restarted, so the reader knows the timeline broke.
    func noteReset() {
        write("# device reset\n")
    }

    /// Note records the device dropped before they could be fetched.
    func noteGap(dropped: UInt32, beforeSeq: UInt32) {
        write("# gap \(dropped) records dropped before seq \(beforeSeq)\n")
    }

    /// Append a batch, advancing the timeline.
    func append(_ records: [LogRecord], layouts: Layouts) {
        var text = ""
        for record in records {
            offsetMs += record.delta.milliseconds
            switch record {
            case .key(let code, let press, _):
                text += "\(offsetMs) \(press ? "+" : "-") \(keyName(code, layouts))\n"
            case .marker(let marker, let value, _):
                text += "\(offsetMs) = \(marker.name) \(value)\n"
            case .unknown(let tag, _):
                text += "# unknown record tag \(tag)\n"
            }
        }
        write(text)
    }

    /// `L.t` and `R.Sp` for the taipo keys, `k12` for anything else -- the same names
    /// `bbq_keyboard::replay::key_name` produces, so the files parse there.
    private func keyName(_ code: UInt8, _ layouts: Layouts) -> String {
        let table = layouts.scanMap.upper
        guard Int(code) < table.count,
              let side = table[Int(code)].side,
              let name = table[Int(code)].name
        else { return "k\(code)" }
        return "\(side == "left" ? "L" : "R").\(name)"
    }

    private func write(_ text: String) {
        guard !text.isEmpty, let data = text.data(using: .utf8) else { return }
        do {
            let handle = try file(for: Date())
            try handle.write(contentsOf: data)
        } catch {
            // Losing the log is not worth losing the session over; the drill and the live
            // view carry on regardless.
            NSLog("taipo-teacher: could not write the log: \(error)")
        }
    }

    func close() {
        try? handle?.close()
        handle = nil
        openedDay = nil
    }
}
