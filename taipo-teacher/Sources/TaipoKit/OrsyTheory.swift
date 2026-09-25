import Foundation

/// The Orsy composition rules: from the four Series of a stroke to the text it spells.
///
/// The third implementation of these rules, after `midi4text-analysis/m4t/theory.py` and
/// `bbq-orsy/src/compose.rs`, and it has to agree with them exactly.  What keeps it
/// honest is `OrsyTheoryTests`: a sample of the Rust crate's exhaustive dump is checked
/// in, and every line of it has to come out the same here.  The Rust side is itself
/// checked against the Python over the whole chord space.
///
/// The tables come from `layouts.json` rather than being written down again; only the
/// rules are here.  Patterns are named by the Michela key pattern they came from, as the
/// export names them, so the rules read the same in all three languages.
///
/// The bit flags in `rules` are the crate's `compose::rules`, and the test checks they
/// agree with the names the export carries.

/// The four patterns of a stroke, by Michela name; empty for an absent Series.
public struct OrsyPatterns: Equatable, Sendable {
    public let onset: String
    public let second: String
    public let vowel: String
    public let coda: String

    /// The pattern keys a skill model measures, `s1:FC` and so on, for the Series that are
    /// present.
    public var keys: [String] {
        var out = [String]()
        if !onset.isEmpty { out.append("s1:\(onset)") }
        if !second.isEmpty { out.append("s2:\(second)") }
        if !vowel.isEmpty { out.append("s3:\(vowel)") }
        if !coda.isEmpty { out.append("s4:\(coda)") }
        return out
    }
}

/// What a stroke spells and how it joins onto its neighbours.
public struct OrsyTranslation: Equatable, Sendable {
    public let text: String
    /// A space may come before this stroke.
    public let spaceBefore: Bool
    /// A space may come after this stroke.
    public let spaceAfter: Bool
    /// The composition rules used, as `OrsyTheory.Rules` flags.
    public let rules: UInt8
    public let patterns: OrsyPatterns
}

/// What a committed stroke turned out to be, as `StrokeOutcome` in the firmware.
public enum OrsyOutcome: Equatable, Sendable {
    case text(OrsyTranslation)
    case undo
    case space
    case capNext
    case doshToggle
    /// The one-shot: this chord is played through the Dosh table.  It is on the right for
    /// the chord form, and on either hand with an Fn key; the Dosh table is the same on
    /// both, so the hand does not matter here.
    case dosh(UInt16)
    /// A mark that takes part in the spacing.
    case punct(Layouts.Orsy.Punctuation)
    /// Neither a syllable nor a command; nothing was typed.
    case dead
}

public final class OrsyTheory {
    /// The rule flags, as the crate numbers them.
    public enum Rules {
        public static let mirrored: UInt8 = 0x01
        public static let cluster: UInt8 = 0x02
        public static let xiH: UInt8 = 0x04
        public static let diphthong: UInt8 = 0x08
        public static let free: UInt8 = 0x10
        public static let bareY: UInt8 = 0x20
        public static let capitalise: UInt8 = 0x40

        /// The flags by the names the export uses.
        public static let byName: [String: UInt8] = [
            "mirrored": mirrored, "cluster": cluster, "xi_h": xiH, "diphthong": diphthong,
            "free": free, "bare_y": bareY, "capitalise": capitalise,
        ]
    }

    public let tables: Layouts.Orsy
    private let outerByBits: [UInt16: Layouts.Orsy.Outer]
    private let secondByBits: [UInt16: Layouts.Orsy.Second]
    private let vowelByBits: [UInt16: Layouts.Orsy.Vowel]
    private let outerByName: [String: Layouts.Orsy.Outer]
    private let secondByName: [String: Layouts.Orsy.Second]
    private let vowelByName: [String: Layouts.Orsy.Vowel]
    private let handMask: UInt16
    private let doshOneshot: UInt16
    private let doshToggle: UInt16
    private let capNext: UInt16
    private let space: UInt16
    private let undo: UInt16

    public init(_ tables: Layouts.Orsy) {
        self.tables = tables
        outerByBits = Dictionary(uniqueKeysWithValues: tables.outer.map { ($0.bits, $0) })
        secondByBits = Dictionary(uniqueKeysWithValues: tables.second.map { ($0.bits, $0) })
        vowelByBits = Dictionary(uniqueKeysWithValues: tables.vowel.map { ($0.bits, $0) })
        outerByName = Dictionary(uniqueKeysWithValues: tables.outer.map { ($0.michela, $0) })
        secondByName = Dictionary(uniqueKeysWithValues: tables.second.map { ($0.michela, $0) })
        vowelByName = Dictionary(uniqueKeysWithValues: tables.vowel.map { ($0.michela, $0) })
        handMask = tables.outerMask | tables.innerMask
        doshOneshot = tables.command("dosh_oneshot") ?? 0x380
        doshToggle = tables.command("dosh_toggle") ?? 0x388
        capNext = tables.command("capitalise_next") ?? 0x188
        space = tables.command("space") ?? 0x200
        undo = tables.command("undo") ?? 0x067
    }

    /// Split a stroke into its patterns, or nil if any Series holds a combination its table
    /// has no entry for, or a key the layout does not have.
    public func patterns(left: UInt16, right: UInt16) -> OrsyPatterns? {
        if (left | right) & ~handMask != 0 { return nil }
        func outer(_ bits: UInt16) -> String? {
            bits == 0 ? "" : outerByBits[bits]?.michela
        }
        guard let s1 = outer(left & tables.outerMask),
            let s2 = left & tables.innerMask == 0
                ? "" : secondByBits[left & tables.innerMask]?.michela,
            let s3 = right & tables.innerMask == 0
                ? "" : vowelByBits[right & tables.innerMask]?.michela,
            let s4 = outer(right & tables.outerMask)
        else { return nil }
        return OrsyPatterns(onset: s1, second: s2, vowel: s3, coda: s4)
    }

    /// What a stroke is: a syllable, a command, a mark, or nothing.
    /// What a stroke is, with the Fn keys that were struck in it.
    /// `OrsyManager::outcome_with_fn` in the Rust.
    ///
    /// An Fn key alone is the toggle, and with keys on the other hand only is the
    /// one-shot for that hand.  Anything else with an Fn key in it is dead, rather than
    /// being read as though Fn were not there.
    public func outcome(left: UInt16, right: UInt16, fnLeft: Bool, fnRight: Bool)
        -> OrsyOutcome
    {
        switch (fnLeft, fnRight) {
        case (false, false): return outcome(left: left, right: right)
        case (true, true): return .dead
        case _ where left == 0 && right == 0: return .doshToggle
        case (true, false) where left == 0: return .dosh(right)
        case (false, true) where right == 0: return .dosh(left)
        default: return .dead
        }
    }

    public func outcome(left: UInt16, right: UInt16) -> OrsyOutcome {
        // A mark is a right-handed stroke on its own, the word-end marker plus outer keys.
        if left == 0, let mark = tables.punctuation.first(where: { $0.bits == right }) {
            return .punct(mark)
        }
        switch (left, right) {
        case (doshToggle, 0): return .doshToggle
        case (doshOneshot, let r) where r != 0: return .dosh(r)
        case (undo, 0): return .undo
        case (0, space): return .space
        case (0, capNext): return .capNext
        default:
            if let t = translate(left: left, right: right) { return .text(t) }
            return .dead
        }
    }

    // MARK: - The rules

    private struct Nucleus {
        var text: String
        var silentE: Bool
        var closes: Bool
        var consumed: Bool
        var rule: UInt8 = 0
    }

    /// Choose the nucleus.  `_nucleus` in the Python, `nucleus` in the Rust.
    private func nucleus(_ s2: String, _ s3: String, _ s4: String) -> Nucleus {
        // The free combinations: alone, a placeholder glyph that closes the word; with
        // Series 2, the digraph Series 2 cannot reach on its own.
        switch s3 {
        case "ea":
            return s2.isEmpty
                ? Nucleus(text: "\u{b0}", silentE: false, closes: true, consumed: false, rule: Rules.free)
                : Nucleus(text: "ea", silentE: false, closes: false, consumed: false, rule: Rules.free)
        case "iea":
            return s2.isEmpty
                ? Nucleus(text: "_", silentE: false, closes: true, consumed: false, rule: Rules.free)
                : Nucleus(text: "ea", silentE: false, closes: true, consumed: false, rule: Rules.free)
        case "ia":
            return s2.isEmpty
                ? Nucleus(text: "*", silentE: false, closes: true, consumed: false, rule: Rules.free)
                : Nucleus(text: "ou", silentE: false, closes: true, consumed: false, rule: Rules.free)
        default: break
        }
        let ends = vowelByName[s3]?.endsWord ?? false
        // A Series 2 vowel fused with the Series 3 vowel.
        switch (s2, s3) {
        case ("U", "u"), ("U", "uia"):
            return Nucleus(text: "au", silentE: false, closes: ends, consumed: true, rule: Rules.diphthong)
        case ("I", "i"), ("I", "ui"):
            return Nucleus(text: "ai", silentE: false, closes: ends, consumed: true, rule: Rules.diphthong)
        default: break
        }
        if !s3.isEmpty {
            return Nucleus(text: vowelByName[s3]?.text ?? "", silentE: false, closes: ends, consumed: false)
        }
        // No Series 3.  Series 2 may supply the nucleus instead.
        switch s2 {
        case "RX": return Nucleus(text: "ea", silentE: true, closes: true, consumed: true, rule: Rules.mirrored)
        case "RXI": return Nucleus(text: "o", silentE: false, closes: false, consumed: true, rule: Rules.mirrored)
        default: break
        }
        if let vowel = secondByName[s2]?.mirroredVowel {
            // Only with a real coda: `ck` and the capitalisation marker do not license the
            // mirrored reading.
            if !s4.isEmpty && s4 != "CZ" && s4 != "SCZ" {
                return Nucleus(text: vowel, silentE: true, closes: true, consumed: true, rule: Rules.mirrored)
            }
        }
        return Nucleus(text: "", silentE: false, closes: false, consumed: false)
    }

    /// An onset that a Series 2 pattern rewrites wholesale.
    private static let onsetOverride: [String: String] = [
        "FC+R": "str", "FC+RI": "spl", "FC+IU": "spr", "FC+XIU": "scr", "C+XIU": "sch",
        "Z+XIU": "sk", "S+X": "sci", "ZN+I": "j", "CP+XIU": "qu",
    ]

    /// Translate one stroke, or nil if it is not a valid syllable.
    public func translate(left: UInt16, right: UInt16) -> OrsyTranslation? {
        guard let p = patterns(left: left, right: right) else { return nil }
        let (s1, s2, s3, s4) = (p.onset, p.second, p.vowel, p.coda)
        // The coda-only shapes have no onset reading.
        let s1Onset: String
        if s1.isEmpty {
            s1Onset = ""
        } else {
            guard let onset = outerByName[s1]?.onset else { return nil }
            s1Onset = onset
        }

        let n = nucleus(s2, s3, s4)
        var used = n.rule

        // An onset cluster needs Series 2 as a consonant; when Series 2 has been taken for
        // the nucleus instead, the cluster is off.
        let override = n.consumed ? nil : Self.onsetOverride["\(s1)+\(s2)"]
        let onset: String
        let middle: String
        if let override {
            used |= Rules.cluster
            onset = override
            middle = ""
        } else {
            onset = (s1 == "FN" && n.consumed) ? "gn" : s1Onset
            if n.consumed {
                middle = ""
            } else if s2 == "XI" {
                if onset.hasSuffix("p") || onset.hasSuffix("w") || onset.hasSuffix("r") {
                    used |= Rules.xiH
                    middle = "h"
                } else {
                    middle = "w"
                }
            } else {
                middle = s2.isEmpty ? "" : (secondByName[s2]?.spells ?? "")
            }
        }

        // A bare final y takes its space from the `ui` keys, which then spell nothing.
        let nucleusText: String
        if s3 == "ui" && s4 == "ZN" {
            used |= Rules.bareY
            nucleusText = ""
        } else {
            nucleusText = n.text
        }

        let capitalise = s4 == "SCZ"
        if capitalise { used |= Rules.capitalise }
        let coda = capitalise ? "" : (s4.isEmpty ? "" : (outerByName[s4]?.coda ?? ""))
        var body = onset + middle + nucleusText
        if capitalise, let first = body.first {
            body = first.uppercased() + body.dropFirst()
        }
        let text = body + coda + (n.silentE ? "e" : "")

        let before: Bool
        let after: Bool
        if n.closes {
            (before, after) = (true, true)
        } else if s3.isEmpty {
            // No vowel: a fragment.  A bare onset leans forward, anything carrying Series 2
            // or a coda leans back onto the preceding syllable.
            let leansForward = (!s1.isEmpty && s2.isEmpty && s4.isEmpty) || s4 == "ZN"
            (before, after) = leansForward ? (true, false) : (false, true)
        } else {
            (before, after) = (true, false)
        }

        return OrsyTranslation(
            text: text, spaceBefore: before, spaceAfter: after, rules: used, patterns: p)
    }
}
