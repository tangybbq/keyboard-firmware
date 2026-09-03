import Foundation

/// Typing a known target, and scoring how it went.
///
/// The chords come from the keyboard's own log, so this needs no keystroke capture at all:
/// the chord stream says both *what* was typed and *how*, which is the whole reason the log
/// exists.  A drill therefore scores things a software-only trainer cannot see -- which
/// hand, whether a gram was chorded or spelled, whether the chord was struck or assembled.

/// One step of the cheapest way to type a target.
public struct DrillUnit: Equatable {
    /// The characters this step types.
    public let text: String
    /// The chord that types them.
    public let code: UInt16
    /// Where in the target it starts.
    public let offset: Int
}

/// A target, and the chord sequence a writer using the whole table would produce.
public struct DrillTarget {
    public let text: String
    public let units: [DrillUnit]

    /// Segment a target into the fewest chords that type it.
    ///
    /// Dynamic programming rather than longest-match-first, because greedy is not optimal:
    /// taking a long gram can leave a remainder that costs more than it saved.  This is the
    /// same cost model `words/ngrams.py` uses to rank grams, and it is what makes "was that
    /// gram chorded or spelled out" a question with a definite answer.
    public init(text: String, layouts: Layouts, variant: String = "taipo") {
        self.text = text
        let chars = Array(text)
        // What each chord types, longest first, so ties prefer the bigger gram.
        var byText: [String: UInt16] = [:]
        for chord in layouts.variants[variant]?.chords ?? [] {
            guard let types = chord.action.types, !types.isEmpty else { continue }
            // Several chords can type the same text -- a letter and its thumb variants.
            // Any of them counts as having typed it, so keep the first.
            if byText[types] == nil { byText[types] = chord.code }
        }
        let maxLen = byText.keys.map(\.count).max() ?? 1

        // cost[i] = fewest chords to type chars[i...]; next[i] = the unit to take there.
        var cost = [Int](repeating: Int.max, count: chars.count + 1)
        var next = [DrillUnit?](repeating: nil, count: chars.count + 1)
        cost[chars.count] = 0

        for i in stride(from: chars.count - 1, through: 0, by: -1) {
            for len in 1...min(maxLen, chars.count - i) {
                let piece = String(chars[i..<(i + len)])
                guard let code = byText[piece], cost[i + len] != Int.max else { continue }
                if cost[i + len] + 1 < cost[i] {
                    cost[i] = cost[i + len] + 1
                    next[i] = DrillUnit(text: piece, code: code, offset: i)
                }
            }
        }

        var units = [DrillUnit]()
        var i = 0
        while i < chars.count, let unit = next[i] {
            units.append(unit)
            i += unit.text.count
        }
        // A target containing something the table cannot type has no complete segmentation;
        // the units cover what it could, and the drill still scores the rest by text.
        self.units = units
    }
}

/// What happened on one chord of a drill.
public enum DrillEvent: Equatable {
    /// Typed the expected text.
    case correct
    /// Typed something the target did not want here.
    case wrong(expected: String, got: String)
    /// A chord the table has no entry for.  It types nothing, so nothing advances.
    case deadChord(code: UInt16)
    /// A backspace.  Counted as an error even when the text ends up right.
    case correction
    /// The right text, but spelled out where a chord would have typed it in one.
    case spelled(gram: String, code: UInt16)
    /// The same hand as the chord before, close enough together to have been avoidable.
    case sameHand(gapMs: UInt32)
    /// A modifier or the null chord: no text, and nothing to score.
    case ignored
}

public struct DrillStats {
    public var chords = 0
    public var correct = 0
    public var wrong = 0
    public var corrections = 0
    public var deadChords = 0
    public var spelled = 0
    public var sameHand = 0
    public var eligiblePairs = 0
    /// Backspaces taken on the same hand as the chord they were correcting.
    ///
    /// Counted apart because a correction is where alternation is easiest to drop -- the
    /// hand that made the mistake reaches for the fix -- and because it is otherwise
    /// invisible: a backspace types no character, so there is nothing in the line to mark.
    public var sameHandCorrections = 0
    /// Milliseconds from the first chord to the last.
    public var elapsedMs: UInt32 = 0

    /// Characters correctly typed, which is what words per minute is computed from.
    public var characters = 0

    /// Words per minute, on the standard five-characters-to-a-word convention.
    ///
    /// This is the number that means something to a person and can be compared against
    /// every other typing measurement in the world.  Chords per minute is the more
    /// *faithful* description of a chorded layout -- it says how much work the fingers did
    /// -- but nobody has an intuition for it, so it is kept as a secondary figure rather
    /// than made the headline.
    public var wordsPerMinute: Double {
        guard elapsedMs > 0 else { return 0 }
        return Double(characters) / 5.0 * 60_000.0 / Double(elapsedMs)
    }

    public var chordsPerMinute: Double {
        guard elapsedMs > 0 else { return 0 }
        return Double(chords) * 60_000.0 / Double(elapsedMs)
    }

    /// Everything that went wrong, over everything typed.
    public var accuracy: Double {
        guard chords > 0 else { return 1 }
        let bad = wrong + corrections + deadChords
        return max(0, Double(chords - bad) / Double(chords))
    }

    public var sameHandRate: Double {
        guard eligiblePairs > 0 else { return 0 }
        return Double(sameHand) / Double(eligiblePairs)
    }
}

/// One run at a target.
public final class DrillSession {
    public let target: DrillTarget
    private let layouts: Layouts

    /// A same-hand pair counts only when the chords are closer than this.  After a pause
    /// either hand is equally correct, and a trainer must never penalise stopping to think.
    public var alternationWindowMs: UInt32 {
        get { alternation.windowMs }
        set { alternation.windowMs = newValue }
    }

    private let alternation = AlternationTracker()

    public private(set) var typed = ""
    public private(set) var events: [DrillEvent] = []
    public private(set) var stats = DrillStats()
    /// Target offsets typed by a chord that stayed on the previous chord's hand.
    ///
    /// Kept per character so the fault can be shown where it happened rather than only
    /// counted: a number in a corner is not something a writer reacts to.
    public private(set) var sameHandOffsets: Set<Int> = []

    private var firstChordMs: UInt32?
    /// A run of single-character chords, for spotting a gram that was spelled out.
    private var run: [(text: String, offset: Int)] = []

    public init(target: DrillTarget, layouts: Layouts) {
        self.target = target
        self.layouts = layouts
    }

    /// Whether the text so far still matches the target.
    public var onTrack: Bool { target.text.hasPrefix(typed) }

    /// Where the typing left the target, while it is off it.
    ///
    /// Kept because the cursor is no use for this: by the time a wrong chord has landed,
    /// `typed` has already grown past the place it went wrong, so the offset has to be
    /// taken at the moment it happens.
    public private(set) var divergedAt: Int?

    /// Whether the last chord that meant anything was a mistake.
    ///
    /// A hint that has faded out because the chord is known should come back when the
    /// hand has just proved otherwise -- and a dead chord, which types nothing and moves
    /// nothing, is the clearest case there is of reaching for something that is not there.
    public private(set) var stumbled = false

    /// The chord the target wants next, if there is one to name.
    ///
    /// While off the target it is the chord that was wanted where the typing left it,
    /// which is the one worth showing.  Nil in the middle of a gram that is being spelled
    /// out a letter at a time: no single chord is what comes next there.
    public var wantedChord: DrillUnit? {
        let at = divergedAt ?? cursor
        return target.units.first { $0.offset == at }
    }

    /// How far through the target the typing has got.
    public var cursor: Int { typed.count }

    public var finished: Bool { typed == target.text }

    /// What a chord means to the app rather than to the text.
    ///
    /// A typing trainer that needs the mouse between lines is not a typing trainer, so the
    /// two chords that type nothing are given jobs: Enter moves on, and the null chord --
    /// both thumbs, which exists to release modifiers -- starts the line again.
    public enum Control { case next, restart }

    public func control(for chord: Chord) -> Control? {
        guard let entry = layouts.chord(chord.code, variant: chord.variant) else { return nil }
        if entry.action.kind == "release" { return .restart }
        if entry.action.kind == "key" && entry.action.key == "ReturnEnter" { return .next }
        return nil
    }

    public func feed(_ chord: Chord) {
        let entry = layouts.chord(chord.code, variant: chord.variant)
        stats.chords += 1
        if firstChordMs == nil { firstChordMs = chord.firstKeyMs }
        stats.elapsedMs = chord.timeMs - (firstChordMs ?? chord.timeMs)

        // Alternation, judged before anything about the text: it is technique, and applies
        // to backspaces as much as to letters.
        let sameHandHere = alternation.note(chord)
        stats.eligiblePairs = alternation.eligiblePairs
        stats.sameHand = alternation.faults
        if sameHandHere { events.append(.sameHand(gapMs: 0)) }

        guard let entry else {
            stats.deadChords += 1
            events.append(.deadChord(code: chord.code))
            stumbled = true
            return
        }

        switch entry.action.kind {
        case "key" where entry.action.key == "DeleteBackspace":
            stats.corrections += 1
            events.append(.correction)
            if sameHandHere { stats.sameHandCorrections += 1 }
            if !typed.isEmpty {
                sameHandOffsets.remove(typed.count - 1)
                typed.removeLast()
                stats.characters = max(0, stats.characters - 1)
            }
            // Backed up far enough to be on the target again, so there is no longer a
            // place where it went wrong -- but the stumble stands until something is
            // typed correctly, which is what keeps the hint up while it is needed.
            if onTrack { divergedAt = nil }
            run.removeAll()
            return
        case "oneshot", "release":
            events.append(.ignored)
            return
        default:
            break
        }

        guard let types = entry.action.types, !types.isEmpty else {
            events.append(.ignored)
            return
        }

        let offset = typed.count
        typed += types
        if sameHandHere {
            for i in offset..<(offset + types.count) { sameHandOffsets.insert(i) }
        }
        let expected = expectedText(at: offset, length: types.count)
        if target.text.hasPrefix(typed) {
            stats.correct += 1
            stats.characters += types.count
            events.append(.correct)
            divergedAt = nil
            stumbled = false
            noteRun(types, at: offset)
        } else {
            stats.wrong += 1
            events.append(.wrong(expected: expected, got: types))
            if divergedAt == nil { divergedAt = offset }
            stumbled = true
            run.removeAll()
        }
    }

    private func expectedText(at offset: Int, length: Int) -> String {
        let chars = Array(target.text)
        guard offset < chars.count else { return "" }
        return String(chars[offset..<min(offset + length, chars.count)])
    }

    /// Track runs of single characters, and report when one covers a gram that the target's
    /// own segmentation would have chorded.
    private func noteRun(_ types: String, at offset: Int) {
        guard types.count == 1 else {
            run.removeAll()
            return
        }
        if run.last.map({ $0.offset + $0.text.count }) != offset {
            run.removeAll()
        }
        run.append((types, offset))

        // Did this run just finish spelling a unit the segmentation wanted chorded?
        for unit in target.units where unit.text.count > 1 {
            let start = unit.offset
            guard let first = run.first, first.offset <= start else { continue }
            let spelled = run.drop { $0.offset < start }.map(\.text).joined()
            if spelled == unit.text {
                stats.spelled += 1
                events.append(.spelled(gram: unit.text, code: unit.code))
                run.removeAll()
                return
            }
        }
    }
}
