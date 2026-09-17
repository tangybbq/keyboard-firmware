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

    public init(
        name: String, count: Int, medianMs: UInt32, deleted: Int, recent: Int? = nil,
        everReached: Bool? = nil
    ) {
        self.name = name
        self.count = count
        self.medianMs = medianMs
        self.deleted = deleted
        self.recent = recent ?? count
        self.everReached = everReached ?? (count > 0 && deleted * 10 <= count)
    }

    public var errorRate: Double {
        recent > 0 ? min(1, Double(deleted) / Double(recent)) : 0
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
                orsyOptions: options, history: history)
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
    mutating func fold(
        _ all: [Stroke], lines: [(text: String, from: Int)] = [], judge: OrsyJudge? = nil,
        changed: Set<String>, options: SkillModel.Options
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
                patterns[key, default: ChordSamples()]
                    .note(gap: gap, deleted: blamed.contains(key), options: options)
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
