import Foundation

/// The chord tables, loaded from the `layouts.json` the Rust side generates.
///
/// Generated from the real tables by the compiler and checked in, so this cannot describe a
/// layout the keyboard does not have.  See `bbq-keyboard/src/layout/export.rs`.
public struct Layouts: Decodable {
    public let formatVersion: Int
    public let scancodeSet: String
    public let chordTimeMs: UInt32
    /// The table the engine comes up in, and so what a log that never mentions the table
    /// means.
    ///
    /// Read rather than assumed.  A reader that guesses is a reader that disagrees with
    /// the firmware the moment the firmware changes its mind, and the disagreement is
    /// silent: chords resolve, words segment, and a whole session lands in the wrong
    /// table's measurements.  `bbq-keyboard/src/layout/taipo.rs` holds the constant this
    /// comes from.
    public let defaultVariant: String
    /// The fingerprint the device reports in `Reply::Hello`.  They must agree, or every
    /// chord this resolves is a plausible lie.
    public let fingerprint: String
    public let bits: [Bit]
    public let scanMap: ScanMap
    public let variants: [String: Variant]
    /// The Orsy tables, when the export carries them.  Not a variant: a variant is a table
    /// of one-hand chord codes, and Orsy is a syllabic layout whose strokes are spelled by
    /// rule from a few dozen patterns.  See `OrsyTheory`.
    public let orsy: Orsy?
    /// The key codes the layout manager handles itself.  Only the ones a reader needs are
    /// decoded.
    public let specialKeys: SpecialKeys?

    public struct SpecialKeys: Decodable {
        /// The mesa3b's Fn keys, which are keys of the stroke in Orsy.  Absent from an
        /// export older than the mesa3b.
        public let fnLeft: Int?
        public let fnRight: Int?

        enum CodingKeys: String, CodingKey {
            case fnLeft = "fn_left"
            case fnRight = "fn_right"
        }
    }

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

    /// The Orsy tables: three groups of patterns, the commands and the rule flags, with a
    /// fingerprint of their own so that a change to them costs nothing Taipo or Dosh has
    /// learned, and the other way round.
    public struct Orsy: Decodable {
        public let fingerprint: String
        public let rulesVersion: Int
        public let outerMask: UInt16
        public let innerMask: UInt16
        public let outer: [Outer]
        public let second: [Second]
        public let vowel: [Vowel]
        public let commands: [Command]
        public let punctuation: [Punctuation]
        public let rules: [Rule]

        /// A shape on the outer five keys: an onset on the left hand, a coda on the right.
        public struct Outer: Decodable {
            public let michela: String
            public let bits: UInt16
            public let keys: [String]
            /// Nil for the three coda-only shapes.
            public let onset: String?
            public let coda: String
        }

        /// A second-character pattern, on the left hand's inner four.
        public struct Second: Decodable {
            public let michela: String
            public let bits: UInt16
            public let keys: [String]
            public let spells: String
            public let mirroredVowel: String?
            enum CodingKeys: String, CodingKey {
                case michela, bits, keys, spells
                case mirroredVowel = "mirrored_vowel"
            }
        }

        /// A vowel pattern, on the right hand's inner four.
        public struct Vowel: Decodable {
            public let michela: String
            public let bits: UInt16
            public let keys: [String]
            public let text: String
            public let endsWord: Bool
            enum CodingKeys: String, CodingKey {
                case michela, bits, keys, text
                case endsWord = "ends_word"
            }
        }

        public struct Command: Decodable {
            public let name: String
            public let hand: String
            public let bits: UInt16
            public let keys: [String]
        }

        public struct Rule: Decodable {
            public let name: String
            public let flag: UInt8
        }

        /// A mark that takes part in the spacing: a right-handed stroke on its own.
        ///
        /// Every mark attaches to what came before it.  `spaceAfter` is the far side: most
        /// owe the next word a space, while the apostrophe and the hyphen bind straight on
        /// to it.
        public struct Punctuation: Decodable, Equatable, Sendable {
            public let text: String
            public let bits: UInt16
            public let keys: [String]
            public let spaceAfter: Bool
            public let capitalises: Bool

            enum CodingKeys: String, CodingKey {
                case text, bits, keys, capitalises
                case spaceAfter = "space_after"
            }
        }

        enum CodingKeys: String, CodingKey {
            case fingerprint, outer, second, vowel, commands, punctuation, rules
            case rulesVersion = "rules_version"
            case outerMask = "outer_mask"
            case innerMask = "inner_mask"
        }

        /// The fingerprint as an integer.
        public var fingerprintValue: UInt64? { UInt64(fingerprint.dropFirst(2), radix: 16) }

        public func command(_ name: String) -> UInt16? {
            commands.first { $0.name == name }?.bits
        }
    }

    enum CodingKeys: String, CodingKey {
        case formatVersion = "format_version"
        case scancodeSet = "scancode_set"
        case chordTimeMs = "chord_time_ms"
        case defaultVariant = "default_variant"
        case fingerprint, bits, variants, orsy
        case scanMap = "scan_map"
        case specialKeys = "special_keys"
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

    /// The hand whose Fn key a key code is, or nil when it is not an Fn key.
    public func fnKey(_ key: Int) -> Side? {
        if key == specialKeys?.fnLeft { return .left }
        if key == specialKeys?.fnRight { return .right }
        return nil
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
