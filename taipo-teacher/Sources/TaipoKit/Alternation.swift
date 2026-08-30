import Foundation

/// Whether consecutive chords stayed on the same hand when they need not have.
///
/// Every letter is available on both hands, so a same-hand pair is always avoidable --
/// *while you are mid-flow*.  After a pause the fingers are back at rest and either hand is
/// equally correct, so a pair further apart than [`windowMs`] is neither a fault nor in the
/// denominator.  Counting it either way would mean a session with a lot of thinking time
/// scored better than one without, and would penalise stopping to think, which a trainer
/// must never do.
///
/// One implementation, used by the drill and by the live view, so the two cannot disagree
/// about what a fault is.
public final class AlternationTracker {
    /// A pair closer together than this is judged; anything slower is not.
    public var windowMs: UInt32

    public private(set) var faults = 0
    public private(set) var eligiblePairs = 0

    private var lastSide: Side?
    private var lastEndMs: UInt32?

    public init(windowMs: UInt32 = 2000) {
        self.windowMs = windowMs
    }

    /// Judge one chord against the one before it.
    ///
    /// Applies to *every* chord, backspaces included.  Correcting a mistake is still
    /// typing, and hitting backspace on the same hand as the letter it deletes is the same
    /// technique fault as any other same-hand pair.
    @discardableResult
    public func note(_ chord: Chord) -> Bool {
        defer {
            lastSide = chord.side
            lastEndMs = chord.lastKeyMs
        }
        guard let last = lastSide, let lastEnd = lastEndMs else { return false }
        let gap = chord.firstKeyMs > lastEnd ? chord.firstKeyMs - lastEnd : 0
        guard gap < windowMs else { return false }
        eligiblePairs += 1
        guard last == chord.side else { return false }
        faults += 1
        return true
    }

    public var rate: Double {
        guard eligiblePairs > 0 else { return 0 }
        return Double(faults) / Double(eligiblePairs)
    }
}
