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
/// Deletions are windowed too, for the same reason and after the same argument was got
/// wrong once.  A ratio over a growing denominator does recover on its own, but only if
/// the *recent* rate is already under the bar, and it recovers at a pace set by everything
/// banked before it: a chord with 16 deletions in 57 uses needs 103 further clean ones to
/// average a tenth, and the chords with the most banked are exactly the ones being drilled
/// hardest.  It felt stuck because it was stuck.
///
/// Windowing a gate makes it something a bad spell can take away, which is what pushed
/// speed out of the gate in the first place.  So the gate is not the current rate but
/// whether the rate has *ever* been good with enough uses behind it -- `everReached`,
/// worked out as the logs are folded and sticky once set.  A demonstration is historical
/// by nature: having shown you can type a thing is not undone by a bad afternoon.

/// What the logs say about one chord.
public struct ChordSkill: Equatable, Sendable {
    public let code: UInt16
    /// How many times it has been typed, over the whole history.
    public let count: Int
    /// Whether the chord has ever been typed enough times with few enough of them taken
    /// back.  Sticky: see the note on windowing at the top of the file.
    public let everReached: Bool
    /// The median gap from the chord before it, over the most recent uses.
    ///
    /// Median rather than mean: a log of real work is full of pauses to think, and one
    /// long one would swamp an average.  Gaps longer than `Options.pauseMs` are dropped
    /// before this is taken, for the same reason the alternation rule has a window -- a
    /// trainer must never mistake stopping to think for being slow.
    public let medianMs: UInt32
    /// How many of the most recent uses were taken back.
    public let deleted: Int
    /// How many of those recent uses there were, which is the window or fewer.
    public let recent: Int

    public init(
        code: UInt16, count: Int, medianMs: UInt32, deleted: Int, recent: Int? = nil,
        everReached: Bool? = nil
    ) {
        self.code = code
        self.count = count
        self.medianMs = medianMs
        self.deleted = deleted
        self.recent = recent ?? count
        // A model built by hand -- which is every test that does not fold a log -- gets
        // the answer its numbers imply, so it does not have to know this exists.
        self.everReached = everReached ?? (count > 0 && deleted * 10 <= count)
    }

    /// How often typing it is followed by taking it back.
    ///
    /// Capped at all of them, which nothing should now reach: a backspace run broken by a
    /// modifier used to read as two corrections, both blaming the chord before the run, so
    /// a chord could be charged more deletions than it had uses.  That is fixed in the
    /// correction scanner on both sides.  The cap stays as a belt: it is a ratio the
    /// screen prints, and there is no reading of "114% taken back" worth showing a writer.
    public var errorRate: Double {
        recent > 0 ? min(1, Double(deleted) / Double(recent)) : 0
    }
}

/// The measurements for one chord, as they are accumulated and stored.
struct ChordSamples: Codable, Equatable {
    var total: Int = 0
    /// The most recent timed gaps, oldest first, capped at `Options.window`.
    var gaps: [UInt32] = []
    /// Whether each of the most recent uses was taken back, oldest first, same cap.
    var outcomes: [Bool] = []
    /// Whether the chord has ever met the gate.  Once true, always true.
    var everReached = false

    /// Note one use, and see whether it is the one that gets the chord past the gate.
    ///
    /// Checked per use rather than per session, because a session can be a whole day and
    /// the answer is "was it ever good", not "was it good at closing time".
    mutating func note(gap: UInt32?, deleted: Bool, options: SkillModel.Options) {
        total += 1
        if let gap {
            gaps.append(gap)
            if gaps.count > options.window { gaps.removeFirst(gaps.count - options.window) }
        }
        outcomes.append(deleted)
        if outcomes.count > options.window {
            outcomes.removeFirst(outcomes.count - options.window)
        }
        if !everReached, total >= options.minSamples {
            let bad = outcomes.filter { $0 }.count
            let allowed = options.allowedErrorRate(after: total)
            if Double(bad) <= allowed * Double(outcomes.count) { everReached = true }
        }
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

                // Which uses were taken back, worked out before the walk rather than
                // after it: each use is noted with its own outcome, so the window holds
                // "was this one undone" and not a count bolted on at the end.
                let scanner = CorrectionScanner(layouts: layouts, variant: variant)
                let undone = Set(
                    scanner.scanIndexed(mine.map(\.code)).compactMap(\.deletedIndex))

                var previousMs: UInt32?
                for (i, chord) in mine.enumerated() {
                    var gap: UInt32?
                    if let previous = previousMs, chord.timeMs >= previous {
                        let d = chord.timeMs - previous
                        if d <= options.pauseMs { gap = d }
                    }
                    byCode[chord.code, default: ChordSamples()]
                        .note(gap: gap, deleted: undone.contains(i), options: options)
                    previousMs = chord.timeMs
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
                deleted: s.outcomes.filter { $0 }.count, recent: s.outcomes.count,
                everReached: s.everReached)
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
        /// After this many uses, twice that share will do.
        ///
        /// A hard chord can sit just above the bar for as long as it is being drilled,
        /// because the window is filled with the drill -- which is the writer's worst
        /// context, while the bar is set by chords they meet in ordinary work.  The
        /// apostrophe and `b` both plateaued at 11% against a bar of 10% and never once
        /// dipped under it in ninety-six and sixty-three uses.  Neither is running ahead
        /// of the writer at 89% right; they are just harder than what came before.
        ///
        /// So exposure buys patience.  Being accurate is one way past the gate and having
        /// put in the practice while not being wildly off is another.
        public var patience: Int
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
            pauseMs: UInt32 = 2000, window: Int = 64, patience: Int = 60
        ) {
            self.minSamples = minSamples
            self.targetMs = targetMs
            self.maxErrorRate = maxErrorRate
            self.pauseMs = pauseMs
            self.window = window
            self.patience = patience
        }

        /// The share of recent uses that may be corrections, given how many there have
        /// been in all.
        public func allowedErrorRate(after uses: Int) -> Double {
            uses >= patience ? maxErrorRate * 2 : maxErrorRate
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

    /// Whether a chord can be produced reliably: typed often enough, and not often taken
    /// back.  Speed is deliberately not asked about.
    ///
    /// This is the gate the ladder moves on, and it is a different question from being
    /// good at a chord.  What decides whether the writer has to break off and switch
    /// tables is whether they know how to type a thing at all; how quickly they manage it
    /// is what practice is for, and practice needs the chord to be in the material first.
    ///
    /// Leaving speed out has a second effect worth having.  The median is windowed to
    /// recent uses, so drilling a chord you are slow at pushes it *down* -- practice could
    /// un-learn an item and take the ladder backwards with it, which is a demoralising
    /// thing for a progress bar to do.  Uses only ever grow.
    public func reached(_ code: UInt16) -> Bool {
        guard let s = skills[code] else { return false }
        // The exposure test as well, though folding only ever sets `everReached` with it
        // already met.  It costs nothing, it is monotone like the flag itself, and it
        // keeps a model assembled by hand in a test from claiming a chord seen twice has
        // been demonstrated.
        return s.everReached && s.count >= options.minSamples
    }

    /// Whether a chord has been typed often enough, quickly enough, and cleanly enough.
    ///
    /// The whole of it, which is what the screen reports and what the hint fades by.  The
    /// ladder asks the easier question; see `reached`.
    public func learned(_ code: UInt16) -> Bool {
        guard let s = skills[code] else { return false }
        return s.count >= options.minSamples && s.medianMs <= options.targetMs
            && s.errorRate <= options.maxErrorRate
    }

    /// How far a chord is toward being learned, split into the three things being asked
    /// of it.
    ///
    /// Split rather than summed because the sum does not say what to do about it.  A
    /// chord at 0.4 might be one you have hardly typed, one you type slowly, or one you
    /// keep taking back, and those are three different afternoons.
    public struct Parts: Equatable, Sendable {
        /// Typed often enough, against `minSamples`.
        public let exposure: Double
        /// Quick enough, against `targetMs`.
        public let speed: Double
        /// Left alone often enough, against `maxErrorRate`.
        public let accuracy: Double

        /// The product, so that a chord which is fast but wrong and one which is accurate
        /// but slow both rank below one that is neither.
        public var confidence: Double { exposure * speed * accuracy }

        /// Whether all three are satisfied, which is what `learned` asks.
        public var complete: Bool { exposure >= 1 && speed >= 1 && accuracy >= 1 }
    }

    /// The three, for one chord.
    ///
    /// A chord the logs have never seen scores zero on all of them, which puts anything
    /// newly unlocked straight into the focus set -- which is where a thing you have never
    /// typed belongs.
    public func parts(_ code: UInt16) -> Parts {
        guard let s = skills[code], s.count > 0 else {
            return Parts(exposure: 0, speed: 0, accuracy: 0)
        }
        // Each is "how close to what is asked", capped at met.  Accuracy is the ratio of
        // the allowance to what was used of it, the same shape as speed -- it used to
        // count down from a clean sheet, which meant it only ever reached 1 at no
        // corrections at all, while `learned` was happy with a tenth of them.  The two
        // disagreeing made the screen call a chord short on accuracy that the ladder
        // considered done with.
        return Parts(
            exposure: min(1, Double(s.count) / Double(max(1, options.minSamples))),
            speed: min(1, Double(options.targetMs) / Double(max(1, s.medianMs))),
            accuracy: s.errorRate <= options.maxErrorRate
                ? 1 : options.maxErrorRate / max(0.0001, s.errorRate))
    }

    /// A single number for ordering the weakest first, in `0...1`.
    public func confidence(_ code: UInt16) -> Double { parts(code).confidence }

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
