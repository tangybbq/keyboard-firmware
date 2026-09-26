import Foundation

/// How well each Orsy pattern is known, derived from the writer's own logs.
///
/// The counterpart of `SkillModel` for a syllabic layout.  Nobody learns 170,000
/// strokes; they learn some seventy patterns and a dozen rules, so **the unit of skill is
/// the pattern**: every stroke is split into its Series and each pattern present gets the
/// sample.  Timing is the gap from the previous stroke, attributed to every pattern in
/// the stroke -- a stroke with one unlearned pattern is slow because of that one, and the
/// attribution to the learned ones is noise the window absorbs.
///
/// Blame for a stroke taken back goes to **the Series that differ** between the undone
/// stroke and the one typed in its place; when every Series differs, or nothing was
/// retyped, the whole stroke was wrong and every pattern in it takes it.  A stroke is
/// taken back by the undo command, or by a run of backspaces through the Dosh escape at
/// least as long as what it typed.
///
/// The keys are `s1:FC` for an onset, `s2:R` a second character, `s3:uia` a vowel,
/// `s4:N` a coda, `rule:mirrored` a composition rule, and `cmd:undo` a command; the same
/// keys `sync-layouts.py` records in the layout history.  Everything else -- the window,
/// the gate, the sticky `everReached` -- is `ChordSamples`, shared with the chord model
/// for the reasons given there.

/// What the logs say about one pattern.
public struct PatternSkill: Equatable, Sendable {
    public let name: String
    public let count: Int
    public let everReached: Bool
    public let medianMs: UInt32
    public let deleted: Int
    public let recent: Int
    /// Its review box, once it has been reached; nil before, and for anything the
    /// ladder does not teach.
    public let retention: Retention?

    public init(
        name: String, count: Int, medianMs: UInt32, deleted: Int, recent: Int? = nil,
        everReached: Bool? = nil, retention: Retention? = nil
    ) {
        self.name = name
        self.count = count
        self.medianMs = medianMs
        self.deleted = deleted
        self.recent = recent ?? count
        self.everReached = everReached ?? (count > 0 && deleted * 10 <= count)
        self.retention = retention
    }

    public var errorRate: Double {
        recent > 0 ? min(1, Double(deleted) / Double(recent)) : 0
    }
}

/// Whether a reached pattern is still there the next day, and when to look again.
///
/// **Reaching an item is a day's work, and that is the trouble.**  The gate is forty uses
/// with few taken back, and a focus item is worked six times a line, so it passes in the
/// sitting it was introduced in.  After that the model has nothing to say about it that
/// the drilling did not put there: the window is full of warmed-up uses, so the item
/// looks *better* than the ones learned a week ago and loses the polishing place to them,
/// and from then on it gets what English happens to ask for.  On the logs up to
/// 2026-09-25, Series 2 `e` was struck 53 times on the 16th and 23 on the 17th, twice on
/// the 18th, and not once in the seven days after; Series 2 `o` 36 times on the 17th and
/// then once in eight days.  And the day an item does come back, its first uses are the
/// slow ones: Series 2 `l` took 3569ms over its first five on the 23rd and 2620ms over
/// the rest of the day.
///
/// So each reached item sits in a **box**, Leitner's way: the box says how many days may
/// pass before the item is looked at again, doubling from one to thirty-two.  **The look
/// is the first `coldUses` uses on a day**, which is the only moment the logs can see
/// recall rather than repetition; after them the hand is warm, and anything later that
/// day says nothing new.  Clean and not badly slow, and the item was due, it moves up a
/// box; clean but not yet due, nothing changes -- meeting it early does no harm and
/// earns nothing.  A use taken back, or a median more than `slowFactor` times the item's
/// warmed-up one, is a **lapse**, and the item goes back to the first box, due tomorrow.
///
/// **Twice the warmed-up median, not the one and a half first thought of.**  Replaying
/// the logs to the 25th with each, one and a half called 118 lapses of which 70 were for
/// speed alone, and on inspection those were mostly the opening of a session, where
/// everything is slow for a stroke or two -- `e␣` failing on a Tuesday is not the vowel
/// being forgotten.  Twice called 68, 13 of them for speed, and the items it still
/// catches are the ones that were visibly lost.  A use with no timing (the first of a
/// session, or after a pause past the cut-off) is judged on whether it was taken back
/// alone.
///
/// Worked out as the logs are folded, like the rest, from the day each file is named
/// for; the cache stays a cache because the day comes from the file, not the clock.  The
/// clock is only asked at the end, by `due(on:)`: a day that ended with fewer than
/// `coldUses` looks is graded on what it had, once it is over.
public struct Retention: Codable, Equatable, Sendable {
    /// How many days each box waits, first box first.
    public static let intervals = [1, 2, 4, 8, 16, 32]
    /// How many of a day's first uses are the look.
    static let coldUses = 3
    /// How much slower than warmed up a look may be before it is a lapse.
    static let slowFactor: UInt64 = 2

    /// Which box, as an index into `intervals`.
    public private(set) var box = 0
    /// The day the current wait runs from: when it was reached, last moved up, or lapsed.
    public private(set) var since: Int
    /// How many times it has gone back to the first box.
    public private(set) var lapses = 0
    /// Whether the last look was a lapse.
    public private(set) var lapsed = false

    /// The day whose first uses are being gathered, and what they were.
    var day: Int?
    var cold: [Use] = []
    /// Whether `day` has been graded; its later uses are warm.
    var graded = false
    /// The median the day's looks are measured against, taken before the first of them.
    var warmMs: UInt32 = .max

    struct Use: Codable, Equatable, Sendable {
        var gap: UInt32?
        var bad: Bool
    }

    /// Reached on `day`: in the first box, due the day after.  `box` is for tests that
    /// want one further up without typing their way there.
    init(reached day: Int, box: Int = 0) {
        since = day
        self.box = box
    }

    /// Whether the ladder teaches `key`, and so whether it gets a box.  Commands, marks
    /// and whole strokes are counted but not reviewed.
    static func tracks(_ key: String) -> Bool {
        key.hasPrefix("s1:") || key.hasPrefix("s2:") || key.hasPrefix("s3:")
            || key.hasPrefix("s4:") || key.hasPrefix("rule:")
    }

    /// How many days this box waits.
    public var interval: Int { Self.intervals[box] }

    /// Whether the item wants looking at on `today`: its wait is over, and today's look
    /// has not yet happened.  A look in progress -- one or two uses so far -- is still due.
    public func due(on today: Int) -> Bool { overdue(on: today) != nil }

    /// How many days past due the item is on `today`: zero on the day its wait runs out,
    /// nil if it is not due.
    public func overdue(on today: Int) -> Int? {
        let settled = settled(on: today)
        let late = today - settled.since - settled.interval
        return late >= 0 ? late : nil
    }

    /// This record with any earlier day's unfinished look graded.
    public func settled(on today: Int) -> Retention {
        var out = self
        if let day, day < today { out.grade() }
        return out
    }

    /// One use on `day`.  `warm` is the item's median as it stands, asked for only when
    /// a new day's look begins.
    mutating func note(day: Int, gap: UInt32?, bad: Bool, warm: () -> UInt32) {
        // The day it was reached or last lapsed is not a look: the hand is warm from
        // the practice that did it.
        guard day > since else { return }
        if self.day != day {
            grade()
            self.day = day
            cold = []
            graded = false
            warmMs = warm()
        }
        guard !graded else { return }
        cold.append(Use(gap: gap, bad: bad))
        if cold.count >= Self.coldUses { grade() }
    }

    mutating func grade() {
        guard let day, !graded, !cold.isEmpty else { return }
        graded = true
        let timed = cold.compactMap(\.gap)
        let slow =
            warmMs != .max && !timed.isEmpty
            && UInt64(SkillModel.median(timed)) > UInt64(warmMs) * Self.slowFactor
        if slow || cold.contains(where: \.bad) {
            box = 0
            since = day
            lapses += 1
            lapsed = true
        } else if day - since >= interval {
            box = min(box + 1, Self.intervals.count - 1)
            since = day
            lapsed = false
        }
    }
}

/// The skill model for Orsy: one entry per pattern the logs have seen.
public struct OrsySkillModel: Sendable {
    public let skills: [String: PatternSkill]
    /// Sessions with Orsy strokes in them.
    public let sessions: Int
    /// Sessions passed over as recorded against Orsy tables nothing can interpret.
    public let skipped: Int
    /// Strokes seen in all.
    public let strokes: Int
    /// Strokes that were neither a syllable nor a command.
    public let dead: Int
    public let options: SkillModel.Options

    public init(
        skills: [String: PatternSkill], sessions: Int, skipped: Int = 0, strokes: Int,
        dead: Int = 0, options: SkillModel.Options = OrsySkillModel.defaultOptions
    ) {
        self.skills = skills
        self.sessions = sessions
        self.skipped = skipped
        self.strokes = strokes
        self.dead = dead
        self.options = options
    }

    public func skill(_ name: String) -> PatternSkill? { skills[name] }

    /// The key measuring one whole stroke, as against the patterns in it.
    ///
    /// The unit of skill here is the pattern, deliberately: seventy of those and a dozen
    /// rules can be taught and measured, where the strokes that use them run to six
    /// figures.  But a stroke is a *combination*, and a combination goes on being new
    /// long after its parts are old -- `ion` is Series 2 `i`, the closing `o` and a coda
    /// `n`, each of them familiar from a dozen other words, and the stroke still has to
    /// be found on the board the first time it comes up.  So each stroke is counted too,
    /// for the one question the patterns cannot answer: has this been made before.
    ///
    /// Nothing gates on these -- the ladder and the drill ranking read the pattern keys,
    /// which are the ones the lesson plan names.  They exist for the hint.
    public static func strokeKey(left: UInt16, right: UInt16) -> String {
        "stroke:\(String(left, radix: 16)),\(String(right, radix: 16))"
    }

    /// How many times this exact stroke has been made.
    public func strokeUses(left: UInt16, right: UInt16) -> Int {
        skills[Self.strokeKey(left: left, right: right)]?.count ?? 0
    }

    /// One stroke as a single number, for a set of them.
    public static func strokeId(left: UInt16, right: UInt16) -> UInt32 {
        UInt32(left) << 16 | UInt32(right)
    }

    /// Every stroke the logs have seen made, so a drill can tell a stroke that has been
    /// found on the board from one that has not.
    public var madeStrokes: Set<UInt32> {
        var out = Set<UInt32>()
        for (name, s) in skills where s.count > 0 && name.hasPrefix("stroke:") {
            let parts = name.dropFirst("stroke:".count).split(separator: ",")
            guard parts.count == 2, let l = UInt16(parts[0], radix: 16),
                let r = UInt16(parts[1], radix: 16)
            else { continue }
            out.insert(Self.strokeId(left: l, right: r))
        }
        return out
    }

    /// The ladder's gate: typed often enough, and not often taken back.  See
    /// `SkillModel.reached` for why speed is left out.
    public func reached(_ name: String) -> Bool {
        guard let s = skills[name] else { return false }
        return s.everReached && s.count >= options.minSamples
    }

    /// Often enough, quickly enough, and cleanly enough.
    public func learned(_ name: String) -> Bool {
        guard let s = skills[name] else { return false }
        return s.count >= options.minSamples && s.medianMs <= options.targetMs
            && s.errorRate <= options.maxErrorRate
    }

    /// Whether a reached pattern is due a look on `today`.  See `Retention`.
    public func due(_ name: String, on today: Int) -> Bool {
        skills[name]?.retention?.due(on: today) ?? false
    }

    /// How many days past due a pattern is, or nil if it is not due.
    public func overdue(_ name: String, on today: Int) -> Int? {
        skills[name]?.retention?.overdue(on: today)
    }

    /// Whether a pattern's last look was a lapse, as of `today`.
    public func lapsed(_ name: String, on today: Int) -> Bool {
        skills[name]?.retention?.settled(on: today).lapsed ?? false
    }

    public func parts(_ name: String) -> SkillModel.Parts {
        guard let s = skills[name], s.count > 0 else {
            return SkillModel.Parts(exposure: 0, speed: 0, accuracy: 0)
        }
        return SkillModel.Parts(
            exposure: min(1, Double(s.count) / Double(max(1, options.minSamples))),
            speed: min(1, Double(options.targetMs) / Double(max(1, s.medianMs))),
            accuracy: s.errorRate <= options.maxErrorRate
                ? 1 : options.maxErrorRate / max(0.0001, s.errorRate))
    }

    public func confidence(_ name: String) -> Double { parts(name).confidence }

    /// The thresholds an Orsy pattern is held to.
    ///
    /// The chord model's twelve uses is a sensible gate for one chord that types one
    /// letter.  An Orsy pattern is bigger -- a shape is a movement with two readings, a
    /// rule is a conditional behaviour -- and most of them have a Dosh habit to overwrite,
    /// so twelve uses is recognition and not production: with a focus item worked six
    /// times a line, it passed in half a block.  Forty is about a block of deliberate
    /// practice, which is the unit this ladder moves in.
    ///
    /// **The pause is four seconds, not the chord model's two.**  Two seconds is eight
    /// times what a Dosh chord costs, which leaves the cut-off far outside the skill and
    /// catching only real interruptions -- reading the next line, being spoken to.  An
    /// Orsy stroke costs about 1.8 seconds while it is being learned, so the same two
    /// seconds sits *inside* the distribution and throws away most of the right-hand half
    /// of it.  Measured over 4,733 strokes, moving the cut-off from 2000ms to 3000ms took
    /// the median of the medians from 1288ms to 1744ms: a third of the measurement was
    /// being discarded as thinking time.  Worse, it was being discarded unevenly -- what
    /// survived the cut was the fast tail of every pattern, so the thirty-three measured
    /// items came out inside a 1223-1428ms band, and a confidence ranking taken over a
    /// band that narrow is ranking noise.  Past four seconds the curve flattens (1827ms,
    /// then 1923 at six and 2030 at ten), which is the knee: the distribution is under
    /// it, and what is above is interruption.  A trainer must not mistake stopping to
    /// think for being slow, but it must not mistake being slow for stopping to think
    /// either, and at this stage of Orsy nearly every stroke is thought about.
    ///
    /// **The speed target is 700ms, which is now derived rather than inherited.**  It
    /// arrived as the chord model's number with a note saying nothing had been typed long
    /// enough to set it from; there is now, and it lands in the same place.  An Orsy
    /// stroke writes 2.74 characters, draw-weighted over the word table, and a Dosh chord
    /// measures at a 245ms use-weighted median over 214,000 of them, so the letters one
    /// stroke replaces cost about 671ms to write the other way.  That is what `learned`
    /// should mean here: not that a stroke is fast in the abstract, but that it is worth
    /// more than the chords it stands in for.
    ///
    /// The error threshold is still the chord model's.
    public static let defaultOptions = SkillModel.Options(minSamples: 40, pauseMs: 4000)

    /// Replay every log in a directory and measure each pattern.  The whole history,
    /// every time; `SkillStore.orsyModel` is the incremental one.
    public static func build(
        logDirectory: URL, layouts: Layouts,
        options: SkillModel.Options = OrsySkillModel.defaultOptions,
        history: LayoutHistory = LayoutHistory.bundled()
    ) -> OrsySkillModel {
        var collector = SkillCollector()
        for file in SkillModel.logFiles(in: logDirectory) {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            collector.fold(
                text: text, layouts: layouts, options: SkillModel.Options(),
                orsyOptions: options, history: history, day: LogDay.number(of: file))
        }
        return collector.orsyModel(options: options)
    }
}

/// What a replay needs to judge a line the way the drill did: the word table, the
/// theory, and the tables under both.
///
/// Built once and carried, because `OrsyWords.bundled()` decodes ten thousand entries
/// and a log holds thousands of lines.
struct OrsyJudge {
    let words: OrsyWords
    let theory: OrsyTheory
    let layouts: Layouts

    private static var cachedWords: OrsyWords?

    static func make(_ layouts: Layouts) -> OrsyJudge? {
        guard let tables = layouts.orsy else { return nil }
        if cachedWords == nil { cachedWords = try? OrsyWords.bundled() }
        guard let words = cachedWords else { return nil }
        return OrsyJudge(words: words, theory: OrsyTheory(tables), layouts: layouts)
    }

    /// Which of `strokes` the drill would have called wrong, given what it asked for.
    func wrong(_ strokes: ArraySlice<Stroke>, target text: String) -> Set<Int> {
        let target = OrsyDrillTarget(text: text, words: words, theory: theory)
        let session = OrsyDrillSession(target: target, theory: theory, layouts: layouts)
        var out = Set<Int>()
        var seen = 0
        for i in strokes.indices {
            session.feed(strokes[i])
            if session.stats.wrong > seen { out.insert(i) }
            seen = session.stats.wrong
        }
        return out
    }
}

/// The Orsy half of `SkillCollector`: samples by pattern, and the counts.
struct OrsySamples: Codable, Equatable {
    var patterns: [String: ChordSamples] = [:]
    /// The review box of every reached pattern the ladder teaches.
    var retention: [String: Retention] = [:]
    var sessions = 0
    var skipped = 0
    var strokes = 0
    var dead = 0

    /// Fold one session's strokes in.
    ///
    /// `changed` is the set of pattern keys whose meaning has changed since the session
    /// was logged; a stroke using one is not evidence about the pattern that lives there
    /// now, and is passed over the way a changed chord is.
    /// `lines` is where each drill line began, as (what it asked for, first stroke).
    /// `day` is the day the session was typed on, for the review boxes; without one they
    /// are left as they are.
    mutating func fold(
        _ all: [Stroke], lines: [(text: String, from: Int)] = [], judge: OrsyJudge? = nil,
        changed: Set<String>, options: SkillModel.Options, day: Int? = nil
    ) {
        guard !all.isEmpty else { return }
        sessions += 1
        strokes += all.count
        dead += all.filter { $0.outcome == .dead }.count

        // What the drill asked for, where the log says.  A stroke the drill called wrong
        // is not evidence about the patterns it spelled, whether or not it was taken
        // back: before the log carried the target, only a correction could reveal one,
        // and anything let stand counted as clean practice.
        var missed = Set<Int>()
        if let judge {
            for (n, line) in lines.enumerated() {
                let to = n + 1 < lines.count ? lines[n + 1].from : all.count
                guard line.from < to else { continue }
                missed.formUnion(judge.wrong(all[line.from..<to], target: line.text))
            }
        }

        let corrections = Self.undone(all)
        var previousMs: UInt32?
        for (i, stroke) in all.enumerated() {
            let keys = Self.keys(stroke)
            guard !keys.isEmpty else {
                previousMs = stroke.outcome == .dead ? nil : stroke.timeMs
                continue
            }
            if keys.contains(where: changed.contains) {
                previousMs = nil
                continue
            }
            var gap: UInt32?
            if let previous = previousMs, stroke.timeMs >= previous {
                let d = stroke.timeMs - previous
                if d <= options.pauseMs { gap = d }
            }
            // A stroke the drill called wrong says nothing about what it spelled.
            if missed.contains(i) {
                previousMs = stroke.timeMs
                continue
            }
            let skipped = corrections.skip[i] ?? []
            let blamed = corrections.blame[i] ?? []
            for key in keys where !skipped.contains(key) {
                let deleted = blamed.contains(key)
                if let day, Retention.tracks(key) {
                    retention[key]?.note(day: day, gap: gap, bad: deleted) {
                        SkillModel.median(patterns[key]?.gaps ?? [])
                    }
                }
                patterns[key, default: ChordSamples()]
                    .note(gap: gap, deleted: deleted, options: options)
                if let day, retention[key] == nil, Retention.tracks(key),
                    patterns[key]?.everReached == true
                {
                    retention[key] = Retention(reached: day)
                }
            }
            previousMs = stroke.timeMs
        }
    }

    /// The pattern keys a stroke gives a sample to.
    static func keys(_ stroke: Stroke) -> [String] {
        switch stroke.outcome {
        case .text(let t):
            var keys = t.patterns.keys
            for (name, flag) in OrsyTheory.Rules.byName where t.rules & flag != 0 {
                keys.append("rule:\(name)")
            }
            keys.append(OrsySkillModel.strokeKey(left: stroke.left, right: stroke.right))
            return keys
        case .undo: return ["cmd:undo"]
        case .space: return ["cmd:space"]
        case .capNext: return ["cmd:capitalise_next"]
        case .join: return ["cmd:join"]
        case .allCaps: return ["cmd:all_caps"]
        case .capPrevious: return ["cmd:capitalise_previous"]
        case .doshToggle: return ["cmd:dosh_toggle"]
        case .dosh: return ["cmd:dosh_oneshot"]
        case .punct(let mark): return ["punct:\(mark.text)"]
        case .dead: return []
        }
    }

    /// How many characters a syllable put on the screen, near enough: its text and the
    /// space before it.  The output stage's spacing rule is not replayed here; a run of
    /// backspaces is judged against this, and one character of slack does not change
    /// which stroke it takes back.
    static func typed(_ t: OrsyTranslation) -> Int {
        t.text.count + (t.spaceBefore ? 1 : 0)
    }

    /// What a correction is evidence about.
    struct Corrections {
        /// Keys of the stroke taken back which must not be counted at all.
        var skip: [Int: Set<String>] = [:]
        /// Keys of a stroke that needed a second go.
        var blame: [Int: Set<String>] = [:]
    }

    /// Which strokes were taken back, and what that says about which patterns.
    ///
    /// The undo command takes back the syllable before it.  A run of escaped backspaces
    /// takes back the syllable before it if the run is at least as long as what the
    /// syllable typed.
    ///
    /// **A wrong stroke is not evidence about the patterns it happened to spell.**  It
    /// used to be counted as one: every Series of the stroke taken back got a use, and
    /// the ones differing from the retype got a use taken back, so a pattern collected a
    /// record out of strokes nobody meant to make.  On a real log, Series 2 `s` reached
    /// its own lesson carrying sixty-one uses at a 39% error rate, every one of them a
    /// misfire aimed elsewhere -- a hand reaching for the Series 1 `r` on the pinky,
    /// striking the index, and spelling `set` where `ret` was wanted.  The item had never
    /// been taught, and arrived looking half failed.
    ///
    /// So the Series that differ are skipped where they lie and counted against the
    /// *retype* instead: the evidence is about the pattern that was wanted and missed,
    /// not the one that turned up uninvited.  The Series that match are left alone, since
    /// they were struck correctly whatever happened around them.  A stroke taken back and
    /// then made again identically is the one case still blamed where it lies: it was
    /// meant, and taken back anyway.
    static func undone(_ all: [Stroke]) -> Corrections {
        var out = Corrections()
        var lastText: Int?
        var backspaces = 0
        func takeBack(_ i: Int, at correction: Int) {
            guard case .text(let wrong) = all[i].outcome else { return }
            let next = all.indices[(correction + 1)...].first { all[$0].translation != nil }
            let right = next.flatMap { all[$0].translation }?.patterns
            let w = wrong.patterns
            let series: [(String, String, String?)] = [
                ("s1", w.onset, right?.onset), ("s2", w.second, right?.second),
                ("s3", w.vowel, right?.vowel), ("s4", w.coda, right?.coda),
            ]
            var skip = Set<String>()
            var blame = Set<String>()
            for (name, mine, theirs) in series where mine != theirs {
                if !mine.isEmpty { skip.insert("\(name):\(mine)") }
                if let theirs, !theirs.isEmpty { blame.insert("\(name):\(theirs)") }
            }
            guard !skip.isEmpty || !blame.isEmpty else {
                out.blame[i] = Set(Self.keys(all[i]))
                return
            }
            // The stroke as a whole was not the one wanted, so neither the rules it used
            // nor the stroke itself are evidence that it has been made before.
            for key in Self.keys(all[i])
            where key.hasPrefix("rule:") || key.hasPrefix("stroke:") {
                skip.insert(key)
            }
            out.skip[i] = skip
            if let next, !blame.isEmpty { out.blame[next, default: []].formUnion(blame) }
        }
        for (i, stroke) in all.enumerated() {
            switch stroke.outcome {
            case .text(let t):
                lastText = i
                backspaces = 0
                _ = t
            case .undo:
                if let last = lastText { takeBack(last, at: i) }
                lastText = nil
                backspaces = 0
            case .dosh(let code) where code == 0x100:
                backspaces += 1
                if let last = lastText, case .text(let t) = all[last].outcome,
                    backspaces >= typed(t)
                {
                    takeBack(last, at: i)
                    lastText = nil
                    backspaces = 0
                }
            default:
                break
            }
        }
        return out
    }
}
