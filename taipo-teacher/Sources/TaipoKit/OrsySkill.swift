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
    /// The speed and error thresholds are the chord model's until there are Orsy medians
    /// to set them from; nothing has been typed long enough to say what a five-key stroke
    /// should cost.
    public static let defaultOptions = SkillModel.Options(minSamples: 40)

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
    mutating func fold(_ all: [Stroke], changed: Set<String>, options: SkillModel.Options) {
        guard !all.isEmpty else { return }
        sessions += 1
        strokes += all.count
        dead += all.filter { $0.outcome == .dead }.count

        let undone = Self.undone(all)
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
            let blamed = undone[i] ?? []
            for key in keys {
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
            return keys
        case .undo: return ["cmd:undo"]
        case .space: return ["cmd:space"]
        case .capNext: return ["cmd:capitalise_next"]
        case .doshToggle: return ["cmd:dosh_toggle"]
        case .dosh: return ["cmd:dosh_oneshot"]
        case .punct(let text, _): return ["punct:\(text)"]
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

    /// Which strokes were taken back, and the pattern keys to blame for each.
    ///
    /// The undo command takes back the syllable before it.  A run of escaped backspaces
    /// takes back the syllable before it if the run is at least as long as what the
    /// syllable typed.  Blame is the Series that differ from the syllable typed in its
    /// place -- the next syllable after the correction -- or all of them when every
    /// Series differs or nothing was retyped.
    static func undone(_ all: [Stroke]) -> [Int: Set<String>] {
        var out = [Int: Set<String>]()
        var lastText: Int?
        var backspaces = 0
        func takeBack(_ i: Int, at correction: Int) {
            guard case .text(let wrong) = all[i].outcome else { return }
            let retype = all[(correction + 1)...].first { $0.translation != nil }?.translation
            var blamed = Set(wrong.patterns.keys)
            if let right = retype?.patterns {
                let same = Set(
                    [
                        wrong.patterns.onset == right.onset ? "s1:\(wrong.patterns.onset)" : nil,
                        wrong.patterns.second == right.second ? "s2:\(wrong.patterns.second)" : nil,
                        wrong.patterns.vowel == right.vowel ? "s3:\(wrong.patterns.vowel)" : nil,
                        wrong.patterns.coda == right.coda ? "s4:\(wrong.patterns.coda)" : nil,
                    ].compactMap { $0 })
                let differing = blamed.subtracting(same)
                if !differing.isEmpty { blamed = differing }
            }
            out[i] = blamed
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
