import Foundation

/// The minder protocol, as `minder/src/lib.rs` defines it.
///
/// # The framing, which is the whole difficulty
///
/// minicbor's derive writes an enum variant as
///
/// ```text
/// [ variant index, [ field 0, field 1, ... ] ]
/// ```
///
/// where the inner array is **positional** and every field number not used is filled with
/// `null`.  A field declared `#[n(1)]` therefore produces a leading `f6`, not a
/// one-element array.  An encoder emitting the natural `[1, ["2024-11-01a"]]` is perfectly
/// valid CBOR and the device will reject it.
///
/// The inner array's length is one past the highest field number used, so it grows when
/// fields are added: `Reply::Hello` is three long from firmware predating `boot_id` and six
/// long from firmware that has it.  Decoding therefore reads whatever length arrives and
/// treats missing trailing fields as absent, which is what makes a new host able to talk to
/// an old keyboard.
///
/// `Tests/MinderKitTests` checks all of this against the vectors the Rust side generates.
public enum MinderVersion {
    public static let current = "2026-08-30a"
}

/// Capability names a device may report in `Reply::Hello`.
public enum Capability {
    public static let events = "events"
    public static let testEvents = "test-events"
    public static let flash = "flash"
    public static let keyLog = "key-log"
}

/// Build a variant: the index, then a positional field array with nulls in the gaps.
private func variant(_ index: UInt64, _ fields: [Int: CBORValue]) -> [UInt8] {
    var w = CBORWriter()
    w.arrayHeader(2)
    w.uint(index)
    let count = (fields.keys.max().map { $0 + 1 }) ?? 0
    w.arrayHeader(count)
    for i in 0..<count {
        w.value(fields[i] ?? .null)
    }
    return w.data
}

public enum Request {
    case hello(version: String)
    case readFlash(offset: UInt32, size: UInt32)
    case hash(offset: UInt32, size: UInt32)
    case program(offset: UInt32, data: [UInt8])
    case getEvent(timeoutMs: UInt32)
    case testEvent(count: UInt32, delayMs: UInt32)
    case setLogging(enabled: Bool, watermark: UInt32)
    case getEventLog(maxBytes: UInt32)
    case eventLogAck(throughSeq: UInt32)
    case reset

    public func encode() -> [UInt8] {
        switch self {
        case .hello(let version):
            return variant(1, [1: .text(version)])
        case .readFlash(let offset, let size):
            return variant(2, [0: .uint(UInt64(offset)), 1: .uint(UInt64(size))])
        case .hash(let offset, let size):
            return variant(4, [0: .uint(UInt64(offset)), 1: .uint(UInt64(size))])
        case .program(let offset, let data):
            return variant(5, [0: .uint(UInt64(offset)), 1: .bytes(data)])
        case .getEvent(let timeoutMs):
            return variant(6, [0: .uint(UInt64(timeoutMs))])
        case .testEvent(let count, let delayMs):
            return variant(7, [0: .uint(UInt64(count)), 1: .uint(UInt64(delayMs))])
        case .setLogging(let enabled, let watermark):
            return variant(8, [0: .bool(enabled), 1: .uint(UInt64(watermark))])
        case .getEventLog(let maxBytes):
            return variant(9, [0: .uint(UInt64(maxBytes))])
        case .eventLogAck(let throughSeq):
            return variant(10, [0: .uint(UInt64(throughSeq))])
        case .reset:
            return variant(255, [:])
        }
    }
}

/// An event the device raises.
public enum Event: Equatable {
    case test(seq: UInt32)
    case logReady(pending: UInt32)
    /// A variant this build does not know.  Newer firmware, not corruption.
    case unknown(index: UInt64)

    static func decode(_ value: CBORValue) -> Event {
        guard let outer = value.arrayValue, outer.count == 2,
              let index = outer[0].uintValue,
              let fields = outer[1].arrayValue
        else { return .unknown(index: 0) }
        func field(_ i: Int) -> CBORValue? { i < fields.count ? fields[i] : nil }
        switch index {
        case 1: return .test(seq: UInt32(field(0)?.uintValue ?? 0))
        case 2: return .logReady(pending: UInt32(field(0)?.uintValue ?? 0))
        default: return .unknown(index: index)
        }
    }
}

public struct HelloInfo: Equatable {
    public let version: String
    public let info: String
    /// Absent from firmware predating the field, which is not the same as zero.
    public let bootID: UInt64?
    public let layoutFingerprint: UInt64?
    public let capabilities: [String]?

    /// Whether the device claims a capability.
    ///
    /// A device that reports no list at all predates it, and is treated as having what
    /// existed before the list did -- flash access -- rather than as having nothing, which
    /// would make an old keyboard look less capable than it is.
    public func supports(_ capability: String) -> Bool {
        guard let capabilities else { return capability == Capability.flash }
        return capabilities.contains(capability)
    }
}

public struct EventLogBatch: Equatable {
    public let bootID: UInt64
    public let seq: UInt32
    public let dropped: UInt32
    public let anchorMs: UInt32
    public let records: [UInt8]
    public let remaining: UInt32
}

public enum Reply: Equatable {
    case hello(HelloInfo)
    case log(String)
    case flashData(offset: UInt32, data: [UInt8])
    case hash([UInt8])
    case programDone
    case event(Event)
    case noEvent
    case ok
    case eventLog(EventLogBatch)
    case error(String)
    case reset
    case unknown(index: UInt64)

    public static func decode(_ bytes: [UInt8]) throws -> Reply {
        let value = try CBORReader.decode(bytes)
        guard let outer = value.arrayValue, outer.count == 2,
              let index = outer[0].uintValue,
              let fields = outer[1].arrayValue
        else { throw CBORError.unsupported(bytes.first ?? 0) }

        // Missing trailing fields are absent, not an error: that is what lets this talk to
        // firmware older than itself.
        func field(_ i: Int) -> CBORValue? {
            guard i < fields.count else { return nil }
            return fields[i].isNull ? nil : fields[i]
        }

        switch index {
        case 1:
            return .hello(HelloInfo(
                version: field(1)?.textValue ?? "",
                info: field(2)?.textValue ?? "",
                bootID: field(3)?.uintValue,
                layoutFingerprint: field(4)?.uintValue,
                capabilities: field(5)?.arrayValue?.compactMap { $0.textValue }
            ))
        case 2:
            return .log(field(1)?.textValue ?? "")
        case 3:
            return .flashData(
                offset: UInt32(field(0)?.uintValue ?? 0),
                data: field(1)?.bytesValue ?? []
            )
        case 4:
            return .hash(field(0)?.bytesValue ?? [])
        case 5:
            return .programDone
        case 6:
            return .event(Event.decode(field(0) ?? .null))
        case 7:
            return .noEvent
        case 8:
            return .ok
        case 9:
            return .eventLog(EventLogBatch(
                bootID: field(0)?.uintValue ?? 0,
                seq: UInt32(field(1)?.uintValue ?? 0),
                dropped: UInt32(field(2)?.uintValue ?? 0),
                anchorMs: UInt32(field(3)?.uintValue ?? 0),
                records: field(4)?.bytesValue ?? [],
                remaining: UInt32(field(5)?.uintValue ?? 0)
            ))
        case 254:
            return .error(field(0)?.textValue ?? "")
        case 255:
            return .reset
        default:
            return .unknown(index: index)
        }
    }
}
