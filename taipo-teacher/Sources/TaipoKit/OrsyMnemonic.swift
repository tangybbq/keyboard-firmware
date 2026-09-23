import Foundation

/// A sayable rule for an Orsy pattern: what it is made of, and what Dosh does there.
///
/// The drill draws the stroke, which is the obvious way to show a chord and no use at all
/// to a writer who has no visual imagery to store it in.  What is left for them is the
/// movement, which wants repetition rather than pictures, and the rules the mapping was
/// designed around, which can be said: `d` is `t` plus the pinky, `ea` is `e` plus `a`,
/// the ending form is the plain one plus `Bk`.
///
/// Derived from the tables rather than written out, so a mapping change cannot leave a
/// mnemonic behind describing a keyboard nobody has.  The Dosh note is the other half:
/// nineteen of the thirty shapes mean something else in Dosh, and the habit that has to
/// be overwritten is worth naming.
public enum OrsyMnemonic {
    /// The rule and the Dosh note for a skill key, or nil when neither applies.
    public static func of(_ key: String, tables: Layouts.Orsy, layouts: Layouts) -> String? {
        let parts = key.split(separator: ":", maxSplits: 1).map(String.init)
        guard parts.count == 2 else { return nil }
        let (series, name) = (parts[0], parts[1])
        let notes = [rule(series: series, name: name, tables: tables),
                     dosh(series: series, name: name, tables: tables, layouts: layouts)]
        let out = notes.compactMap { $0 }.joined(separator: " · ")
        return out.isEmpty ? nil : out
    }

    /// The pinky, which is the voicing modifier the outer five are built around.
    private static let pinky: UInt16 = 0x001
    /// The word-end marker, which is what an ending vowel adds.
    private static let endMarker: UInt16 = 0x200

    /// What the pattern is made of.
    private static func rule(series: String, name: String, tables: Layouts.Orsy) -> String? {
        switch series {
        case "s1", "s4":
            // The mapping's voicing rule: the rarer of a pair is the commoner plus the
            // pinky.  `d` is `t` plus the pinky, `b` is `p` plus the pinky.
            guard let shape = tables.outer.first(where: { $0.michela == name }),
                shape.bits & pinky != 0,
                let base = tables.outer.first(where: { $0.bits == shape.bits & ~pinky })
            else { return nil }
            let spells = series == "s1" ? base.onset : base.coda
            guard let spells, !spells.isEmpty else { return nil }
            return "\(spells) + pinky"
        case "s3":
            guard let vowel = tables.vowel.first(where: { $0.michela == name }) else { return nil }
            // An ending form is the plain one plus the space thumb.
            if vowel.endsWord,
                let plain = tables.vowel.first(where: { $0.bits == vowel.bits & ~endMarker })
            {
                return "\(plain.text) + Bk"
            }
            // And the two-letter vowels are their two letters' chords together.
            for a in tables.vowel where !a.endsWord {
                for b in tables.vowel
                where !b.endsWord && a.bits | b.bits == vowel.bits && a.bits != b.bits
                    && a.text + b.text == vowel.text
                {
                    return "\(a.text) + \(b.text)"
                }
            }
            return nil
        case "s2":
            // Series 2 keeps the vowel's chord wherever it means the same vowel, plainly
            // or as a mirrored vowel, which is the one thing about the left hand's inner
            // four that transfers.  The mirrored o is on the ending o, since the plain o
            // has the plain chord.
            guard let second = tables.second.first(where: { $0.michela == name }),
                let vowel = tables.vowel.first(where: { $0.bits == second.bits })
            else { return nil }
            let form = vowel.endsWord ? "ending" : "vowel"
            if vowel.text == second.spells {
                return "same chord as the \(form) \(vowel.text)"
            }
            if vowel.text == second.mirroredVowel {
                return "mirrored \(vowel.text): same chord as the \(form) \(vowel.text)"
            }
            return nil
        default:
            return nil
        }
    }

    /// What Dosh does with the same keys: the habit to keep or the one to overwrite.
    private static func dosh(
        series: String, name: String, tables: Layouts.Orsy, layouts: Layouts
    ) -> String? {
        let bits: UInt16?
        let spells: String?
        switch series {
        case "s1", "s4":
            let shape = tables.outer.first { $0.michela == name }
            bits = shape?.bits
            spells = series == "s1" ? shape?.onset : shape?.coda
        case "s2":
            let second = tables.second.first { $0.michela == name }
            bits = second?.bits
            spells = second?.spells
        case "s3":
            let vowel = tables.vowel.first { $0.michela == name }
            bits = vowel?.bits
            spells = vowel?.text
        default:
            return nil
        }
        guard let bits, let chord = layouts.chord(bits, variant: "dosh") else { return nil }
        let does = describe(chord.action)
        guard let does else { return nil }
        if let spells, does == spells { return "as in Dosh" }
        return "Dosh: \(does)"
    }

    /// What a Dosh chord does, in as few words as it takes.
    private static func describe(_ action: Layouts.Action) -> String? {
        if let types = action.types { return types == " " ? "space" : types }
        switch action.kind {
        case "oneshot": return action.mods?.joined(separator: "+")
        case "release": return "release"
        case "key": return action.key
        default: return nil
        }
    }
}
