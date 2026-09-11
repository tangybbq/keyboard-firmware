import Foundation

/// Typing a known target in Orsy, and scoring how it went.
///
/// Different from `DrillSession` in one decisive way: **the judge is the text, not the
/// stroke.**  Most words divide into strokes in more than one way, and a writer who
/// chooses a different division from the table's is right.  So the strokes are run
/// through the theory and a port of the firmware's output stage -- the spacing rule, the
/// pending capital, undo -- and what they put on the screen is compared with the target.
/// The table's division is what the hint shows, and using more strokes than it is
/// reported, not marked wrong.
///
/// A wrong stroke is diagnosed by Series against the hinted one, so the screen can say
/// "coda: s+t is l" rather than paint a word red.

/// The output stage, as the firmware runs it.  See `bbq-orsy/src/output.rs`.
struct OrsyOutput {
    private(set) var recent: [Character] = []
    /// Characters typed by each stroke, oldest first.
    private var strokes: [Int] = []
    private var pendingSpace = false
    private var pendingCap = false

    /// Type a syllable, returning the characters it puts on the screen.
    mutating func stroke(_ t: OrsyTranslation) -> String {
        var out = ""
        if pendingSpace && t.spaceBefore && !t.text.isEmpty {
            out.append(" ")
        }
        for ch in t.text {
            if pendingCap && ch.isLetter {
                pendingCap = false
                out.append(contentsOf: ch.uppercased())
            } else {
                out.append(ch)
            }
        }
        pendingSpace = t.spaceAfter
        recent.append(contentsOf: out)
        strokes.append(out.count)
        return out
    }

    mutating func space() -> String {
        recent.append(" ")
        strokes.append(1)
        pendingSpace = false
        return " "
    }

    mutating func capNext() { pendingCap = true }

    /// Take back the last stroke, returning how many characters go.
    mutating func undo() -> Int {
        guard let count = strokes.popLast() else { return 0 }
        recent.removeLast(min(count, recent.count))
        pendingCap = false
        // The firmware restores the space that was pending before the stroke; near
        // enough here to say a space is owed again if the record now ends a word.
        pendingSpace = recent.last.map { $0 != " " } ?? false
        return count
    }

    /// A character erased by a backspace through the Dosh escape.
    mutating func erase() {
        guard let erased = recent.popLast() else { return }
        if erased == " " { pendingSpace = true }
        if let last = strokes.indices.last { strokes[last] = max(0, strokes[last] - 1) }
    }

    /// Something typed through the Dosh escape.
    mutating func typed(_ text: String) {
        recent.append(contentsOf: text)
        strokes.append(text.count)
    }
}

/// One stroke of the table's division of a target.
public struct OrsyDrillUnit: Equatable {
    public let left: UInt16
    public let right: UInt16
    /// The text the stroke puts on the screen, the space before it included.
    public let text: String
    /// Where in the target its text starts.
    public let offset: Int
}

/// A target, and the strokes the table would use for it.
public struct OrsyDrillTarget {
    public let text: String
    public let units: [OrsyDrillUnit]

    /// Divide a line of words the way the table does.  A word the table lacks gets no
    /// units, and the drill still judges it by text.
    public init(text: String, words: OrsyWords, theory: OrsyTheory) {
        self.text = text
        var units = [OrsyDrillUnit]()
        var output = OrsyOutput()
        var offset = 0
        for word in text.split(separator: " ", omittingEmptySubsequences: false) {
            guard let entry = words.word(String(word)) else {
                // Skip the word and the space after it, keeping the offsets right.
                offset += word.count + 1
                output = OrsyOutput()
                _ = output.stroke(OrsyTranslation(
                    text: String(word), spaceBefore: true, spaceAfter: true, rules: 0,
                    patterns: OrsyPatterns(onset: "", second: "", vowel: "", coda: "")))
                continue
            }
            for (left, right) in entry.strokes {
                guard let t = theory.translate(left: left, right: right) else { continue }
                let typed = output.stroke(t)
                units.append(OrsyDrillUnit(left: left, right: right, text: typed, offset: offset))
                offset += typed.count
            }
        }
        self.units = units
    }
}

public enum OrsyDrillEvent: Equatable {
    case correct
    /// Typed something the target did not want here, with the Series that differ from
    /// the hinted stroke, or all of them when nothing matched.
    case wrong(expected: String, got: String, series: [String])
    case dead
    case correction
    case ignored
}

public struct OrsyDrillStats {
    public var strokes = 0
    /// Strokes that spelled text, right or wrong.
    public var textStrokes = 0
    public var correct = 0
    public var wrong = 0
    public var corrections = 0
    public var dead = 0
    public var characters = 0
    /// Keys pressed over all strokes, for keys per stroke.
    public var keys = 0
    /// Spread by key count: the sum of spreads and the number of strokes at each size.
    public var spreadByKeys: [Int: (total: UInt32, count: Int)] = [:]
    public var elapsedMs: UInt32 = 0

    public var wordsPerMinute: Double {
        guard elapsedMs > 0 else { return 0 }
        return Double(characters) / 5.0 * 60_000.0 / Double(elapsedMs)
    }

    public var strokesPerMinute: Double {
        guard elapsedMs > 0 else { return 0 }
        return Double(strokes) * 60_000.0 / Double(elapsedMs)
    }

    public var keysPerStroke: Double {
        strokes > 0 ? Double(keys) / Double(strokes) : 0
    }

    public var accuracy: Double {
        guard strokes > 0 else { return 1 }
        return max(0, Double(strokes - wrong - corrections - dead) / Double(strokes))
    }

    /// The mean spread for strokes of a size, where any were typed.
    public func meanSpread(keys: Int) -> Double? {
        guard let s = spreadByKeys[keys], s.count > 0 else { return nil }
        return Double(s.total) / Double(s.count)
    }
}

public final class OrsyDrillSession {
    public let target: OrsyDrillTarget
    private let theory: OrsyTheory
    private let layouts: Layouts
    private var output = OrsyOutput()

    public private(set) var typed = ""
    public private(set) var events: [OrsyDrillEvent] = []
    public private(set) var stats = OrsyDrillStats()
    public private(set) var divergedAt: Int?
    public private(set) var stumbled = false
    /// What the last wrong stroke got wrong, in words, for the screen.
    public private(set) var diagnosis: String?
    private var firstStrokeMs: UInt32?

    public init(target: OrsyDrillTarget, theory: OrsyTheory, layouts: Layouts) {
        self.target = target
        self.theory = theory
        self.layouts = layouts
    }

    public var onTrack: Bool { target.text.hasPrefix(typed) }
    public var cursor: Int { typed.count }
    public var finished: Bool { typed == target.text }

    /// The stroke the table wants next, if the typing is at the start of one.
    public var wantedStroke: OrsyDrillUnit? {
        let at = divergedAt ?? cursor
        return target.units.first { $0.offset == at }
    }

    public enum Control { case next, restart }

    /// Enter through the Dosh escape moves on; the null chord through it starts over.
    public func control(for stroke: Stroke) -> Control? {
        guard case .dosh(let code) = stroke.outcome,
            let entry = layouts.chord(code, variant: "dosh")
        else { return nil }
        if entry.action.kind == "release" { return .restart }
        if entry.action.kind == "key" && entry.action.key == "ReturnEnter" { return .next }
        return nil
    }

    public func feed(_ stroke: Stroke) {
        stats.strokes += 1
        if firstStrokeMs == nil { firstStrokeMs = stroke.firstKeyMs }
        stats.elapsedMs = stroke.timeMs - (firstStrokeMs ?? stroke.timeMs)
        let keys = (stroke.left | stroke.right).nonzeroBitCount
        stats.keys += keys
        var s = stats.spreadByKeys[keys] ?? (0, 0)
        s.total += stroke.spreadMs
        s.count += 1
        stats.spreadByKeys[keys] = s

        switch stroke.outcome {
        case .text(let t):
            stats.textStrokes += 1
            let wanted = wantedStroke
            append(output.stroke(t), stroke: stroke, translation: t, wanted: wanted)
        case .space:
            append(output.space(), stroke: stroke, translation: nil, wanted: wantedStroke)
        case .capNext:
            output.capNext()
            events.append(.ignored)
        case .undo:
            let count = output.undo()
            takeBack(count)
        case .dosh(let code):
            let entry = layouts.chord(code, variant: "dosh")
            if entry?.action.kind == "key" && entry?.action.key == "DeleteBackspace" {
                output.erase()
                takeBack(1)
            } else if let types = entry?.action.types {
                output.typed(types)
                if [".", "!", "?"].contains(types) { output.capNext() }
                append(types, stroke: stroke, translation: nil, wanted: wantedStroke)
            } else {
                events.append(.ignored)
            }
        case .doshToggle:
            events.append(.ignored)
        case .dead:
            stats.dead += 1
            events.append(.dead)
            stumbled = true
            diagnosis = "not a syllable"
        }
    }

    private func takeBack(_ count: Int) {
        stats.corrections += 1
        events.append(.correction)
        let n = min(count, typed.count)
        if n > 0 {
            typed.removeLast(n)
            stats.characters = max(0, stats.characters - n)
        }
        if onTrack { divergedAt = nil }
    }

    private func append(
        _ text: String, stroke: Stroke, translation: OrsyTranslation?, wanted: OrsyDrillUnit?
    ) {
        let offset = typed.count
        typed += text
        if target.text.hasPrefix(typed) {
            stats.correct += 1
            stats.characters += text.count
            events.append(.correct)
            divergedAt = nil
            stumbled = false
            diagnosis = nil
        } else {
            stats.wrong += 1
            let chars = Array(target.text)
            let expected = offset < chars.count
                ? String(chars[offset..<min(offset + max(1, text.count), chars.count)]) : ""
            let series = diagnose(stroke, translation: translation, wanted: wanted)
            events.append(.wrong(expected: expected, got: text, series: series))
            if divergedAt == nil { divergedAt = offset }
            stumbled = true
        }
    }

    /// Which Series differ from the hinted stroke, and a sentence saying so.
    private func diagnose(_ stroke: Stroke, translation: OrsyTranslation?, wanted: OrsyDrillUnit?)
        -> [String]
    {
        guard let translation, let wanted,
            let want = theory.translate(left: wanted.left, right: wanted.right)
        else {
            diagnosis = nil
            return []
        }
        let got = translation.patterns
        let need = want.patterns
        var differing = [String]()
        var notes = [String]()
        func check(_ name: String, _ g: String, _ n: String, spell: (String) -> String) {
            guard g != n else { return }
            differing.append(name)
            let gotText = g.isEmpty ? "nothing" : spell(g)
            let needText = n.isEmpty ? "nothing" : spell(n)
            notes.append("\(name): \(gotText) for \(needText)")
        }
        let t = theory.tables
        check("onset", got.onset, need.onset) { m in t.outer.first { $0.michela == m }?.onset ?? m }
        check("second", got.second, need.second) { m in t.second.first { $0.michela == m }?.spells ?? m }
        check("vowel", got.vowel, need.vowel) { m in t.vowel.first { $0.michela == m }?.text ?? m }
        check("coda", got.coda, need.coda) { m in t.outer.first { $0.michela == m }?.coda ?? m }
        diagnosis = notes.isEmpty ? nil : notes.joined(separator: "; ")
        return differing
    }
}
