import Foundation

/// The chord tables, loaded from the `layouts.json` the Rust side generates.
///
/// Generated from the real tables by the compiler and checked in, so this cannot describe a
/// layout the keyboard does not have.  See `bbq-keyboard/src/layout/export.rs`.
public struct Layouts: Decodable {
    public let formatVersion: Int
    public let scancodeSet: String
    public let chordTimeMs: UInt32
    /// The fingerprint the device reports in `Reply::Hello`.  They must agree, or every
    /// chord this resolves is a plausible lie.
    public let fingerprint: String
    public let bits: [Bit]
    public let scanMap: ScanMap
    public let variants: [String: Variant]

    public struct Bit: Decodable {
        public let bit: Int
        public let mask: UInt16
        public let name: String
        public let finger: String
        public let row: String
    }

    public struct ScanSlot: Decodable {
        public let key: Int
        public let side: String?
        public let bit: Int?
        public let mask: UInt16?
        public let name: String?
    }

    public struct ScanMap: Decodable {
        public let upper: [ScanSlot]
        public let lower: [ScanSlot]
    }

    public struct Action: Decodable {
        public let kind: String
        public let key: String?
        public let text: String?
        public let mods: [String]?
        /// The characters this chord types, when it types any.
        public let types: String?
    }

    public struct Chord: Decodable {
        public let code: UInt16
        public let keys: [String]
        public let action: Action
    }

    public struct Variant: Decodable {
        public let count: Int
        public let chords: [Chord]
    }

    enum CodingKeys: String, CodingKey {
        case formatVersion = "format_version"
        case scancodeSet = "scancode_set"
        case chordTimeMs = "chord_time_ms"
        case fingerprint, bits, variants
        case scanMap = "scan_map"
    }

    public static func load(from url: URL) throws -> Layouts {
        try JSONDecoder().decode(Layouts.self, from: Data(contentsOf: url))
    }

    /// The tables shipped inside this module.
    ///
    /// A copy of `bbq-keyboard/layouts.json`, which the Rust side generates from the real
    /// chord tables and guards with a staleness test.  Compare [`fingerprintValue`] against
    /// what the device reports before trusting anything resolved through it.
    public static func bundled() throws -> Layouts {
        guard let url = Bundle.module.url(forResource: "layouts", withExtension: "json") else {
            throw LayoutsError.notBundled
        }
        return try load(from: url)
    }

    /// The fingerprint as the integer the device reports.
    public var fingerprintValue: UInt64? {
        UInt64(fingerprint.dropFirst(2), radix: 16)
    }

    /// A chord code looked up in a variant's table.
    public func chord(_ code: UInt16, variant: String) -> Chord? {
        variants[variant]?.chords.first { $0.code == code }
    }

    /// The hand and chord bit a key code maps to, or nil for a key the layout ignores.
    public func scan(_ key: Int, lowerRow: Bool) -> (side: Side, mask: UInt16)? {
        let table = lowerRow ? scanMap.lower : scanMap.upper
        guard key < table.count, let sideName = table[key].side, let mask = table[key].mask
        else { return nil }
        return (sideName == "left" ? .left : .right, mask)
    }
}

public enum LayoutsError: Error, CustomStringConvertible {
    case notBundled
    public var description: String { "layouts.json is missing from the TaipoKit bundle" }
}

public enum Side: Equatable, Sendable {
    case left
    case right
    public var letter: String { self == .left ? "L" : "R" }
    public var index: Int { self == .left ? 0 : 1 }
}
