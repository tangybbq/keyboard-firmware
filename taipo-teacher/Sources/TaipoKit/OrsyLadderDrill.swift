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
    /// The strokes the learner has made before, as `OrsySkillModel.strokeId`.
    ///
    /// Empty means the question cannot be asked -- no logs yet, or a caller that does
    /// not care -- and every candidate is then equally gentle, which is what the drill
    /// did before this existed.
    private let made: Set<UInt32>

    public init(words: OrsyWords, made: Set<UInt32> = []) {
        self.words = words
        self.made = made
    }

    /// How many new strokes a line may introduce.
    ///
    /// One.  An unlocked item is one thing to learn, but a *pattern* is not one stroke:
    /// the outer five combine with what is already known and bring a couple of dozen new
    /// strokes into the pool, while a Series 2 character multiplies against every vowel
    /// and coda there is.  Measured against a real ladder at lesson 17, the coda `b`
    /// brought 57 strokes of which 18 had never been made, and Series 2 `i` brought 112
    /// of which 110 had never been made.  Unlocking one item then means meeting a new
    /// stroke almost every word, which is not what learning one thing should feel like.
    static let newStrokesPerLine = 1

    /// How many of a word's strokes are not in `seen`.
    func novelty(_ word: OrsyWords.Word, seen: Set<UInt32>) -> Int {
        guard !made.isEmpty else { return 0 }
        return word.strokes.reduce(0) {
            $0 + (seen.contains(OrsySkillModel.strokeId(left: $1.0, right: $1.1)) ? 0 : 1)
        }
    }

    /// How many of a word's strokes the learner has never made.
    public func novelty(_ word: OrsyWords.Word) -> Int { novelty(word, seen: made) }

    /// The candidates that spend no more than `budget` on strokes not yet seen, or, when
    /// none of them can, the ones that spend the least.  Never empty if the input was not.
    ///
    /// The fallback is not a failure: for an item just unlocked, *every* word is new, and
    /// one of them has to go first.  What matters is that the line then goes on counting
    /// that stroke as seen, so the item's next word reuses it rather than reaching for
    /// another -- which is how a line comes to drill `ies` several times over instead of
    /// touring `ies`, `ion` and `ial` once each.
    func affordable(
        _ candidates: [OrsyWords.Word], budget: Int, seen: Set<UInt32>
    ) -> [OrsyWords.Word] {
        guard !made.isEmpty, !candidates.isEmpty else { return candidates }
        let within = candidates.filter { novelty($0, seen: seen) <= budget }
        if !within.isEmpty { return within }
        let least = candidates.map { novelty($0, seen: seen) }.min() ?? 0
        return candidates.filter { novelty($0, seen: seen) == least }
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
        // What the line has left to spend on strokes never made.  A focus word for a
        // freshly unlocked item spends it immediately, and the rest of the line is then
        // drawn from what the learner can already strike.
        var budget = Self.newStrokesPerLine
        // What the line has already put in front of the learner.  A stroke introduced by
        // one word is familiar by the next, so it stops being charged for.
        var seen = made
        func take(_ word: OrsyWords.Word) -> String {
            budget = max(0, budget - novelty(word, seen: seen))
            for (left, right) in word.strokes {
                seen.insert(OrsySkillModel.strokeId(left: left, right: right))
            }
            return word.text
        }
        for i in 0..<count {
            // Slot 0 is the opener; the focus words follow, round robin, so that every
            // item gets its first word before any gets its second.
            if i > 0, i - 1 < placings,
                let word = pick(
                    affordable(
                        pool.filter { $0.patterns.contains(focusKeys[(i - 1) % focusKeys.count]) },
                        budget: budget, seen: seen),
                    using: &rng)
            {
                tokens.append(take(word))
            } else if i == 0 {
                // The first stroke of a line measures nothing, so the opener is not a
                // focus word.
                let rest = pool.filter { w in !focusKeys.contains { w.patterns.contains($0) } }
                let word = pick(affordable(rest, budget: budget, seen: seen), using: &rng)
                    ?? pick(affordable(pool, budget: budget, seen: seen), using: &rng)
                tokens.append(take(word!))
            } else if i == placings + 1, let key = circulating,
                let word = pick(
                    affordable(pool.filter { $0.patterns.contains(key) }, budget: budget, seen: seen),
                    using: &rng)
            {
                tokens.append(take(word))
            } else {
                tokens.append(take(pick(affordable(pool, budget: budget, seen: seen), using: &rng)!))
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
