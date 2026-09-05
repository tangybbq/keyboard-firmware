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
    /// Where the file had got to *after* a write at a given wall-clock moment.
    ///
    /// After, not before.  A checkpoint taken before the write points at an offset that
    /// still has that write ahead of it, so truncating there throws away something written
    /// before the cutoff -- which a test caught immediately, deleting a line that should
    /// have survived.
    ///
    /// This is what makes a retroactive scrub possible.  Prevention only covers what the
    /// system tells us about, and it does not tell us about a password prompt inside tmux,
    /// so there has to be a way to take something back after the fact.
    private var checkpoints: [(at: Date, offset: UInt64)] = []
    /// The header of the session in progress, so a day rollover can repeat it.
    ///
    /// A new file is a new timeline, and a timeline with no header is one a reader cannot
    /// check the fingerprint of.  Sessions outlive midnight, so the header has to be
    /// reproducible rather than only written when the device connects.
    private var sessionHeader: String?
    /// What the device last said its state was: the mode, the chord table and the row
    /// position.
    ///
    /// A marker records a *change*, so a reader takes the state from the markers it has
    /// seen and its own defaults for whatever it has not.  That holds within a file and
    /// breaks across two.  A session that crosses midnight opened the new file with no
    /// markers in it at all, and nothing announced the state again until the writer next
    /// changed something -- so a morning of Dosh read as Taipo, folded into the wrong
    /// table's measurements, with nothing about the replay looking wrong.  A rollover
    /// therefore repeats the state as well as the header.
    private var state: [Marker: UInt8] = [:]

    /// The order a repeated snapshot is written in, which is the order the device sends
    /// its own on connect.
    private static let stateMarkers: [Marker] = [.mode, .variant, .rowShift]

    /// Where the logs live.
    static var defaultDirectory: URL {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("TaipoTeacher/logs", isDirectory: true)
    }

    /// What the writer reads the clock as.  Injectable so that a test can cross midnight
    /// without waiting for one: the day rollover is the part of this that has been wrong,
    /// and it is unreachable otherwise.
    private let now: () -> Date

    init(directory: URL = LogWriter.defaultDirectory, now: @escaping () -> Date = Date.init) {
        self.directory = directory
        self.now = now
    }

    private static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        return f
    }()

    /// The handle for `date`'s file, opening -- and starting -- a new day's file if the
    /// one in hand is not it.
    ///
    /// Callers that are about to format records must go through `rollOver` first: this
    /// resets the offset a new file counts from, and records formatted before it runs
    /// would carry the previous file's offsets into the new one.
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
        let opening = openedDay
        self.handle = handle
        openedDay = day
        // A new file starts a new timeline, counting from a fresh zero.
        offsetMs = 0
        checkpoints.removeAll()
        // Carry the session across midnight.  `opening == nil` is the first write of the
        // run, where `beginSession` is about to write the header itself; anything else is
        // a rollover in the middle of a session, and the new file needs its own copy or
        // it opens with records whose device, boot and tables are nowhere stated.
        if opening != nil, let header = sessionHeader {
            var opener = header
            // At offset zero, because that is where the new timeline starts and the state
            // was already true before the first record in it.
            for marker in Self.stateMarkers where state[marker] != nil {
                opener += "0 = \(marker.name) \(state[marker]!)\n"
            }
            try handle.write(contentsOf: Data(opener.utf8))
        }
        return handle
    }

    /// Open today's file, if today is not the day the open one belongs to.
    ///
    /// Separate from `write` because the offset it resets is read while records are being
    /// formatted, which happens first.  Rolling over inside `write` stamped a whole batch
    /// with the closing day's offsets and then filed it under the opening one -- a step
    /// backwards in the middle of a file, and the only record in it not covered by a
    /// session header.
    private func rollOver() {
        _ = try? file(for: now())
    }

    /// Open a session: a header saying which keyboard, which boot, and which tables.
    ///
    /// The fingerprint is what makes a replay checkable.  Records replayed through chord
    /// tables other than the ones that produced them are plausible and wrong, so a reader
    /// that finds an unfamiliar fingerprint should stop rather than derive something.
    func beginSession(device: String, bootID: UInt64?, fingerprint: UInt64?) {
        let boot = bootID.map { String(format: "%#018llx", $0) } ?? "unknown"
        let layout = fingerprint.map { String(format: "%#018llx", $0) } ?? "unknown"
        let started = UInt64(now().timeIntervalSince1970)
        rollOver()
        let header = "# session device=\(device) boot_id=\(boot) layout=\(layout)\n"
        // The `# started` line is when the collector connected, so it is not repeated at
        // a rollover: the session did not start again at midnight.
        sessionHeader = header
        write(header + "# started \(started) (unix seconds)\n")
    }

    /// Note that the device restarted, so the reader knows the timeline broke.
    func noteReset() {
        rollOver()
        // The offsets carry on; it is the header that stops applying, since the session
        // it described is over and `beginSession` is what says what replaced it.  The
        // state goes with it: the device is starting again and will announce its own,
        // and repeating what the last boot was doing would be a guess.
        sessionHeader = nil
        state.removeAll()
        write("# device reset\n")
    }

    /// Note records the device dropped before they could be fetched.
    func noteGap(dropped: UInt32, beforeSeq: UInt32) {
        rollOver()
        write("# gap \(dropped) records dropped before seq \(beforeSeq)\n")
    }

    /// Append a batch, advancing the timeline.
    func append(_ records: [LogRecord], layouts: Layouts) {
        // Before a single offset is read: the file this lands in decides where they count
        // from.
        rollOver()
        var text = ""
        for record in records {
            offsetMs += record.delta.milliseconds
            switch record {
            case .key(let code, let press, _):
                text += "\(offsetMs) \(press ? "+" : "-") \(keyName(code, layouts))\n"
            case .marker(let marker, let value, _):
                text += "\(offsetMs) = \(marker.name) \(value)\n"
                state[marker] = value
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
            let at = now()
            let handle = try file(for: at)
            try handle.write(contentsOf: data)
            checkpoints.append((at, try handle.offset()))
            // An hour of checkpoints is far more than any scrub will reach back for.
            let cutoff = at.addingTimeInterval(-3600)
            checkpoints.removeAll { $0.at < cutoff }
        } catch {
            // Losing the log is not worth losing the session over; the drill and the live
            // view carry on regardless.
            NSLog("taipo-teacher: could not write the log: \(error)")
        }
    }

    /// Throw away everything written since `date`, and report how many bytes went.
    ///
    /// Truncation rather than rewriting: the file is append-only and a scrub is meant to
    /// leave nothing behind, so cutting it back to a known offset is both the simplest
    /// thing and the one with no chance of leaving a fragment.
    @discardableResult
    func discard(since date: Date) -> UInt64 {
        guard let handle else { return 0 }
        // The last checkpoint at or before the cutoff is where the file has to go back to.
        // Without one, everything in this file is newer than the cutoff.
        let target = checkpoints.last { $0.at <= date }?.offset ?? 0
        do {
            let end = try handle.offset()
            guard end > target else { return 0 }
            try handle.truncate(atOffset: target)
            try handle.seek(toOffset: target)
            checkpoints.removeAll { $0.at > date }
            // The timeline restarts: the offsets that followed are gone, and a fresh
            // header will say so.
            offsetMs = 0
            write("# scrubbed \(end - target) bytes at \(UInt64(now().timeIntervalSince1970))\n")
            return end - target
        } catch {
            NSLog("taipo-teacher: could not scrub the log: \(error)")
            return 0
        }
    }

    func close() {
        try? handle?.close()
        handle = nil
        openedDay = nil
    }
}
