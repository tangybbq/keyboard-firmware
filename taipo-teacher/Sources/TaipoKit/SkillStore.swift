import Foundation

/// A checkpoint of what the logs have already said, so a rebuild costs what has changed
/// rather than what has ever happened.
///
/// Replaying everything is linear in the whole history -- a year of daily use measures at
/// nine seconds -- and the ladder asks for a rebuild every block.  So finished log files
/// are folded once and their measurements kept.
///
/// **It is a cache, not a second source of truth.**  Deleting the file loses nothing: the
/// logs are still there and folding them all in order gives exactly the same answer, which
/// is a property the tests hold rather than a claim.  That is why the window is in *uses*
/// and not in wall-clock time, and why folding is strictly in file order -- both are what
/// make the incremental answer and the from-scratch one the same answer.
///
/// The newest file is never folded.  It is still being appended to, and it is the one a
/// scrub reaches into, so it is replayed live every time.  That costs a day, not a year.

public struct SkillStore {
    /// One log file, as it looked when it was folded in.
    struct Folded: Codable, Equatable {
        var name: String
        var size: Int
        var modified: Double
    }

    struct Contents: Codable {
        /// Bumped when the shape changes, so an old file is discarded rather than
        /// misread.  A wrong cache is worse than no cache.
        var version: Int = 5
        /// The window the samples were gathered with.  A different one means the stored
        /// gaps are the wrong length and have to be gathered again.
        var window: Int
        /// The layout fingerprint the samples were folded under.
        ///
        /// Which sessions count depends on it, and so does what every chord in them
        /// typed.  New tables therefore mean the whole checkpoint is about a keyboard
        /// that no longer exists.
        var fingerprint: String
        /// The table a session with nothing to say about one was folded as.
        ///
        /// The fingerprint does not cover this -- which table the keyboard comes up in is
        /// not part of what a chord means, so it can change without the tables changing --
        /// but it decides how every unmarked stretch of log was read.  A checkpoint built
        /// under a different one describes typing in the wrong table, and the cache's one
        /// job is to give the same answer a fresh build would.
        var defaultVariant: String
        var folded: [Folded] = []
        /// variant -> chord code, in decimal -> what has been seen of it.
        var samples: [String: [String: ChordSamples]] = [:]
    }

    /// Where the checkpoint lives, given where the logs do.
    ///
    /// Beside them rather than inside: the logs directory is one the writer is invited to
    /// open in the Finder, and everything in it should be a log.
    public static func defaultURL(forLogsIn directory: URL) -> URL {
        directory.deletingLastPathComponent().appendingPathComponent("skill-cache.json")
    }

    /// Build the model for one variant, folding only what has not been folded before.
    ///
    /// Any disagreement between the checkpoint and the files it claims to describe -- a
    /// file changed, shrunk by a scrub, or gone -- throws the whole checkpoint away and
    /// starts again.  Re-folding one changed file into an accumulation that already holds
    /// what came after it would give an answer that is not the answer, and the cache's one
    /// job is to give the same answer.
    public static func model(
        logDirectory: URL, cache: URL, layouts: Layouts, variant: String = "taipo",
        options: SkillModel.Options = SkillModel.Options()
    ) -> SkillModel {
        let files = SkillModel.logFiles(in: logDirectory)
        let fresh = Contents(
            window: options.window, fingerprint: layouts.fingerprint,
            defaultVariant: layouts.defaultVariant)
        var contents = load(cache) ?? fresh
        if contents.version != fresh.version || contents.window != fresh.window
            || contents.fingerprint != fresh.fingerprint
            || contents.defaultVariant != fresh.defaultVariant
        {
            contents = fresh
        }

        // Everything but the newest file can be folded once and kept.
        let settled = files.dropLast()
        let live = files.last

        if !stillDescribes(contents.folded, settled) {
            contents = fresh
        }

        var collector = SkillCollector(
            samples: decode(contents.samples), sessions: 0)
        var folded = contents.folded

        for file in settled.dropFirst(folded.count) {
            guard let text = try? String(contentsOf: file, encoding: .utf8),
                let stamp = stamp(file)
            else { continue }
            collector.fold(text: text, layouts: layouts, options: options)
            folded.append(stamp)
        }

        if folded != contents.folded {
            contents.folded = folded
            contents.samples = encode(collector.samples)
            save(contents, to: cache)
        }

        // The newest file on top, every time, and never kept: it is still growing.
        if let live, let text = try? String(contentsOf: live, encoding: .utf8) {
            collector.fold(text: text, layouts: layouts, options: options)
        }
        return collector.model(variant: variant, options: options)
    }

    /// Whether the checkpoint still describes the files it was built from.
    ///
    /// Prefix-wise: the folded list must be the first files on disk, unchanged.  A file
    /// appearing earlier in name order than one already folded would reorder the history,
    /// which the window's meaning depends on, so that counts as a disagreement too.
    static func stillDescribes(_ folded: [Folded], _ files: ArraySlice<URL>) -> Bool {
        guard folded.count <= files.count else { return false }
        for (was, url) in zip(folded, files) {
            guard let now = stamp(url), now == was else { return false }
        }
        return true
    }

    static func stamp(_ url: URL) -> Folded? {
        guard
            let values = try? url.resourceValues(forKeys: [
                .fileSizeKey, .contentModificationDateKey,
            ]), let size = values.fileSize, let modified = values.contentModificationDate
        else { return nil }
        return Folded(
            name: url.lastPathComponent, size: size,
            modified: modified.timeIntervalSince1970)
    }

    // MARK: - The file

    static func load(_ url: URL) -> Contents? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(Contents.self, from: data)
    }

    /// Written atomically, and a failure to write is not a failure to practise.
    ///
    /// Two rebuilds racing can only cost one of them its saved work; both computed the
    /// same thing from the same files, and the loser's is recomputed next time.
    static func save(_ contents: Contents, to url: URL) {
        guard let data = try? JSONEncoder().encode(contents) else { return }
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: .atomic)
    }

    /// JSON has no integer keys, so the chord codes travel as decimal strings.
    static func encode(_ samples: [String: [UInt16: ChordSamples]])
        -> [String: [String: ChordSamples]]
    {
        samples.mapValues { byCode in
            Dictionary(uniqueKeysWithValues: byCode.map { (String($0.key), $0.value) })
        }
    }

    static func decode(_ samples: [String: [String: ChordSamples]])
        -> [String: [UInt16: ChordSamples]]
    {
        samples.mapValues { byCode in
            Dictionary(
                uniqueKeysWithValues: byCode.compactMap { key, value in
                    UInt16(key).map { ($0, value) }
                })
        }
    }
}
