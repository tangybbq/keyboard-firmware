import Foundation

/// How well each chord is known, derived from the writer's own logs.
///
/// The same trick as `ConfusionModel`: nothing is stored, because the logs already say
/// everything a skill model needs.  How often a chord has been typed, how long the hand
/// takes over it, and how often a backspace follows it are all in there, and practice done
/// in the app goes through the keyboard like everything else -- so a drill improves the
/// model that chose it, with no results file to keep in step.
///
/// The measure of speed is the gap from the previous chord rather than the chord's own
/// spread.  Spread says how sloppily the keys of one chord were struck; the gap says how
/// long the hand took to *find* it, which is what practice changes and what a drill is
/// trying to move.

/// What the logs say about one chord.
public struct ChordSkill: Equatable, Sendable {
    public let code: UInt16
    /// How many times it has been typed.
    public let count: Int
    /// The median gap from the chord before it, in milliseconds.
    ///
    /// Median rather than mean: a log of real work is full of pauses to think, and one
    /// long one would swamp an average.  Gaps longer than `Options.pauseMs` are dropped
    /// before this is taken, for the same reason the alternation rule has a window -- a
    /// trainer must never mistake stopping to think for being slow.
    public let medianMs: UInt32
    /// How many times a backspace deleted it.
    public let deleted: Int

    /// How often typing it is followed by taking it back.
    public var errorRate: Double {
        count > 0 ? Double(deleted) / Double(count) : 0
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

        public init(
            minSamples: Int = 12, targetMs: UInt32 = 700, maxErrorRate: Double = 0.1,
            pauseMs: UInt32 = 2000
        ) {
            self.minSamples = minSamples
            self.targetMs = targetMs
            self.maxErrorRate = maxErrorRate
            self.pauseMs = pauseMs
        }
    }

    public let skills: [UInt16: ChordSkill]
    /// How many sessions were read, so a thin model can say it is thin.
    public let sessions: Int
    /// How many chords of the requested variant were seen in all.
    public let chords: Int
    public let options: Options

    public init(
        skills: [UInt16: ChordSkill], sessions: Int, chords: Int, options: Options = Options()
    ) {
        self.skills = skills
        self.sessions = sessions
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
    /// Session by session, as `ConfusionModel` does and for the same reasons: the gap
    /// across a join is not an interval anyone typed, and a backspace does not correct
    /// something typed yesterday.
    public static func build(
        logDirectory: URL, layouts: Layouts, variant: String = "taipo",
        options: Options = Options()
    ) -> SkillModel {
        let files =
            (try? FileManager.default.contentsOfDirectory(
                at: logDirectory, includingPropertiesForKeys: nil))?
            .filter { $0.pathExtension == "txt" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent } ?? []

        var gaps: [UInt16: [UInt32]] = [:]
        var counts: [UInt16: Int] = [:]
        var deleted: [UInt16: Int] = [:]
        var sessions = 0
        var total = 0
        let scanner = CorrectionScanner(layouts: layouts, variant: variant)

        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            for session in KeyLogFile.sessions(from: text, layouts: layouts) {
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
                let mine = chords.filter { $0.variant == variant }
                total += mine.count

                var previousMs: UInt32?
                for chord in mine {
                    counts[chord.code, default: 0] += 1
                    if let previous = previousMs, chord.timeMs >= previous {
                        let gap = chord.timeMs - previous
                        if gap <= options.pauseMs { gaps[chord.code, default: []].append(gap) }
                    }
                    previousMs = chord.timeMs
                }

                for correction in scanner.scan(mine.map(\.code)) {
                    if let d = correction.deleted { deleted[d, default: 0] += 1 }
                }
            }
        }

        var skills: [UInt16: ChordSkill] = [:]
        for (code, count) in counts {
            skills[code] = ChordSkill(
                code: code, count: count, medianMs: median(gaps[code] ?? []),
                deleted: deleted[code] ?? 0)
        }
        return SkillModel(
            skills: skills, sessions: sessions, chords: total, options: options)
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
