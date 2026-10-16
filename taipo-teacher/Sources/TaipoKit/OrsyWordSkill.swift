import Foundation

/// How fast and how cleanly each whole word is typed in Orsy, from the writer's logs.
///
/// The pattern model says whether the rules are known; this says whether the words are
/// fast.  The two come apart once the ladder is done.  Over the logs to 2026-10-12 the wait
/// before a stroke fell from 3.5 s for a shape made five times or fewer to 1.1 s for one
/// made more than 250 times, while the patterns in both were long since reached.  Speed is
/// built a word at a time, and a word is what the word ladder unlocks and drills.
///
/// A word's time runs from the end of the word before it to its own end, so it covers
/// the pause to find the first stroke, which is most of the cost, as well as the strokes
/// themselves.  It is timed only when the word before ended right there, with nothing
/// but a space between them: the first word of a session, a line, or a sentence includes
/// reading and deciding what comes next, and a mark or an escape in between belongs to
/// neither word.  A pause past the cut-off anywhere in it leaves it untimed too, though it
/// still counts as a use.
///
/// A use is **clean** when nothing in the word was taken back: no undo, no backspace
/// through the escape, no dead stroke.  A wrong stroke that opened the word and was
/// undone counts against the word typed in its place, since that was the word being
/// aimed at.

/// A word as the output stage left it on the screen, and what it cost.
struct TypedWord: Equatable {
    /// In lower case: a capital after a full stop is the same word.
    var text: String
    /// From the end of the word before to the end of this one, when it can be timed.
    var ms: UInt32?
    var clean: Bool
}

/// The words a stream of strokes typed, found by replaying the output stage.
///
/// The text has to be replayed rather than read off the strokes: where a word ends is
/// the spacing rule's decision, spread across two strokes, and an undo can reach back
/// over a space into the word before.  So this keeps a mirror of what is on the screen,
/// word by word, and takes characters off it exactly as the output stage does.  The words
/// are only handed out when the record is forgotten -- an escaped key that types no text,
/// leaving Orsy, the end of the session -- since until then any of them can still be
/// reopened.
struct OrsyWordTracker {
    let pauseMs: UInt32
    private var output = OrsyOutput()
    private var entries: [Entry] = []
    /// The words handed out so far, in the order they were typed.
    private(set) var finished: [TypedWord] = []
    /// The commit time of the stroke before, for the pause rule.
    private var lastMs: UInt32?
    /// A pause has happened since the last letter, so the word the next one lands in
    /// cannot be timed.
    private var pendingPause = true
    /// Something was taken back past the end of the last word, or went wrong between
    /// words, so the next word to start was not got right first time.
    private var dirtyNext = false

    private enum Entry {
        case word(Word)
        /// Spaces and marks between words.
        case gap(String)
    }

    private struct Word {
        var text: String
        /// When the word before it ended, if it was timed from there.
        var startMs: UInt32?
        var endMs: UInt32
        var clean: Bool
        var paused: Bool
        /// A stroke in it means something else now; see `OrsySamples.fold`.
        var tainted: Bool
    }

    /// For the strokes played through the Dosh escape.
    private let layouts: Layouts?

    init(pauseMs: UInt32, layouts: Layouts?) {
        self.pauseMs = pauseMs
        self.layouts = layouts
    }

    /// One stroke.  `tainted` says the tables have changed under it since it was logged,
    /// so the word it lands in is not evidence about any word.
    mutating func feed(_ stroke: Stroke, tainted: Bool = false) {
        if let last = lastMs, stroke.timeMs >= last, stroke.timeMs - last <= pauseMs {
            // In time.
        } else {
            pendingPause = true
        }
        lastMs = stroke.timeMs
        let ms = stroke.timeMs
        switch stroke.outcome {
        case .text(let t):
            type(output.stroke(t), at: ms, tainted: tainted)
        case .punct(let mark):
            type(output.mark(mark), at: ms, tainted: tainted)
        case .space:
            type(output.space(), at: ms, tainted: tainted, closing: true)
        case .undo:
            let n = output.undo()
            if n > 0 { erase(n) } else { slip() }
        case .capNext: output.prefix(.capNext)
        case .join: output.prefix(.join)
        case .allCaps: output.prefix(.allCaps)
        case .uncap: output.prefix(.uncap)
        case .capPrevious, .uncapPrevious:
            // Retyped in place, the same length; the words are compared in lower case.
            _ = stroke.outcome == .capPrevious ? output.capPrevious(1) : output.uncapPrevious(1)
        case .dead:
            slip()
        case .dosh(let code):
            // Read as `OrsyDrillSession` reads it: a backspace, some characters, or a key
            // that types none and leaves the record saying nothing about the cursor.
            let entry = layouts?.chord(code, variant: "dosh")
            if entry?.action.kind == "key" && entry?.action.key == "DeleteBackspace" {
                output.erase()
                erase(1)
            } else if let types = entry?.action.types {
                type(types.map { output.escaped($0) }.joined(), at: ms, tainted: tainted)
            } else if entry?.action.kind == "key" {
                forget()
            }
        case .doshToggle:
            // Leaving Orsy: nothing typed before can be reached again.
            forget()
        }
    }

    /// Hand out every word on the record and start a new one.
    mutating func forget() {
        output.forget()
        for entry in entries {
            guard case .word(let w) = entry, !w.tainted, !w.text.isEmpty else { continue }
            let ms: UInt32? =
                w.paused ? nil : w.startMs.flatMap { w.endMs >= $0 ? w.endMs - $0 : nil }
            finished.append(TypedWord(text: w.text.lowercased(), ms: ms, clean: w.clean))
        }
        entries.removeAll()
        dirtyNext = false
        pendingPause = true
    }

    /// Characters on the screen, letters into words and the rest between them.
    private mutating func type(
        _ text: String, at ms: UInt32, tainted: Bool, closing: Bool = false
    ) {
        for ch in text {
            if ch.isLetter || ch == "'" {
                if case .word(var w) = entries.last {
                    w.text.append(ch)
                    w.endMs = ms
                    w.paused = w.paused || pendingPause
                    w.tainted = w.tainted || tainted
                    entries[entries.count - 1] = .word(w)
                } else {
                    entries.append(.word(Word(
                        text: String(ch), startMs: previousEnd(), endMs: ms, clean: !dirtyNext,
                        paused: pendingPause, tainted: tainted)))
                    dirtyNext = false
                }
                pendingPause = false
            } else {
                if case .gap(let g) = entries.last {
                    entries[entries.count - 1] = .gap(g + String(ch))
                } else {
                    // The space command closes the word before it, so striking it is
                    // part of that word's cost.
                    if closing, case .word(var w) = entries.last {
                        w.endMs = ms
                        entries[entries.count - 1] = .word(w)
                    }
                    entries.append(.gap(String(ch)))
                }
            }
        }
    }

    /// When the word before the one about to start ended, if nothing but spaces lie
    /// between them.
    private func previousEnd() -> UInt32? {
        guard entries.count >= 2, case .gap(let g) = entries[entries.count - 1],
            g.allSatisfy({ $0 == " " }), case .word(let w) = entries[entries.count - 2]
        else { return nil }
        return w.endMs
    }

    /// Take `n` characters off the end of the screen.
    private mutating func erase(_ n: Int) {
        var n = n
        while n > 0, let last = entries.last {
            switch last {
            case .gap(var g):
                let k = min(n, g.count)
                g.removeLast(k)
                n -= k
                if g.isEmpty { entries.removeLast() } else { entries[entries.count - 1] = .gap(g) }
                // Past the end of a word: what comes next is a second go at whatever
                // followed it.
                dirtyNext = true
            case .word(var w):
                let k = min(n, w.text.count)
                w.text.removeLast(k)
                n -= k
                if w.text.isEmpty {
                    entries.removeLast()
                    dirtyNext = true
                } else {
                    w.clean = false
                    entries[entries.count - 1] = .word(w)
                }
            }
        }
    }

    /// A stroke that went wrong without typing anything: against the word in progress,
    /// or the one about to start if the last was closed.
    private mutating func slip() {
        if !output.pendingSpace, case .word(var w) = entries.last {
            w.clean = false
            entries[entries.count - 1] = .word(w)
        } else {
            dirtyNext = true
        }
    }
}

/// What the logs say about one word, as accumulated and stored.
struct WordSamples: Codable, Equatable {
    var total = 0
    /// The times of the most recent clean, timed uses, oldest first, capped at `window`.
    var times: [UInt32] = []
    /// Whether each of the most recent uses was not clean, oldest first, same cap.
    var outcomes: [Bool] = []
    /// The lowest the median of `times` has been while the window was full enough and
    /// clean enough.  `.max` until then.
    ///
    /// What the word ladder's gate reads, and kept for the same reason the pattern
    /// model's `everReached` is: a gate on the current window can close again, and a
    /// ladder that takes words back because one sitting was slow is a ladder nobody can
    /// trust.  Kept as a time rather than as a yes or no so the gate can be asked about
    /// any target speed without folding the history again.
    var bestMs: UInt32 = .max

    /// How many recent uses the median is taken over.
    ///
    /// Twenty, not the pattern model's sixty-four.  A common word comes round several
    /// times a line in the word ladder, so twenty is a few minutes of drilling and says
    /// how fast the word is now; sixty-four of `with` would still be carrying last week.
    static let window = 20
    /// How many uses the window must hold before its median counts toward `bestMs`.
    static let minUses = 15
    /// And how many of them timed.
    static let minTimed = 10
    /// The share of the window that may have needed a correction.
    static let maxErrorRate = 0.1

    mutating func note(ms: UInt32?, clean: Bool) {
        total += 1
        if clean, let ms {
            times.append(ms)
            if times.count > Self.window { times.removeFirst(times.count - Self.window) }
        }
        outcomes.append(!clean)
        if outcomes.count > Self.window { outcomes.removeFirst(outcomes.count - Self.window) }
        let bad = outcomes.filter { $0 }.count
        if outcomes.count >= Self.minUses, times.count >= Self.minTimed,
            Double(bad) <= Self.maxErrorRate * Double(outcomes.count)
        {
            bestMs = min(bestMs, SkillModel.median(times))
        }
    }
}

/// What the logs say about one word.
public struct WordSkill: Equatable, Sendable {
    public let text: String
    /// Every use, clean or not.
    public let count: Int
    /// The median of the recent clean, timed uses; nil before there is one.
    public let medianMs: UInt32?
    /// The share of the recent uses that needed a correction.
    public let errorRate: Double
    /// The best the recent median has been while clean enough to count; nil if never.
    public let bestMs: UInt32?

    public init(
        text: String, count: Int, medianMs: UInt32?, errorRate: Double = 0,
        bestMs: UInt32? = nil
    ) {
        self.text = text
        self.count = count
        self.medianMs = medianMs
        self.errorRate = errorRate
        self.bestMs = bestMs
    }

    init(text: String, samples s: WordSamples) {
        self.init(
            text: text, count: s.total,
            medianMs: s.times.isEmpty ? nil : SkillModel.median(s.times),
            errorRate: s.outcomes.isEmpty
                ? 0 : Double(s.outcomes.filter { $0 }.count) / Double(s.outcomes.count),
            bestMs: s.bestMs == .max ? nil : s.bestMs)
    }
}
