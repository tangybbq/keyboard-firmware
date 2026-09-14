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
/// a chord the ladder has not introduced.  A stage switched off in `Ladder.Options.stages`
/// is not decorated into a line at all -- neither as a focus item nor for circulation --
/// so asking for letters only gives plain words, which is what asking for letters only
/// ought to give.  Letters are exempt from that: they are what words are made of, and the
/// switch is about what is *drilled*, not about what the vocabulary may use.
///
/// **What the letters get is English, and English is not even-handed.**  A mark or a digit
/// has a slot held for it once it is learned; a letter has only however often the words
/// want it, which after 211,000 chords came out at 322 uses of `z` against 18,080 of `e`
/// -- fifty-six to one -- and the six rarest letters measured at a 640ms median against
/// the six commonest at 244ms.  A letter out of focus and out of favour with English gets
/// no practice at all, and it shows in the timings.
///
/// So one plain word of the line is drawn for whichever unlocked letter the vocabulary
/// asks for least, in rotation.  It costs nothing: a filler word is there to be read, and
/// which one it is was never doing any work.  A decoration may still land on it -- every
/// decoration keeps its word -- and only the trim is taught to leave it alone.
///
/// On the finished Dosh ladder the rotation comes out as `z j x k w v`, which is the six
/// letters measured slowest, and over four thousand lines it takes `j` from 5.8
/// appearances a hundred lines to 13.8 and `z` from 6.0 to 15.7, narrowing the spread
/// across the alphabet from 93:1 to 39:1.  The line is 66.5 characters instead of 66.4:
/// the same words, one of them chosen rather than drawn.

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
        // Worked out once for the block: it depends on the unlocked alphabet, which does
        // not move while the ladder stands still.
        let rotation = self.rotation(ladder)
        for turn in 0..<lines {
            let line = self.line(
                ladder, words: wordsPerLine, rotation: rotation, turn: turn, using: &rng)
            if !line.isEmpty { out.append(line) }
        }
        return Drill(title: ladder.title(), lines: out)
    }

    /// One line.
    ///
    /// Most of it is ordinary words from the unlocked alphabet, so the line reads as
    /// English and the hands get the alternation practice that is the whole point of the
    /// layout.  Woven through it: one word for each focus letter, one decoration for each
    /// focus mark or digit, one decoration for a mark already learned, and one plain word
    /// for a letter the vocabulary neglects.  That is the "less insistent" part -- the
    /// focus items get worked every line, but they never take the line over.
    ///
    /// `turn` is which line of the block this is, which is what walks the letter rotation
    /// on; a caller that only wants a line can leave it, and get the first of them.
    public func line(
        _ ladder: Ladder, words wordCount: Int, rotation: [Character]? = nil, turn: Int = 0,
        using rng: inout DrillRandom
    ) -> String {
        let alphabet = Set(
            ladder.unlocked.filter { $0.stage == .letter }.flatMap { $0.label })
        let pool = words.filter { word in
            !word.isEmpty && word.allSatisfy { alphabet.contains($0) }
        }
        guard !pool.isEmpty else { return "" }

        let focusLetters = ladder.focus.filter { $0.stage == .letter }.map(\.label)
        let focusExtras = ladder.focus.filter { $0.stage != .letter }
        // The characters the line is being built to work, which is what may not open it.
        let drilled = Set(ladder.focus.compactMap { $0.label.first })
        // A longer line when it has more marks to carry.  Every mark wants `perLine`
        // placings and every line wants two plain words left in it, and with three marks
        // in focus at once a seven-word line cannot have both -- they came out at one
        // apiece, which is the problem this is meant to fix.  Bounded, because a line
        // nobody wants to read teaches nothing either.  One slot more than the placings
        // themselves need, because the first word is never one of them -- see `opener`.
        let count = min(
            wordCount + 4,
            max(wordCount, focusLetters.count + 1, focusExtras.count * Self.perLine + 3))
        var tokens = [String]()
        /// Slots holding a word chosen for a focus letter, which a decoration must not take.
        var reserved = Set<Int>()
        for i in 0..<count {
            // Slot 0 is the opener, so the focus letters begin at slot 1.
            if i > 0, i - 1 < focusLetters.count,
                let word = pick(
                    pool, containing: Character(focusLetters[i - 1]), using: &rng)
            {
                tokens.append(word)
                reserved.insert(i)
            } else if i == 0 {
                tokens.append(opener(pool, avoiding: drilled, using: &rng))
            } else {
                tokens.append(pick(pool, using: &rng))
            }
        }

        // One plain word for the letter the vocabulary is neglecting most, this line's
        // turn of the rotation.  It takes a slot that was holding filler, so it costs the
        // line nothing: which filler word it was is the one choice in a line that was
        // never carrying any weight.  It is not reserved -- a decoration may land on it,
        // and every decoration keeps the word it decorates -- but the trim may not drop
        // it, or the practice it was put there for goes with it.
        let rotation = rotation ?? self.rotation(ladder)
        var circulated: Int?
        if !rotation.isEmpty,
            let slot = tokens.indices.filter({ $0 > 0 && !reserved.contains($0) })
                .randomElement(using: &rng),
            let word = pick(pool, containing: rotation[turn % rotation.count], using: &rng)
        {
            tokens[slot] = word
            circulated = slot
        }

        // A decoration replaces the word in its slot, so it may not have one that was
        // chosen for a focus letter: the line would then work the mark and quietly drop
        // the letter it was also meant to be practising.  Nor the opening slot: several
        // of the marks go on the front of their word, which would make the mark the
        // first chord of the line.
        var slots = tokens.indices
            .filter { $0 > 0 && !reserved.contains($0) }
            .shuffled(using: &rng)
        // With digits switched off, the marks whose natural shape wants a number fall back
        // to their word forms -- `word%` rather than `50%` -- the same way they do before
        // the first digit is unlocked.
        let digits =
            ladder.options.stages.contains(.digit)
            ? ladder.unlocked.filter { $0.stage == .digit }.map(\.label) : []

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
            $0.stage != .letter && ladder.options.stages.contains($0.stage)
                && !ladder.focus.contains($0)
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
            let plain = tokens.indices.filter {
                !reserved.contains($0) && !decorated.contains($0) && $0 != circulated
            }
            guard plain.count > 2, let drop = plain.last else { break }
            tokens.remove(at: drop)
            reserved = Set(reserved.map { $0 > drop ? $0 - 1 : $0 })
            decorated = Set(decorated.map { $0 > drop ? $0 - 1 : $0 })
            circulated = circulated.map { $0 > drop ? $0 - 1 : $0 }
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

    // MARK: - What the vocabulary starves

    /// The unlocked letters the vocabulary asks for least, most neglected first.
    ///
    /// Focus letters are left out -- a word is chosen for each of them every line -- and
    /// so is everything else on the ladder: a mark or a digit has the circulation slot
    /// already, and nothing in the pool contains one to draw for anyway.  Letters
    /// switched off in `Ladder.Options.stages` are left out too: the switch says what may
    /// be drilled, and this is drilling.
    public func rotation(_ ladder: Ladder) -> [Character] {
        guard ladder.options.stages.contains(.letter) else { return [] }
        let alphabet = Set(
            ladder.unlocked.filter { $0.stage == .letter }.flatMap { $0.label })
        let pool = words.filter { word in
            !word.isEmpty && word.allSatisfy { alphabet.contains($0) }
        }
        guard !pool.isEmpty else { return [] }
        let focused = Set(ladder.focus.filter { $0.stage == .letter }.flatMap { $0.label })
        let expected = Self.expectation(pool)
        let candidates = alphabet.subtracting(focused)
            .map { ($0, expected[$0] ?? 0) }
            .sorted { $0.1 != $1.1 ? $0.1 < $1.1 : $0.0 < $1.0 }
        guard !candidates.isEmpty else { return [] }
        // Against the alphabet's own middle rather than a number written down here: what
        // counts as neglected depends on how wide the unlocked alphabet is, and it widens
        // with every letter.  See `OrsyLadderMaker.rotation`, which does the same thing
        // for the patterns.
        let bar = candidates[candidates.count / 2].1 / 2
        return candidates.prefix { $0.1 < bar }.prefix(Self.rotationSize).map(\.0)
    }

    /// How often the draw would put each letter in a word, per word drawn.
    ///
    /// Worked out rather than sampled, as `OrsyLadderMaker.expectation` explains: `pick`
    /// takes `u * u * n` of a list ordered most frequent first, so the chance of landing
    /// on the word at `i` is `sqrt((i+1)/n) - sqrt(i/n)`.  Occurrences rather than words,
    /// because `letter` is typed twice by `letter` and that is two chords.
    static func expectation(_ pool: [String]) -> [Character: Double] {
        let n = Double(pool.count)
        var out = [Character: Double]()
        for (i, word) in pool.enumerated() {
            let weight = (Double(i + 1) / n).squareRoot() - (Double(i) / n).squareRoot()
            for ch in word { out[ch, default: 0] += weight }
        }
        return out
    }

    /// How many letters the rotation may hold; see `OrsyLadderMaker.rotationSize`.
    static let rotationSize = 10

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

    /// The word a line opens with: an ordinary one that does not *begin* with anything
    /// in focus.
    ///
    /// The skill model times a chord by the gap from the chord before it, so the first
    /// chord of a line is not evidence about anything -- there is no previous key to time
    /// in from, and what gap there is is the writer reading the new line.  A placing there
    /// is therefore a placing that measures nothing, and with the marks it is worse than
    /// nothing: a line asks for a focus mark two or three times, so losing one of those to
    /// the front loses a third of the practice the line was built to give.
    ///
    /// Falling back to any word at all, for the ladder's first lines: with three letters
    /// unlocked and all three in focus there may be nothing else a line could open with,
    /// and an opening word matters less than having a line.
    private func opener(
        _ pool: [String], avoiding drilled: Set<Character>, using rng: inout DrillRandom
    ) -> String {
        let rest = pool.filter { word in word.first.map { !drilled.contains($0) } ?? false }
        return pick(rest.isEmpty ? pool : rest, using: &rng)
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
