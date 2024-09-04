// The Swift Programming Language
// https://docs.swift.org/swift-book

/// A Steno stroke represents the keys that were pressed in a single "stroke".
public struct Stroke: Equatable, Hashable, Comparable {
    public let bits: UInt32

    public enum StrokeError: Error {
        case invalidStroke(reason: String, text: String)
    }

    /// Build a stroke from a set of bits.
    public init(bits: UInt32) {
        self.bits = bits
    }

    /// An entirely empty stroke, with no keys pressed.  Generally, this doesn't correspond
    /// with anything that would be typed.
    init() {
        self.init(bits: 0)
    }

    /// Construct a stroke from the textual compact representation.
    public init(text: String) throws {
        var stroke: UInt32 = 0

        var tpos = text.startIndex

        // Check for a leading '#'.
        if tpos < text.endIndex && text[tpos] == "#" {
            stroke |= Self.NUM.bits
            tpos = text.index(after: tpos)
        }

        // For the conversion:
        // tpos is the position in the input string.
        // normPos is the position in the regular character string.
        // numPos is the position in the number string.
        // bit is the bit value.
        // cpos and npos are separate because in Swift, character
        // index values are only valid in the string they were created
        // from.
        var normPos = Self.normal.startIndex
        var numPos = Self.nums.startIndex
        var bit = Self.NUM.bits >> 1

        // Move the indexes forward together.
        func keysBump() {
            normPos = Self.normal.index(after: normPos)
            numPos = Self.nums.index(after: numPos)
            bit >>= 1
        }

        // Advance the text pos.
        func textBump() {
            tpos = text.index(after: tpos)
        }

        while tpos < text.endIndex && normPos < Self.normal.endIndex {
            let ch = text[tpos]
            if ch == "#" {
                stroke |= Self.NUM.bits
                textBump()
                continue
            }
            if ch == "-" {
                // The hyphen indicates that there are no left or
                // vowel characters.
                while bit > Self.RIGHT.bits {
                    keysBump()
                }
                textBump()
                continue
            }
            else if ch == Self.normal[normPos] {
                stroke |= bit
                textBump()
            }
            else if ch == Self.nums[numPos] {
                stroke |= bit
                stroke |= Self.NUM.bits
                textBump()
            }

            keysBump()
        }

        // If there are any input characters left, then we have an
        // invalid character in the input.
        if tpos < text.endIndex {
            throw StrokeError.invalidStroke(reason: "Invalid character", text: text)
        }

        self.bits = stroke
    }

    public static func < (lhs: Stroke, rhs: Stroke) -> Bool {
        return lhs.bits < rhs.bits
    }

    public var succ: Stroke {
        get {
            Stroke(bits: bits + 1)
        }
    }

    /// Show this stroke in a compact format.
    public func ToCompact() -> String {
        var buf = ""
        if hasAny(Self.NUM) && !hasAny(Self.DIGITS) {
            buf.append("#")
        }
        let needHyphen = hasAny(Self.RIGHT) && !hasAny(Self.MID)
        let chars = if hasAny(Self.NUM) { Self.nums } else { Self.normal }
        var bit = Self.NUM.bits >> 1
        for ch in chars {
            if ch == "*" && needHyphen {
                buf.append("-")
            }
            if hasAny(Stroke(bits: bit)) {
                buf.append(ch)
            }
            bit >>= 1
        }
        return buf
    }

    // Are any of the bits set in self.
    func hasAny(_ other: Stroke) -> Bool {
        return (bits & other.bits) != 0
    }

    // These are the characters, not including the number bar, that make up a stroke.
    static let normal = "^+STKPWHRAO*EUFRPBLGTSDZ"
    static let nums = "^+12K3W4R50*EU6R7B8G9SDZ"

    // # ^+ST KPHW RAO* EURF PBLG TSDZ
    static let MID = Stroke(bits: 0x007c00)
    static let RIGHT = Stroke(bits: 0x0003ff)
    static let NUM = Stroke(bits: 0x1000000)
    static let DIGITS = Stroke(bits: 0x3562a8)
}

extension String {
    public init(_ stroke: Stroke) {
        self.init(stroke.ToCompact())
    }
}
