import Foundation

/// Practice material for the Orsy ladder.
///
/// Every word of a line is one the table can write with unlocked patterns only, so a
/// line never asks for a shape or a rule the ladder has not introduced.  `perFocus`
/// words for each focus item, an opener that does not begin with one, and ordinary words
/// between; no decorations yet, as punctuation is the Dosh escape and not on this ladder.
///
/// Three words per focus item rather than one: with one, half of every line was filler
/// from a pool of forty words, and a line read as a tour of `in`, `it` and `is` with the
/// thing being practised somewhere in the middle of it.  With two items in focus that is
/// six of a seven-word line spent on what is being learned.
///
/// **And one word a line for whatever the pool starves.**  What an item gets once it
/// leaves the focus set is however often English happens to want it, and English is not
/// even-handed: of the 292 words writable at lesson 11, four contain the onset `y` --
/// `yes yet yen yields` -- and the frequency-biased draw put the earliest of them 26th,
/// so the onset came up twice in a hundred lines while `e␣` came up four hundred and
/// sixty-six times.  An item met once in fifty lines is one that has to be learned again
/// each time it is met, which is what happened to the onset `y` after it left focus.
///
/// So a line carries one more word, for the items the pool asks for least, in rotation.
/// One more rather than one of the focus placings: the items being learned are what the
/// line is for, and feeding a learned one at their expense would be robbing the ladder to
/// pay for its own past.  It is a word from the same pool drawn the same way, so the line
/// still reads as English and costs seven characters.  Measured over four thousand lines
/// at lesson 11, it takes the onset `y` from 2.0 appearances a hundred lines to 18.4, the
/// least practised item of the twenty-nine out of focus from 2.0 to 16.0, and the spread
/// between the most and least practised from 227:1 to 33:1.
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
        // The rotation is worked out once for the block: it depends on the pool, which
        // does not change while the ladder stands still, and a rotation re-derived per
        // line would be the same list every time anyway.
        let rotation = self.rotation(ladder)
        for turn in 0..<lines {
            let line = self.line(
                ladder, words: wordsPerLine, rotation: rotation, turn: turn, using: &rng)
            if !line.isEmpty { out.append(line) }
        }
        return Drill(title: ladder.title(), lines: out)
    }

    /// The words a ladder can ask for.
    public func pool(_ ladder: OrsyLadder) -> [OrsyWords.Word] {
        let unlocked = ladder.unlockedKeys
        return words.words.filter { $0.patterns.isSubset(of: unlocked) }
    }

    /// One line.
    ///
    /// `turn` is which line of the block this is, which is what walks the rotation on;
    /// a caller that only wants a line can leave it, and get the first item of it.
    public func line(
        _ ladder: OrsyLadder, words wordCount: Int, rotation: [String]? = nil, turn: Int = 0,
        using rng: inout DrillRandom
    ) -> String {
        let pool = self.pool(ladder)
        guard !pool.isEmpty else { return "" }
        let focusKeys = ladder.focus.map(\.key)
        let placings = focusKeys.count * Self.perFocus
        // The starved item this line is carrying, if there is one.  Slot `placings + 1`
        // is its own: taking a focus placing for it would be robbing the item being
        // learned to feed one that has been.
        let rotation = rotation ?? self.rotation(ladder)
        let circulating = rotation.isEmpty ? nil : rotation[turn % rotation.count]
        let count = max(wordCount, placings + (circulating == nil ? 1 : 2))
        var tokens = [String]()
        for i in 0..<count {
            // Slot 0 is the opener; the focus words follow, round robin, so that every
            // item gets its first word before any gets its second.
            if i > 0, i - 1 < placings,
                let word = pick(
                    pool.filter { $0.patterns.contains(focusKeys[(i - 1) % focusKeys.count]) },
                    using: &rng)
            {
                tokens.append(word.text)
            } else if i == 0 {
                // The first stroke of a line measures nothing, so the opener is not a
                // focus word.
                let rest = pool.filter { w in !focusKeys.contains { w.patterns.contains($0) } }
                tokens.append((pick(rest, using: &rng) ?? pick(pool, using: &rng))!.text)
            } else if i == placings + 1, let key = circulating,
                let word = pick(pool.filter { $0.patterns.contains(key) }, using: &rng)
            {
                tokens.append(word.text)
            } else {
                tokens.append(pick(pool, using: &rng)!.text)
            }
        }
        // The trim may not reach the opener, the focus words or the circulating one.
        let keep = placings + (circulating == nil ? 1 : 2)
        while tokens.joined(separator: " ").count > Self.maxCharacters, tokens.count > keep {
            tokens.removeLast()
        }
        return tokens.joined(separator: " ")
    }

    // MARK: - What the pool starves

    /// The unlocked items the pool asks for least, most starved first.
    ///
    /// Focus items are left out: they are being worked six times a line already.  So are
    /// items no word in the pool can reach at all -- there is nothing a line could do for
    /// them, and the ladder does not hold them against it either.
    public func rotation(_ ladder: OrsyLadder) -> [String] {
        let pool = self.pool(ladder)
        guard !pool.isEmpty else { return [] }
        let focusKeys = Set(ladder.focus.map(\.key))
        let expected = Self.expectation(pool)
        let candidates = ladder.unlocked.map(\.key)
            .filter { !focusKeys.contains($0) }
            .compactMap { key in expected[key].map { (key, $0) } }
            .sorted { $0.1 < $1.1 }
        guard !candidates.isEmpty else { return [] }
        // Starved is measured against the pool's own middle rather than a number written
        // down here: what counts as neglected depends on how evenly the pool is spread,
        // and the pool changes with every unlock.  At lesson 11 the middling item is
        // reached by a word in nine, so the bar is a word in eighteen, and six items are
        // under it -- from the onset `y` at a word in sixty-nine up to the onset `c`.
        let bar = candidates[candidates.count / 2].1 / 2
        return candidates.prefix { $0.1 < bar }.prefix(Self.rotationSize).map(\.0)
    }

    /// How often the draw would ask for each pattern, per word drawn.
    ///
    /// Worked out rather than sampled.  `pick` takes `u * u * n` of a list ordered most
    /// frequent first, so the chance of landing on the word at `i` is
    /// `sqrt((i+1)/n) - sqrt(i/n)`, and a pattern's share is the sum of that over the
    /// words that use it.  Exact, and a single pass over the pool.
    static func expectation(_ pool: [OrsyWords.Word]) -> [String: Double] {
        let n = Double(pool.count)
        var out = [String: Double]()
        for (i, word) in pool.enumerated() {
            let weight = (Double(i + 1) / n).squareRoot() - (Double(i) / n).squareRoot()
            for key in word.patterns { out[key, default: 0] += weight }
        }
        return out
    }

    /// How many items the rotation may hold.
    ///
    /// One slot a line shared ten ways is one appearance in ten lines apiece; sharing it
    /// more ways than that would put the items back where they were and rescue none of
    /// them.  Fewer is the usual case, and a pool that spreads itself evenly gives an
    /// empty rotation and a line as it was.
    static let rotationSize = 10

    static let maxCharacters = 90

    /// How many words a line carries for each focus item.
    public static let perFocus = 3

    /// A word from the pool, biased toward the frequent end, as `LadderMaker` draws.
    private func pick(_ pool: [OrsyWords.Word], using rng: inout DrillRandom) -> OrsyWords.Word? {
        guard !pool.isEmpty else { return nil }
        let u = Double(rng.next() >> 11) / Double(1 << 53)
        return pool[min(pool.count - 1, Int(u * u * Double(pool.count)))]
    }
}
