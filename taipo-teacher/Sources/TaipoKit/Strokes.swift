import Foundation

/// Assembling key events into Orsy strokes.
///
/// A port of `OrsyManager` in `bbq-keyboard/src/layout/orsy.rs`, kept honest the same
/// way `ChordEngine` is: the Rust replay's `orsy-*` golden files are checked in, and
/// this has to reproduce their `stroke` lines exactly.
///
/// Orsy borrows steno's front and none of Taipo's: the whole board is one stroke,
/// accumulated across both hands and committed on the *first* release, with everything
/// held at that moment.  Further releases only take keys away, and a press while
/// releasing starts a new stroke from what is still held.  There is no chord window.

/// An Orsy stroke, as assembled.
public struct Stroke: Equatable {
    public let timeMs: UInt32
    public let left: UInt16
    public let right: UInt16
    public let firstKeyMs: UInt32
    public let lastKeyMs: UInt32
    public let outcome: OrsyOutcome

    public var spreadMs: UInt32 { lastKeyMs - firstKeyMs }

    /// The translation, when the stroke was a syllable.
    public var translation: OrsyTranslation? {
        if case .text(let t) = outcome { return t }
        return nil
    }
}

/// Turns key events into strokes, as the firmware does.
public final class StrokeEngine {
    private let layouts: Layouts
    public let theory: OrsyTheory
    private var left: UInt16 = 0
    private var right: UInt16 = 0
    /// The Fn keys held.  Keys of the stroke, but not chord bits, so kept apart.
    private var fnLeft = false
    private var fnRight = false
    /// Whether keys are being pressed (true) or released (false).
    private var pressing = true
    /// When each chord bit last went down, per hand.
    private var pressedMs = [[UInt32]](repeating: [UInt32](repeating: 0, count: 10), count: 2)

    public init(layouts: Layouts, theory: OrsyTheory) {
        self.layouts = layouts
        self.theory = theory
    }

    /// Forget the keys held, for a mode change: the releases of whatever is down go to the
    /// other mode.
    public func reset() {
        left = 0
        right = 0
        fnLeft = false
        fnRight = false
        pressing = true
    }

    /// Feed one key event, returning the stroke it completed, if any.
    public func feed(key: Int, press: Bool, timeMs: UInt32, lowerRow: Bool) -> Stroke? {
        let held = (left: left, right: right, fnLeft: fnLeft, fnRight: fnRight)
        if let side = layouts.fnKey(key) {
            // An Fn key is a key of the stroke like any other, on the mesa3b.
            if side == .left { fnLeft = press } else { fnRight = press }
        } else {
            guard let (side, mask) = layouts.scan(key, lowerRow: lowerRow) else { return nil }
            let handMask = theory.tables.outerMask | theory.tables.innerMask
            // The upper pinky, which is not an Orsy key.
            guard mask & handMask != 0 else { return nil }

            if press {
                pressedMs[side.index][mask.trailingZeroBitCount] = timeMs
                if side == .left { left |= mask } else { right |= mask }
            } else {
                if side == .left { left &= ~mask } else { right &= ~mask }
            }
        }

        switch (press, pressing) {
        case (true, true):
            return nil
        case (false, true):
            // The first release sends everything that was held.  A stroke of nothing can
            // only be a release left over from another mode.
            pressing = false
            guard held.left != 0 || held.right != 0 || held.fnLeft || held.fnRight
            else { return nil }
            return stroke(
                left: held.left, right: held.right, fnLeft: held.fnLeft,
                fnRight: held.fnRight, at: timeMs)
        case (true, false):
            pressing = true
            return nil
        case (false, false):
            return nil
        }
    }

    private func stroke(
        left: UInt16, right: UInt16, fnLeft: Bool, fnRight: Bool, at time: UInt32
    ) -> Stroke {
        var first = UInt32.max
        var last: UInt32 = 0
        for (side, code) in [(0, left), (1, right)] {
            for bit in 0..<10 where code & (1 << bit) != 0 {
                first = min(first, pressedMs[side][bit])
                last = max(last, pressedMs[side][bit])
            }
        }
        return Stroke(
            timeMs: time, left: left, right: right,
            firstKeyMs: first == .max ? time : first,
            lastKeyMs: first == .max ? time : last,
            outcome: theory.outcome(left: left, right: right, fnLeft: fnLeft, fnRight: fnRight))
    }

    /// The stroke rendered as the Rust replay renders it, so the golden files can be
    /// compared line for line.
    public func describe(_ stroke: Stroke) -> String {
        func names(_ code: UInt16) -> String {
            layouts.bits.filter { code & $0.mask != 0 }.map(\.name).joined(separator: "+")
        }
        let outcome: String
        switch stroke.outcome {
        case .text(let t):
            outcome = "text \"\(t.text)\" before=\(t.spaceBefore ? 1 : 0) after=\(t.spaceAfter ? 1 : 0)"
        case .undo: outcome = "undo"
        case .space: outcome = "space"
        case .capNext: outcome = "cap-next"
        case .doshToggle: outcome = "dosh-toggle"
        case .dosh(let code): outcome = String(format: "dosh 0x%03x", code)
        case .punct(let mark): outcome = "punct \"\(mark.text)\""
        case .dead: outcome = "dead"
        }
        return "\(stroke.timeMs) [\(names(stroke.left))-\(names(stroke.right))] spread=\(stroke.spreadMs) \(outcome)"
    }
}

/// The key log's numbering for a mode, `minder::keylog::mode_code`.
public enum KeyboardMode: UInt8, Equatable, Sendable {
    case taipo = 0
    case steno = 1
    case stenoDirect = 2
    case qwerty = 3
    case nkro = 4
    case orsy = 5
}
