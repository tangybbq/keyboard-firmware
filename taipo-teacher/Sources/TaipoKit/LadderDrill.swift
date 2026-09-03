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
    public func line(_ ladder: Ladder, words wordCount: Int, using rng: inout DrillRandom)
        -> String
    {
        let alphabet = Set(
            ladder.unlocked.filter { $0.stage == .letter }.flatMap { $0.label })
        let pool = words.filter { word in
            !word.isEmpty && word.allSatisfy { alphabet.contains($0) }
        }
        guard !pool.isEmpty else { return "" }

        let focusLetters = ladder.focus.filter { $0.stage == .letter }.map(\.label)
        let focusExtras = ladder.focus.filter { $0.stage != .letter }
        // A longer line when it has more marks to carry.  Every mark wants `perLine`
        // placings and every line wants two plain words left in it, and with three marks
        // in focus at once a seven-word line cannot have both -- they came out at one
        // apiece, which is the problem this is meant to fix.  Bounded, because a line
        // nobody wants to read teaches nothing either.
        let count = min(
            wordCount + 4, max(wordCount, focusExtras.count * Self.perLine + 2))
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

        // A decoration replaces the word in its slot, so it may not have one that was
        // chosen for a focus letter: the line would then work the mark and quietly drop
        // the letter it was also meant to be practising.
        var slots = tokens.indices.filter { !reserved.contains($0) }.shuffled(using: &rng)
        let digits = ladder.unlocked.filter { $0.stage == .digit }.map(\.label)

        // The second word for the marks that join two things.  Kept short: `word*word`
        // puts a whole extra word in the line, and eight of those with the draw's usual
        // taste for `informational` is how a line reached 174 characters.
        let shortPool = pool.filter { $0.count <= 5 }
        var decorated = Set<Int>()

        /// Decorate one free word with an item, if there is a word left to spare.
        @discardableResult
        func place(_ item: LadderItem) -> Bool {
            guard let slot = slots.popLast() else { return false }
            tokens[slot] = decorate(
                item.label, word: tokens[slot],
                other: pick(shortPool.isEmpty ? pool : shortPool, using: &rng),
                digits: digits, using: &rng)
            decorated.insert(slot)
            return true
        }
        /// How many times the line already asks for an item, incidental uses included.
        func appearances(_ item: LadderItem) -> Int {
            guard let ch = item.label.first else { return 0 }
            return tokens.reduce(0) { $0 + $1.filter { $0 == ch }.count }
        }

        // Leave at least two plain words, or the line stops being typing practice.
        var budget = max(1, count - 2)
        let older = ladder.unlocked.filter {
            $0.stage != .letter && !ladder.focus.contains($0)
        }
        // One slot held back to keep what has just been learned in circulation.
        if !older.isEmpty && budget > 1 { budget -= 1 }

        // The focus marks and digits, round robin, until each is asked for as often as an
        // ordinary focus letter would be.  A letter is worked by every word that happens
        // to contain it, which measured at two to three uses a line; a mark is worked only
        // where one is deliberately put, so without this it got exactly one and took two
        // or three times as long to learn.
        //
        // Round robin, one apiece per pass, so that three of them share a line rather than
        // the first taking all of it.  A pass that places nothing ends the loop, which is
        // what stops it when the words run out before the targets are met.
        func length() -> Int { tokens.joined(separator: " ").count }
        var placing = true
        while budget > 0 && placing {
            placing = false
            for item in focusExtras where budget > 0 {
                guard appearances(item) < Self.perLine else { continue }
                // Past the floor, only while the line has room.  Several of the marks
                // join two words, so each placing is a whole extra word, and three of
                // them apiece for three marks is what ran a line to 174 characters.  The
                // floor is what keeps the point of all this: even a crowded line works
                // every focus mark more than the one time it used to.
                if appearances(item) >= Self.minPerLine, length() >= Self.maxCharacters {
                    continue
                }
                guard place(item) else { continue }
                budget -= 1
                placing = true
            }
        }

        // And one mark or digit that has already been learned, drawn with a bias toward
        // the ones learned most recently.
        //
        // A bias rather than a window.  Sharing the slot evenly between every mark ever
        // learned has each appearing a third of a line at fifteen items and a twentieth at
        // forty, which is not circulation; but taking only the newest few dropped the
        // period and the comma to exactly never, and those are the two marks most worth
        // keeping a hand in.  Squaring the draw favours what was learned lately without
        // letting anything fall off the end.
        if !older.isEmpty {
            let u = Double(rng.next() >> 11) / Double(1 << 53)
            let fromEnd = min(older.count - 1, Int(u * u * Double(older.count)))
            place(older[older.count - 1 - fromEnd])
        }

        // Trim to length by dropping filler.
        //
        // Only words that are doing nothing: never one chosen for a focus letter, never
        // one a mark was put on, and never the last two plain words -- the trim must not
        // be able to undo the practice the line was built for.  A line that cannot get
        // under the cap without giving one of those up stays long.
        while tokens.joined(separator: " ").count > Self.maxCharacters {
            let plain = tokens.indices.filter { !reserved.contains($0) && !decorated.contains($0) }
            guard plain.count > 2, let drop = plain.last else { break }
            tokens.remove(at: drop)
            reserved = Set(reserved.map { $0 > drop ? $0 - 1 : $0 })
            decorated = Set(decorated.map { $0 > drop ? $0 - 1 : $0 })
        }

        return tokens.joined(separator: " ")
    }

    /// How long a line may get.
    ///
    /// A drill line is typed in one go before Enter, so its length is how long the writer
    /// is committed for.  Lines grew past 170 characters once marks were being placed as
    /// often as letters -- five wrapped lines of the target, and too far to see the end of
    /// what you have started.
    static let maxCharacters = 110

    /// How often a focus mark or digit is asked for even in a line with no room.
    static let minPerLine = 2

    /// How often a focus mark or digit is asked for in a line.
    ///
    /// Three, which is what a focus *letter* measured at, once the words that happen to
    /// contain it are counted.  A mark is worked only where one is deliberately put, so
    /// without this it got exactly one placing a line and took two or three times as long
    /// to learn as a letter did.  The point is equal practice, not equal decoration.
    static let perLine = 3

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
            // A digit, kept beside its word rather than in place of it.  Replacing the
            // word cost the line a word -- which measurably thinned the letters at the
            // stages where three digits are placed -- and turned a line into `0 0 0` when
            // only one digit was unlocked and there was nothing else a number could be.
            // `page 12` is how a number is typed anyway.
            if digits.contains(label) {
                return "\(word) \(number(label, digits: digits, using: &rng))"
            }
            // A mark with no shape of its own yet.
            return "\(word)\(label)"
        }
    }
}
