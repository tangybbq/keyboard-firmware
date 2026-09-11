import Foundation

/// Practice material for the Orsy ladder.
///
/// Every word of a line is one the table can write with unlocked patterns only, so a
/// line never asks for a shape or a rule the ladder has not introduced.  `perFocus`
/// words for each focus item, an opener that does not begin with one, and ordinary words
/// between; no decorations yet, as punctuation is the Dosh escape and not on this ladder.
///
/// Two words per focus item rather than one: with one, and three items in focus, half of
/// every line was filler from a pool of forty words, and a line read as a tour of `in`,
/// `it` and `is` with the thing being practised somewhere in the middle of it.
public struct OrsyLadderMaker {
    private let words: OrsyWords

    public init(words: OrsyWords) {
        self.words = words
    }

    public func drill(
        _ ladder: OrsyLadder, lines: Int = 8, wordsPerLine: Int = 6, seed: UInt64 = 0x0A51
    ) -> Drill {
        var rng = DrillRandom(seed: seed)
        var out = [String]()
        for _ in 0..<lines {
            let line = self.line(ladder, words: wordsPerLine, using: &rng)
            if !line.isEmpty { out.append(line) }
        }
        return Drill(title: ladder.title(), lines: out)
    }

    /// The words a ladder can ask for.
    public func pool(_ ladder: OrsyLadder) -> [OrsyWords.Word] {
        let unlocked = ladder.unlockedKeys
        return words.words.filter { $0.patterns.isSubset(of: unlocked) }
    }

    public func line(_ ladder: OrsyLadder, words wordCount: Int, using rng: inout DrillRandom)
        -> String
    {
        let pool = self.pool(ladder)
        guard !pool.isEmpty else { return "" }
        // One word per focus item, for the reading the ladder is waiting on; any reading
        // of the item only when no word in the pool uses that one.
        let focusKeys: [Set<String>] = zip(ladder.focus, ladder.focusKeys).map { item, key in
            pool.contains { $0.patterns.contains(key) } ? [key] : Set(item.keys)
        }
        let placings = focusKeys.count * Self.perFocus
        let count = max(wordCount, placings + 1)
        var tokens = [String]()
        for i in 0..<count {
            // Slot 0 is the opener; the focus words follow, round robin, so that every
            // item gets its first word before any gets its second.
            if i > 0, i - 1 < placings,
                let word = pick(
                    pool.filter { !$0.patterns.isDisjoint(with: focusKeys[(i - 1) % focusKeys.count]) },
                    using: &rng)
            {
                tokens.append(word.text)
            } else if i == 0 {
                // The first stroke of a line measures nothing, so the opener is not a
                // focus word.
                let rest = pool.filter { w in !focusKeys.contains { !w.patterns.isDisjoint(with: $0) } }
                tokens.append((pick(rest, using: &rng) ?? pick(pool, using: &rng))!.text)
            } else {
                tokens.append(pick(pool, using: &rng)!.text)
            }
        }
        while tokens.joined(separator: " ").count > Self.maxCharacters, tokens.count > placings + 2 {
            tokens.removeLast()
        }
        return tokens.joined(separator: " ")
    }

    static let maxCharacters = 90

    /// How many words a line carries for each focus item.
    static let perFocus = 2

    /// A word from the pool, biased toward the frequent end, as `LadderMaker` draws.
    private func pick(_ pool: [OrsyWords.Word], using rng: inout DrillRandom) -> OrsyWords.Word? {
        guard !pool.isEmpty else { return nil }
        let u = Double(rng.next() >> 11) / Double(1 << 53)
        return pool[min(pool.count - 1, Int(u * u * Double(pool.count)))]
    }
}
