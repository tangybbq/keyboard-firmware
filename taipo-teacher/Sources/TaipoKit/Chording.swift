import Foundation

/// Assembling key events into chords.
///
/// This is a second implementation of `SideManager` in `bbq-keyboard/src/layout/taipo.rs`,
/// which is exactly the divergence the "replay is the actual engine" decision was meant to
/// prevent.  It exists because the trainer needs chords live, as they are typed, and
/// round-tripping every keystroke through a Rust replay to get them would be absurd.
///
/// What keeps it honest is `Tests/TaipoKitTests`: the Rust replay's golden files are
/// checked in, and this has to reproduce their `chord` lines exactly.  Divergence is a test
/// failure rather than a slow drift in the statistics.
///
/// **Only chord assembly is ported.**  The modifier state machine, `type_text`, and the
/// steno latch are not: the trainer knows the target text and sees the characters actually
/// typed through its own key events, so it needs to know *which chord on which hand*, not
/// what the firmware did with it afterwards.  The smaller this is, the less there is to
/// drift.

/// Why a chord stopped accumulating.
public enum ChordEnd: String, Equatable, Sendable {
    /// Every key came back up while the window was still open.
    case allReleased = "released"
    /// The window expired with keys still held.
    case timerExpired = "timer"
    /// A key landed on the other hand, which ends this hand's chord.
    case otherHand = "other-hand"
}

/// A chord, as assembled.
public struct Chord: Equatable {
    public let timeMs: UInt32
    public let side: Side
    public let code: UInt16
    public let variant: String
    public let firstKeyMs: UInt32
    public let lastKeyMs: UInt32
    public let end: ChordEnd

    /// How much the chord was assembled rather than struck.
    public var spreadMs: UInt32 { lastKeyMs - firstKeyMs }
}

/// One hand's accumulator.
///
/// A direct port; the comments explain what the Rust does rather than restating it.
private struct SideManager {
    let side: Side
    /// Keys physically held.
    var pressed: UInt16 = 0
    /// Keys still held that belong to a chord already sent.  They take no part in the
    /// chord being built and are tracked only until they come back up.
    var inactive: UInt16 = 0
    /// Keys taking part in the chord being built.
    var seen: UInt16 = 0
    /// When the first key of the current chord landed.
    var firstMs: UInt32 = 0
    /// When the most recent key of the current chord landed.
    var lastMs: UInt32 = 0
    /// Whether the chord has already been committed.
    var down = false

    mutating func press(_ mask: UInt16, at time: UInt32, into out: inout [Chord], variant: String) {
        if down {
            // The chord was already sent, so this key starts a new one.  Its keys become
            // inactive rather than waiting to come up.
            inactive |= pressed
            seen = 0
            down = false
        }
        if seen == 0 {
            // The window runs from the first key of the chord, so a chord rolled together
            // slowly still lands at a predictable time.
            firstMs = time
        }
        seen |= mask
        pressed |= mask
        lastMs = time
    }

    mutating func release(
        _ mask: UInt16, at time: UInt32, into out: inout [Chord], variant: String
    ) {
        pressed &= ~mask
        if inactive & mask != 0 {
            inactive &= ~mask
            return
        }
        if pressed & ~inactive == 0 && seen != 0 {
            if !down {
                commit(.allReleased, at: time, into: &out, variant: variant)
            }
            seen = 0
            down = false
        }
    }

    /// Commit the chord being built, if there is one.
    mutating func commit(
        _ end: ChordEnd, at time: UInt32, into out: inout [Chord], variant: String
    ) {
        guard !down, seen != 0 else { return }
        out.append(Chord(
            timeMs: time,
            side: side,
            code: seen,
            variant: variant,
            firstKeyMs: firstMs,
            lastKeyMs: lastMs,
            end: end
        ))
        down = true
    }
}

/// Turns key events into chords, as the firmware does.
public final class ChordEngine {
    private var sides: [SideManager]
    private let layouts: Layouts
    private let chordTimeMs: UInt32

    /// The chord table in use.  Follows the log's `variant` markers.
    public private(set) var variant: String = "taipo"
    /// Whether the two-row layout sits on the lower rows of a three-row board.
    public private(set) var lowerRow = false

    /// Virtual time, advanced by `tick` so that the window can expire between events.
    private var nowMs: UInt32 = 0

    public init(layouts: Layouts) {
        self.layouts = layouts
        self.chordTimeMs = layouts.chordTimeMs
        self.sides = [SideManager(side: .left), SideManager(side: .right)]
    }

    /// Advance to a moment in time, committing any chord whose window has expired.
    ///
    /// The firmware ticks once a millisecond; stepping straight to the next event's time
    /// gives the same answer because nothing else happens in between.
    ///
    /// The deadline is `first + window - 1`, not `first + window`, and the off-by-one is
    /// real rather than a fudge.  A press at time T is delivered before T's own tick, so
    /// the first tick to age the chord is the one stamped T and the hundredth is stamped
    /// T+99.  The hardware agrees: keys down at 219738 committed at 219837.
    private func advance(to time: UInt32, into out: inout [Chord]) {
        for i in sides.indices {
            guard !sides[i].down, sides[i].seen != 0 else { continue }
            let deadline = sides[i].firstMs + chordTimeMs - 1
            if deadline <= time {
                sides[i].commit(.timerExpired, at: deadline, into: &out, variant: variant)
            }
        }
        nowMs = max(nowMs, time)
    }

    /// Feed one key event, returning any chords it completed.
    public func feed(key: Int, press: Bool, timeMs: UInt32) -> [Chord] {
        var out = [Chord]()
        advance(to: timeMs, into: &out)

        guard let (side, mask) = layouts.scan(key, lowerRow: lowerRow) else {
            // A key the layout ignores: the mode key, the toggles, or a dead position.
            return out
        }

        if press {
            // The hands alternate, so a key landing here means the other hand is finished
            // with whatever it was building.
            sides[1 - side.index].commit(.otherHand, at: timeMs, into: &out, variant: variant)
            sides[side.index].press(mask, at: timeMs, into: &out, variant: variant)
        } else {
            sides[side.index].release(mask, at: timeMs, into: &out, variant: variant)
        }
        return out
    }

    /// Apply a marker line from the log.
    public func marker(_ name: String, value: UInt8) {
        switch name {
        case "variant": variant = value == 1 ? "dosh" : "taipo"
        case "row": lowerRow = value == 1
        default: break
        }
    }

    /// Advance the clock without a key event, committing any chord whose window has now
    /// expired.
    ///
    /// Live use needs this and replaying a finished log does not, which is why it was
    /// missing at first.  A chord that is *tapped* commits on the release of its last key,
    /// but one that is *held* past the window commits on the timer -- and on real typing
    /// that is about seven chords in ten.  Without a way to move time forward between
    /// keystrokes, each of those would sit unreported until the next key arrived, so the
    /// display would run a chord behind.
    @discardableResult
    public func advance(toMs time: UInt32) -> [Chord] {
        var out = [Chord]()
        advance(to: time, into: &out)
        return out
    }

    /// Commit anything still open, for the end of a log.
    public func finish() -> [Chord] {
        var out = [Chord]()
        // Far enough ahead that every open window has expired.
        advance(to: nowMs + chordTimeMs, into: &out)
        return out
    }

    /// What a chord types, rendered as the Rust replay renders it, so the golden files can
    /// be compared line for line.
    public func describe(_ chord: Chord) -> String {
        // `bits` covers all ten, thumbs included, in bit order.
        let names = layouts.bits
            .filter { chord.code & $0.mask != 0 }
            .map(\.name)

        let action: String
        if let entry = layouts.chord(chord.code, variant: chord.variant) {
            switch entry.action.kind {
            case "key": action = "key \(entry.action.key ?? "?")"
            case "shifted": action = "shifted \(entry.action.key ?? "?")"
            case "text": action = "text \"\(entry.action.text ?? "")\""
            case "release": action = "release"
            case "oneshot":
                action = "oneshot \(entry.action.mods?.joined(separator: "+") ?? "")"
            default: action = entry.action.kind
            }
        } else {
            action = "dead"
        }

        return String(
            format: "%u %@ %@ 0x%03x [%@] %@ spread=%u %@",
            chord.timeMs, chord.side.letter, chord.variant, chord.code,
            names.joined(separator: "+"), chord.end.rawValue, chord.spreadMs, action
        )
    }
}
