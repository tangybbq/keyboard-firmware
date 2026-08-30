import Foundation

/// The slice of CBOR that minicbor's derive actually emits.
///
/// Not a general CBOR implementation, and not trying to be: it handles unsigned integers,
/// text, byte strings, booleans, null and arrays, which is everything the minder protocol
/// uses.  Anything else is an error rather than a silent misread.
public enum CBORValue: Equatable {
    case uint(UInt64)
    case text(String)
    case bytes([UInt8])
    case bool(Bool)
    case null
    case array([CBORValue])

    public var uintValue: UInt64? { if case .uint(let v) = self { return v }; return nil }
    public var textValue: String? { if case .text(let v) = self { return v }; return nil }
    public var bytesValue: [UInt8]? {
        switch self {
        case .bytes(let b): return b
        // `Vec<u8>` encodes as an array of integers where `ByteVec` encodes as a byte
        // string.  Both appear in the protocol, so both read back as bytes.
        case .array(let items):
            var out = [UInt8]()
            out.reserveCapacity(items.count)
            for item in items {
                guard let v = item.uintValue, v <= 255 else { return nil }
                out.append(UInt8(v))
            }
            return out
        default: return nil
        }
    }
    public var boolValue: Bool? { if case .bool(let v) = self { return v }; return nil }
    public var arrayValue: [CBORValue]? { if case .array(let v) = self { return v }; return nil }
    public var isNull: Bool { self == .null }
}

public enum CBORError: Error, CustomStringConvertible {
    case truncated
    case unsupported(UInt8)
    case badUTF8

    public var description: String {
        switch self {
        case .truncated: return "CBOR data ended mid-item"
        case .unsupported(let b): return String(format: "unsupported CBOR initial byte 0x%02x", b)
        case .badUTF8: return "text string was not valid UTF-8"
        }
    }
}

/// Writes the encoding minicbor reads.
public struct CBORWriter {
    public private(set) var data = [UInt8]()
    public init() {}

    /// Head byte plus the shortest argument encoding, which is what minicbor emits and so
    /// what the golden vectors expect.  A longer-than-necessary encoding would still be
    /// valid CBOR and would still decode, but it would not match byte for byte, and the
    /// vectors are the contract.
    private mutating func head(_ major: UInt8, _ value: UInt64) {
        let m = major << 5
        switch value {
        case 0...23: data.append(m | UInt8(value))
        case 24...0xff:
            data.append(m | 24)
            data.append(UInt8(value))
        case 0x100...0xffff:
            data.append(m | 25)
            data.append(contentsOf: withUnsafeBytes(of: UInt16(value).bigEndian, Array.init))
        case 0x1_0000...0xffff_ffff:
            data.append(m | 26)
            data.append(contentsOf: withUnsafeBytes(of: UInt32(value).bigEndian, Array.init))
        default:
            data.append(m | 27)
            data.append(contentsOf: withUnsafeBytes(of: value.bigEndian, Array.init))
        }
    }

    public mutating func uint(_ v: UInt64) { head(0, v) }
    public mutating func bytes(_ v: [UInt8]) { head(2, UInt64(v.count)); data.append(contentsOf: v) }
    public mutating func text(_ v: String) {
        let utf8 = Array(v.utf8)
        head(3, UInt64(utf8.count))
        data.append(contentsOf: utf8)
    }
    public mutating func arrayHeader(_ count: Int) { head(4, UInt64(count)) }
    public mutating func bool(_ v: Bool) { data.append(v ? 0xf5 : 0xf4) }
    public mutating func null() { data.append(0xf6) }

    public mutating func value(_ v: CBORValue) {
        switch v {
        case .uint(let n): uint(n)
        case .text(let s): text(s)
        case .bytes(let b): bytes(b)
        case .bool(let b): bool(b)
        case .null: null()
        case .array(let items):
            arrayHeader(items.count)
            for item in items { value(item) }
        }
    }
}

/// Reads what the device sends.
public struct CBORReader {
    private let data: [UInt8]
    private var pos = 0

    public init(_ data: [UInt8]) { self.data = data }

    private mutating func byte() throws -> UInt8 {
        guard pos < data.count else { throw CBORError.truncated }
        defer { pos += 1 }
        return data[pos]
    }

    private mutating func argument(_ low: UInt8) throws -> UInt64 {
        switch low {
        case 0...23: return UInt64(low)
        case 24: return UInt64(try byte())
        case 25:
            return (UInt64(try byte()) << 8) | UInt64(try byte())
        case 26:
            var v: UInt64 = 0
            for _ in 0..<4 { v = (v << 8) | UInt64(try byte()) }
            return v
        case 27:
            var v: UInt64 = 0
            for _ in 0..<8 { v = (v << 8) | UInt64(try byte()) }
            return v
        default: throw CBORError.unsupported(low)
        }
    }

    public mutating func value() throws -> CBORValue {
        let initial = try byte()
        let major = initial >> 5
        let low = initial & 0x1f

        switch major {
        case 0:
            return .uint(try argument(low))
        case 2:
            let n = Int(try argument(low))
            guard pos + n <= data.count else { throw CBORError.truncated }
            defer { pos += n }
            return .bytes(Array(data[pos..<(pos + n)]))
        case 3:
            let n = Int(try argument(low))
            guard pos + n <= data.count else { throw CBORError.truncated }
            defer { pos += n }
            guard let s = String(bytes: data[pos..<(pos + n)], encoding: .utf8) else {
                throw CBORError.badUTF8
            }
            return .text(s)
        case 4:
            let n = Int(try argument(low))
            var items = [CBORValue]()
            items.reserveCapacity(n)
            for _ in 0..<n { items.append(try value()) }
            return .array(items)
        case 7:
            switch low {
            case 20: return .bool(false)
            case 21: return .bool(true)
            case 22: return .null
            default: throw CBORError.unsupported(initial)
            }
        default:
            throw CBORError.unsupported(initial)
        }
    }

    /// Decode one complete item, ignoring anything after it.
    ///
    /// Trailing bytes are tolerated because minicbor's decoder tolerates them, and the
    /// device relies on that: a message that arrives concatenated with a stray fragment
    /// still decodes to the item at its front.
    public static func decode(_ data: [UInt8]) throws -> CBORValue {
        var reader = CBORReader(data)
        return try reader.value()
    }
}
