import Foundation

/// What the writer keeps getting wrong, derived from their own corrections.
///
/// This is a second implementation of `taipo_analyze::stats`'s correction finding and
/// confusion classification, and it exists for the reason the plan gives for the chord
/// engine: the trainer builds its drill material from these and cannot round-trip through
/// a Rust CLI to do it.  `Tests/TaipoKitTests/golden/*.corrections` is what keeps the two
/// from drifting -- if the Rust side changes its mind about what a mistake is, the Swift
/// test fails rather than the numbers quietly diverging.

/// How a mistyped chord differed in shape from the one that replaced it.
///
/// Mirrors `taipo_analyze::stats::Confusion`, names included: the raw values are what the
/// golden file spells, so a rename on either side is a test failure.
public enum Confusion: String, CaseIterable, Sendable {
    /// The right fingers, one or more of them on the wrong row.
    case wrongRow = "wrong-row"
    /// The right rows, a key struck by the wrong finger.
    case wrongFinger = "wrong-finger"
    /// The right chord with a key missing.
    case keyDropped = "key-dropped"
    /// The right chord with an extra key.
    case keyAdded = "key-added"
    /// The right finger keys under the wrong thumb, so the wrong layer.
    case wrongLayer = "wrong-layer"
    /// Nothing systematic.
    case unrelated = "unrelated"

    /// What each of the four columns and two thumbs was asked to do: 0 nothing, 1 the
    /// bottom key, 2 the top key, 3 both.
    ///
    /// Bits 0..3 are the bottom row, pinky to index, and 4..7 the top, so a finger's two
    /// keys are four bits apart.
    static func fingerStates(_ code: UInt16) -> [UInt8] {
        var s = [UInt8](repeating: 0, count: 6)
        for col in 0..<4 {
            s[col] = UInt8((code >> col) & 1) | (UInt8((code >> (col + 4)) & 1) << 1)
        }
        s[4] = UInt8((code >> 8) & 1)
        s[5] = UInt8((code >> 9) & 1)
        return s
    }

    /// Classify the chord that was typed against the one that replaced it.
    ///
    /// The order of the tests is the classification: each is narrower than the next and the
    /// first that fits wins.  A chord wrong in two ways at once falls through to
    /// `unrelated` rather than being filed under whichever test ran first.
    public static func of(typed: UInt16, meant: UInt16) -> Confusion {
        if typed == meant { return .unrelated }
        let t = fingerStates(typed)
        let m = fingerStates(meant)
        let differ = (0..<6).filter { t[$0] != m[$0] }
        let thumbs = differ.contains { $0 >= 4 }
        let fingers = differ.contains { $0 < 4 }

        // A thumb selects the layer, so a thumb difference is a different chord rather
        // than a misfingering -- unless it is the only difference.
        if thumbs { return fingers ? .unrelated : .wrongLayer }

        // Every finger that differs is on the other row.
        if differ.allSatisfy({ (t[$0] == 1 && m[$0] == 2) || (t[$0] == 2 && m[$0] == 1) }) {
            return .wrongRow
        }

        // The same number of keys on each row, so the rows were right and the keys are
        // under the wrong fingers.  Nothing narrower works: the whole shape can shift a
        // column over, leaving a finger holding a key in both chords.
        func rowCounts(_ c: UInt16) -> (Int, Int) {
            ((c & 0x0f).nonzeroBitCount, (c & 0xf0).nonzeroBitCount)
        }
        if rowCounts(typed) == rowCounts(meant) { return .wrongFinger }

        if typed & meant == typed { return .keyDropped }
        if typed & meant == meant { return .keyAdded }
        return .unrelated
    }
}

/// How far off a correction was, as opposed to what shape it had.
public enum CorrectionKind: String, Sendable {
    case sameChord = "same-chord"
    case oneKeyOff = "one-key-off"
    case different = "different"
    case noReplacement = "no-replacement"
}

/// A backspace, what it deleted, and what replaced it.
public struct Correction: Equatable, Sendable {
    public let deleted: UInt16?
    public let replacement: UInt16?
    public let kind: CorrectionKind
    /// The shape of the mistake, when both ends are known and they differ.
    public let confusion: Confusion?
}

/// Finding the corrections in a stream of chords.
public struct CorrectionScanner {
    private let layouts: Layouts
    private let variant: String

    public init(layouts: Layouts, variant: String = "taipo") {
        self.layouts = layouts
        self.variant = variant
    }

    /// Whether a chord's action is a backspace, which is what a correction looks like.
    func isBackspace(_ code: UInt16) -> Bool {
        layouts.chord(code, variant: variant)?.action.key == "DeleteBackspace"
    }

    /// Whether a chord types something, as opposed to being a modifier or a backspace.
    ///
    /// By the kind rather than by `types`: Return and Tab type no printable character but
    /// are still text as far as a backspace is concerned, since backspacing after one
    /// deletes it.
    func isText(_ code: UInt16) -> Bool {
        guard let action = layouts.chord(code, variant: variant)?.action else { return false }
        guard ["key", "shifted", "text"].contains(action.kind) else { return false }
        return action.key != "DeleteBackspace"
    }

    /// Two chords differing by exactly one finger.
    ///
    /// A key added or removed flips one bit, but a key substituted for its neighbour flips
    /// two -- one off, one on -- and is still a single misplaced finger.  Requiring the
    /// popcounts to match separates that from two unrelated chords two bits apart.
    static func oneKeyOff(_ a: UInt16, _ b: UInt16) -> Bool {
        let diff = (a ^ b).nonzeroBitCount
        return diff == 1 || (diff == 2 && a.nonzeroBitCount == b.nonzeroBitCount)
    }

    static func classify(deleted: UInt16?, replacement: UInt16?) -> CorrectionKind {
        guard let r = replacement else { return .noReplacement }
        guard let d = deleted else { return .different }
        if d == r { return .sameChord }
        return oneKeyOff(d, r) ? .oneKeyOff : .different
    }

    /// Every correction in one session's chords.
    ///
    /// One session at a time, never across a join: two chords either side of a break may
    /// be hours apart, and a backspace does not correct something typed yesterday.
    public func scan(_ codes: [UInt16]) -> [Correction] {
        var out = [Correction]()
        for (i, code) in codes.enumerated() {
            guard isBackspace(code) else { continue }
            // Only the first backspace of a run opens a correction: a run of them is one
            // correction, not several.
            if i > 0, isBackspace(codes[i - 1]) { continue }

            // What it deleted: the nearest preceding chord that typed something.
            let deleted = codes[..<i].last { isText($0) }
            // What replaced it: the next chord that types, as long as it is not another
            // backspace.
            let after = codes[(i + 1)...].first { isText($0) || isBackspace($0) }
            let replacement = after.flatMap { isBackspace($0) ? nil : $0 }

            let confusion: Confusion? =
                if let d = deleted, let r = replacement, d != r {
                    Confusion.of(typed: d, meant: r)
                } else {
                    nil
                }
            out.append(
                Correction(
                    deleted: deleted, replacement: replacement,
                    kind: Self.classify(deleted: deleted, replacement: replacement),
                    confusion: confusion))
        }
        return out
    }
}
