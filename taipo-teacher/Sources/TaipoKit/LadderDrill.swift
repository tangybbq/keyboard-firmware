import Foundation

/// Practice material for a ladder.
///
/// Letters are practised in words, which is what words are for.  Digits and marks cannot
/// be: no amount of searching finds words containing a `%`, and a line of bare `%%%` is
/// not something anyone types.  So they are *decorations* -- an ordinary word with the
/// mark done to it, in the shape it actually appears in.  `word,` `"word"` `word-word`
/// `50%` `$12`.  That teaches the chord and the context it belongs to at once.
///
/// Every character of every line comes from an unlocked item, so a drill can never ask for
/// a chord the ladder has not introduced.

/// A small deterministic generator.
///
/// The material has to be reproducible: a test that asserted anything about a line built
/// from `SystemRandomNumberGenerator` would be a test that sometimes passed.  SplitMix64,
/// which is four lines and good enough to shuffle a word list.
public struct DrillRandom: RandomNumberGenerator {
    private var state: UInt64
    public init(seed: UInt64) { self.state = seed }

    public mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}

public struct LadderMaker {
    private let layouts: Layouts
    private let variant: String
    /// The word list, most frequent first.
    private let words: [String]

    public init(layouts: Layouts, variant: String = "taipo", words: [String]? = nil) {
        self.layouts = layouts
        self.variant = variant
        self.words = words ?? DrillMaker.bundledWords()
    }

    /// A ladder's drill: a title and some lines.
    public func drill(
        _ ladder: Ladder, lines: Int = 8, wordsPerLine: Int = 7, seed: UInt64 = 0x7A190
    ) -> Drill {
        var rng = DrillRandom(seed: seed)
        var out = [String]()
        for _ in 0..<lines {
            let line = self.line(ladder, words: wordsPerLine, using: &rng)
            if !line.isEmpty { out.append(line) }
        }
        return Drill(title: ladder.title(), lines: out)
    }

    /// One line.
    ///
    /// Most of it is ordinary words from the unlocked alphabet, so the line reads as
    /// English and the hands get the alternation practice that is the whole point of the
    /// layout.  Woven through it: one word for each focus letter, and one decoration for
    /// each focus mark or digit.  That is the "less insistent" part -- the focus items get
    /// worked every line, but they never take the line over.
    public func line(_ ladder: Ladder, words count: Int, using rng: inout DrillRandom)
        -> String
    {
        let alphabet = Set(
            ladder.unlocked.filter { $0.stage == .letter }.flatMap { $0.label })
        let pool = words.filter { word in
            !word.isEmpty && word.allSatisfy { alphabet.contains($0) }
        }
        guard !pool.isEmpty else { return "" }

        let focusLetters = ladder.focus.filter { $0.stage == .letter }.map(\.label)
        var tokens = [String]()
        /// Slots holding a word chosen for a focus letter, which a decoration must not take.
        var reserved = Set<Int>()
        for i in 0..<count {
            if i < focusLetters.count,
                let word = pick(pool, containing: Character(focusLetters[i]), using: &rng)
            {
                tokens.append(word)
                reserved.insert(i)
            } else {
                tokens.append(pick(pool, using: &rng))
            }
        }

        // Decorations: the focus marks and digits first, then one already-unlocked extra
        // to keep what has been learned in circulation.  Capped at half the line, because
        // a line that is more punctuation than words stops being typing practice.
        var extras = ladder.focus.filter { $0.stage != .letter }
        let older = ladder.unlocked.filter { $0.stage != .letter && !extras.contains($0) }
        if let one = older.randomElement(using: &rng) { extras.append(one) }
        extras = Array(extras.prefix(max(1, count / 2)))

        let digits = ladder.unlocked.filter { $0.stage == .digit }.map(\.label)
        // A decoration replaces the word in its slot, so it may not have one that was
        // chosen for a focus letter: the line would then work the mark and quietly drop
        // the letter it was also meant to be practising.
        var slots = tokens.indices.filter { !reserved.contains($0) }.shuffled(using: &rng)
        for extra in extras {
            guard let slot = slots.popLast() else { break }
            tokens[slot] = decorate(
                extra.label, word: tokens[slot], other: pick(pool, using: &rng),
                digits: digits, using: &rng)
        }
        return tokens.joined(separator: " ")
    }

    // MARK: - Choosing words

    /// A word from the pool, biased toward the frequent end.
    ///
    /// Squaring a uniform draw pulls the choice toward the front of the list, so the lines
    /// read like English rather than like a dictionary, while still reaching far enough
    /// down to keep them from repeating.
    private func pick(_ pool: [String], using rng: inout DrillRandom) -> String {
        let u = Double(rng.next() >> 11) / Double(1 << 53)
        return pool[min(pool.count - 1, Int(u * u * Double(pool.count)))]
    }

    /// The same, restricted to words using a particular letter.
    private func pick(
        _ pool: [String], containing letter: Character, using rng: inout DrillRandom
    ) -> String? {
        let matching = pool.filter { $0.contains(letter) }
        guard !matching.isEmpty else { return nil }
        return pick(matching, using: &rng)
    }

    // MARK: - Decorations

    /// A number using the unlocked digits, always containing `digit`.
    ///
    /// One to three of them, and never a leading zero unless zero is all there is -- a
    /// drill that asked for `007` would be teaching a shape nobody types.
    func number(_ digit: String, digits: [String], using rng: inout DrillRandom) -> String {
        // No longer than the digits available, so the first digit on the ladder gives `0`
        // rather than `000` -- a shape nobody types, and no more practice than `0` is.
        let length = 1 + Int(rng.next() % UInt64(max(1, min(3, digits.count))))
        var out = [digit]
        while out.count < length {
            out.append(digits.randomElement(using: &rng) ?? digit)
        }
        out.shuffle(using: &rng)
        if out.count > 1, out[0] == "0" {
            let nonZero = digits.filter { $0 != "0" }
            if let swap = nonZero.randomElement(using: &rng) {
                out[0] = out.firstIndex(of: swap).map { i -> String in
                    out[i] = "0"
                    return swap
                } ?? swap
            }
        }
        return out.joined()
    }

    /// One word with a mark or a number done to it, in the shape the mark appears in.
    ///
    /// `other` is a second word for the marks that join two things, and `digits` is what
    /// is available to the marks that want a number.  A mark whose natural shape needs a
    /// number falls back to a word when no digit is unlocked yet, rather than reaching for
    /// a chord the ladder has not introduced.
    func decorate(
        _ label: String, word: String, other: String, digits: [String],
        using rng: inout DrillRandom
    ) -> String {
        func numeric() -> String? {
            guard let d = digits.randomElement(using: &rng) else { return nil }
            return number(d, digits: digits, using: &rng)
        }
        switch label {
        case ".": return "\(word)."
        case ",": return "\(word),"
        case "'": return "\(word)'s"
        case "?": return "\(word)?"
        case "!": return "\(word)!"
        case ":": return "\(word):"
        case ";": return "\(word);"
        case "\"": return "\"\(word)\""
        case "()": return "(\(word))"
        case "[]": return "[\(word)]"
        case "{}": return "{\(word)}"
        case "<>": return "<\(word)>"
        case "`": return "`\(word)`"
        case "~": return "~\(word)"
        case "-", "/", "\\", "_", "*", "&", "+", "=", "|", "@":
            return "\(word)\(label)\(other)"
        case "%": return numeric().map { "\($0)%" } ?? "\(word)%"
        case "$": return numeric().map { "$\($0)" } ?? "$\(word)"
        case "#": return numeric().map { "#\($0)" } ?? "#\(word)"
        case "^": return numeric().map { "\(word)^\($0)" } ?? "\(word)^"
        default:
            // A digit, or a mark with no shape of its own yet.
            if digits.contains(label) {
                return number(label, digits: digits, using: &rng)
            }
            return "\(word)\(label)"
        }
    }
}
