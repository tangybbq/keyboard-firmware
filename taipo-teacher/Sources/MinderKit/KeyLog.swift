import Foundation

/// The key event log's record format, as `minder/src/keylog.rs` defines it.
///
/// Four bytes, fixed stride: tag, delta low, delta high, aux.  The stride is fixed so the
/// device's ring buffer stays trivial, and it is what lets a reader skip a record it does
/// not understand without losing its place.
public enum LogRecord: Equatable {
    case key(code: UInt8, press: Bool, delta: Delta)
    case marker(marker: Marker, value: UInt8, delta: Delta)
    /// A marker kind this build does not know: newer firmware, not corruption.  Carries its
    /// delta so the timeline stays right even when the record itself means nothing here.
    case unknown(tag: UInt8, delta: Delta)

    public static let size = 4

    /// The key code reported for a physical key with no key code at all.
    public static let codeNone: UInt8 = 63

    public var delta: Delta {
        switch self {
        case .key(_, _, let d), .marker(_, _, let d), .unknown(_, let d): return d
        }
    }

    public static func decode(_ bytes: ArraySlice<UInt8>) -> LogRecord? {
        guard bytes.count == size else { return nil }
        let b = Array(bytes)
        let delta = Delta(raw: UInt16(b[1]) | (UInt16(b[2]) << 8))
        if b[0] & 0x80 == 0 {
            return .key(code: b[0] & 0x3f, press: b[0] & 0x40 != 0, delta: delta)
        }
        guard let marker = Marker(rawValue: b[0] & 0x7f) else {
            return .unknown(tag: b[0], delta: delta)
        }
        return .marker(marker: marker, value: b[3], delta: delta)
    }

    /// Decode a whole batch.
    public static func decodeAll(_ bytes: [UInt8]) -> [LogRecord] {
        stride(from: 0, to: bytes.count - bytes.count % size, by: size).compactMap {
            decode(bytes[$0..<($0 + size)])
        }
    }
}

/// What a marker record describes.
public enum Marker: UInt8, Equatable {
    /// The layout mode, so keystrokes made outside taipo can be excluded.
    case mode = 0
    /// The chord table: 0 Taipo, 1 Dosh.  Without it the two layouts' statistics pool.
    case variant = 1
    /// The row position on a three-row board.
    case rowShift = 2
    /// Logging turned on.  A gap after a pause is silence, not loss.
    case resume = 3
    /// Logging turned off.
    case pause = 4

    public var name: String {
        switch self {
        case .mode: return "mode"
        case .variant: return "variant"
        case .rowShift: return "row"
        case .resume: return "resume"
        case .pause: return "pause"
        }
    }
}

/// The time since the previous record.
///
/// A delta rather than an uptime, because an absolute millisecond counter narrowed into a
/// record wraps at 49.7 days and a wrapped entry does not look wrong, it looks recent.  A
/// delta is bounded by human behaviour instead.
///
/// Bit 15 selects the unit: clear means milliseconds up to 32.7 s, set means seconds up to
/// 9.1 h.  Resolution is spent only on gaps where nobody cares about the millisecond.
public struct Delta: Equatable {
    public let raw: UInt16
    public init(raw: UInt16) { self.raw = raw }

    public var isCoarse: Bool { raw & 0x8000 != 0 }
    public var milliseconds: UInt64 {
        isCoarse ? UInt64(raw & 0x7fff) * 1000 : UInt64(raw)
    }
}
