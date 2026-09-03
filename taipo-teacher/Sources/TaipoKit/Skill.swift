import Foundation

/// How well each chord is known, derived from the writer's own logs.
///
/// The logs already say everything a skill model needs -- how often a chord has been
/// typed, how long the hand takes over it, and how often a backspace follows it -- so this
/// is derived rather than recorded, as `ConfusionModel` is.  Practice goes through the
/// keyboard like any other typing, so a drill improves the model that chose it, with no
/// results file to keep in step.
///
/// The measure of speed is the gap from the previous chord rather than the chord's own
/// spread.  Spread says how sloppily the keys of one chord were struck; the gap says how
/// long the hand took to *find* it, which is what practice changes and what a drill is
/// trying to move.
///
/// **Two things bound it.**  The gaps are kept as a window of the most recent
/// `Options.window` uses rather than all of them, so a chord that used to be slow and is
/// now fast reads as fast.  A median over a year of a common letter cannot move at all,
/// and an item that cannot move is one the ladder would never let go of.  And the folding
/// is incremental, through `SkillStore`, because replaying every log on every rebuild is
/// linear in the whole history: a year of daily use measured at nine seconds, and the
/// ladder asks for a rebuild every block.
///
/// Deletions are *not* windowed, and that asymmetry is deliberate.  A ratio over a growing
/// denominator recovers on its own -- type it right often enough and the early mistakes
/// dilute -- where a median simply stops moving.

/// What the logs say about one chord.
public struct ChordSkill: Equatable, Sendable {
    public let code: UInt16
    /// How many times it has been typed, over the whole history.
    public let count: Int
    /// The median gap from the chord before it, over the most recent uses.
    ///
    /// Median rather than mean: a log of real work is full of pauses to think, and one
    /// long one would swamp an average.  Gaps longer than `Options.pauseMs` are dropped
    /// before this is taken, for the same reason the alternation rule has a window -- a
    /// trainer must never mistake stopping to think for being slow.
    public let medianMs: UInt32
    /// How many times a backspace deleted it, over the whole history.
    public let deleted: Int

    public init(code: UInt16, count: Int, medianMs: UInt32, deleted: Int) {
        self.code = code
        self.count = count
        self.medianMs = medianMs
        self.deleted = deleted
    }

    /// How often typing it is followed by taking it back.
    public var errorRate: Double {
        count > 0 ? Double(deleted) / Double(count) : 0
    }
}

/// The measurements for one chord, as they are accumulated and stored.
struct ChordSamples: Codable, Equatable {
    var total: Int = 0
    var deleted: Int = 0
    /// The most recent timed gaps, oldest first, capped at `Options.window`.
    var gaps: [UInt32] = []

    mutating func note(gap: UInt32?, window: Int) {
        total += 1
        guard let gap else { return }
        gaps.append(gap)
        if gaps.count > window { gaps.removeFirst(gaps.count - window) }
    }
}

/// Folding logs into measurements, one file at a time.
///
/// Separate from `SkillModel` because the same folding has to serve both the from-scratch
/// build and `SkillStore`'s incremental one, and the two must agree exactly: the store is
/// only a cache if deleting it and starting again gives the same answer.
struct SkillCollector {
    /// variant -> chord code -> what has been seen of it.
    var samples: [String: [UInt16: ChordSamples]] = [:]
    var sessions = 0
    /// Sessions passed over because they were recorded against other tables.
    var skipped = 0

    /// Fold one log file's sessions in, for every variant it contains.
    ///
    /// All variants at once, rather than only the one being asked about, so that switching
    /// tables does not mean folding the history again.
    mutating func fold(text: String, layouts: Layouts, options: SkillModel.Options) {
        for session in KeyLogFile.sessions(from: text, layouts: layouts) {
            // Recorded against other tables, so its chords mean something else.  Counted
            // rather than dropped in silence: a ladder that has quietly reset is a thing
            // the writer is owed an explanation for.
            guard session.recorded(with: layouts) else {
                skipped += 1
                continue
            }
            sessions += 1
            let engine = ChordEngine(layouts: layouts)
            var chords = [Chord]()
            for entry in session.entries {
                switch entry {
                case .marker(let m): engine.marker(m.name, value: m.value)
                case .key(let e):
                    chords += engine.feed(key: e.key, press: e.press, timeMs: e.timeMs)
                }
            }
            chords += engine.finish()

            for variant in Set(chords.map(\.variant)) {
                let mine = chords.filter { $0.variant == variant }
                var byCode = samples[variant] ?? [:]

                var previousMs: UInt32?
                for chord in mine {
                    var gap: UInt32?
                    if let previous = previousMs, chord.timeMs >= previous {
                        let d = chord.timeMs - previous
                        if d <= options.pauseMs { gap = d }
                    }
                    byCode[chord.code, default: ChordSamples()]
                        .note(gap: gap, window: options.window)
                    previousMs = chord.timeMs
                }

                let scanner = CorrectionScanner(layouts: layouts, variant: variant)
                for correction in scanner.scan(mine.map(\.code)) {
                    if let d = correction.deleted {
                        byCode[d, default: ChordSamples()].deleted += 1
                    }
                }
                samples[variant] = byCode
            }
        }
    }

    /// The finished measurements for one variant.
    func model(variant: String, options: SkillModel.Options) -> SkillModel {
        let byCode = samples[variant] ?? [:]
        var skills = [UInt16: ChordSkill]()
        for (code, s) in byCode {
            skills[code] = ChordSkill(
                code: code, count: s.total, medianMs: SkillModel.median(s.gaps),
                deleted: s.deleted)
        }
        return SkillModel(
            skills: skills, sessions: sessions, skipped: skipped,
            chords: byCode.values.reduce(0) { $0 + $1.total }, options: options)
    }
}

/// The skill model: one entry per chord the logs have seen.
public struct SkillModel: Sendable {
    /// What each measurement has to reach for a chord to count as known.
    public struct Options: Sendable {
        /// How many times a chord must have been typed before its timing means anything.
        public var minSamples: Int
        /// The median gap a learned chord is expected to be under.
        public var targetMs: UInt32
        /// The share of a chord's uses that may be corrections.
        public var maxErrorRate: Double
        /// Gaps longer than this are a pause, not a chord that was slow to find.
        public var pauseMs: UInt32
        /// How many recent uses the median is taken over.
        ///
        /// In uses rather than in days, which is the right unit here: sixty-four uses of a
        /// common letter is a few minutes of typing and sixty-four of `q` is a fortnight,
        /// and in both cases it is "lately".
        public var window: Int

        public init(
            minSamples: Int = 12, targetMs: UInt32 = 700, maxErrorRate: Double = 0.1,
            pauseMs: UInt32 = 2000, window: Int = 64
        ) {
            self.minSamples = minSamples
            self.targetMs = targetMs
            self.maxErrorRate = maxErrorRate
            self.pauseMs = pauseMs
            self.window = window
        }
    }

    public let skills: [UInt16: ChordSkill]
    /// How many sessions were read, so a thin model can say it is thin.
    public let sessions: Int
    /// How many were passed over as recorded against other chord tables.
    public let skipped: Int
    /// How many chords of the requested variant were seen in all.
    public let chords: Int
    public let options: Options

    public init(
        skills: [UInt16: ChordSkill], sessions: Int, skipped: Int = 0, chords: Int,
        options: Options = Options()
    ) {
        self.skills = skills
        self.sessions = sessions
        self.skipped = skipped
        self.chords = chords
        self.options = options
    }

    public func skill(_ code: UInt16) -> ChordSkill? { skills[code] }

    /// Whether a chord has been typed often enough, quickly enough, and cleanly enough.
    public func learned(_ code: UInt16) -> Bool {
        guard let s = skills[code] else { return false }
        return s.count >= options.minSamples && s.medianMs <= options.targetMs
            && s.errorRate <= options.maxErrorRate
    }

    /// A single number for ordering the weakest first, in `0...1`.
    ///
    /// The product of the three, so that a chord which is fast but wrong and one which is
    /// accurate but slow both rank below one that is neither.  A chord the logs have never
    /// seen scores 0, which puts anything newly unlocked straight into the focus set --
    /// which is where a thing you have never typed belongs.
    public func confidence(_ code: UInt16) -> Double {
        guard let s = skills[code], s.count > 0 else { return 0 }
        let exposure = min(1, Double(s.count) / Double(max(1, options.minSamples)))
        let speed = min(1, Double(options.targetMs) / Double(max(1, s.medianMs)))
        let accuracy = max(0, 1 - s.errorRate / max(0.0001, options.maxErrorRate))
        return exposure * speed * accuracy
    }

    /// Replay every log in a directory and measure each chord.
    ///
    /// The whole history, every time.  Correct, and what the incremental path is checked
    /// against, but linear in how much has ever been typed -- see `SkillStore` for the one
    /// the app uses.
    public static func build(
        logDirectory: URL, layouts: Layouts, variant: String = "taipo",
        options: Options = Options()
    ) -> SkillModel {
        var collector = SkillCollector()
        for file in logFiles(in: logDirectory) {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            collector.fold(text: text, layouts: layouts, options: options)
        }
        return collector.model(variant: variant, options: options)
    }

    /// The log files, in the order they were written.
    ///
    /// Name order, which for `yyyy-MM-dd.txt` is date order, and date order is what makes
    /// "the most recent uses" mean anything.
    static func logFiles(in directory: URL) -> [URL] {
        (try? FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: nil))?
            .filter { $0.pathExtension == "txt" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent } ?? []
    }

    /// The middle value, or `UInt32.max` for a chord with no timed use at all.
    ///
    /// Not zero: a chord seen once, at the start of a session, would otherwise look like
    /// the fastest thing on the keyboard.
    static func median(_ values: [UInt32]) -> UInt32 {
        guard !values.isEmpty else { return .max }
        let sorted = values.sorted()
        return sorted[sorted.count / 2]
    }
}
