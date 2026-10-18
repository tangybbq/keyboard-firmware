import Foundation

/// The Orsy word ladder: whole words, most used in prose first, each unlocked once the
/// ones before it are fast.
///
/// The pattern ladder teaches the rules; this one is for speed, which comes a word at a
/// time once the rules are known.  Over the logs to 2026-10-12 the wait before a stroke
/// was 3.5 s for a shape made five times or fewer and 1.1 s for one made more than 250
/// times, but the pattern ladder's material was spread over three thousand words, most
/// of them rare: `the` came up twice in four weeks.  This ladder keeps a small set of
/// common words in front of the hands until they are fast, keybr's way, and adds the
/// next one as they get there.
///
/// **The gate is speed**, unlike the pattern ladder's, which is reach: there the job was
/// learning what each pattern does, here it is doing it quickly.  A word has passed once
/// its best windowed median (`WordSkill.bestMs`) is under its target.  The best rather
/// than the current median, so a slow sitting cannot lock words again; the current one
/// still decides which words are worked on.
///
/// **The target is per stroke**, the time per stroke times the strokes in the word's
/// division, since the time goes into the strokes and not the letters.  It was the time a
/// typing speed allows the characters, and that asked a two-stroke word for the speed of
/// a one-stroke word the same length: `all`, the first two-stroke word out, was allowed
/// the 1920ms that `the` was, while it ran at 1140ms a stroke against `the`'s 790 and the
/// first forty words' median of nearly 2000.
///
/// **Up to `inFlight` words below the target at once**, as the other ladders allow a few
/// unlearned items, so one stubborn word never stops the ladder.  The focus set is the
/// slowest of those, then the slowest of the rest, and a line is mostly the focus set,
/// with words already passed mixed in at their frequency: passing the target at twenty
/// uses is not the same as being automatic at two hundred and fifty.
///
/// Nothing is stored: where the ladder stands is a pure function of the logs.
public struct OrsyWordLadder: Sendable {
    public struct Options: Sendable, Equatable {
        /// How many words are out before any have passed.
        public var initial: Int
        /// How many unlocked words may be below the target at once.
        public var inFlight: Int
        /// How many words a line works on.  One more than `inFlight`, for polishing the
        /// slowest word already passed.
        public var focus: Int
        /// The time a stroke is allowed, from the stroke before; a word is allowed this
        /// for each stroke of its division.  1500ms is about 26 wpm at the table's 1.55
        /// strokes a word (`equivalentWpm`), where the ladder's first target, 25 wpm on
        /// characters, had been.
        public var strokeMs: UInt32

        public init(
            initial: Int = 10, inFlight: Int = 3, focus: Int = 4, strokeMs: UInt32 = 1500
        ) {
            self.initial = initial
            self.inFlight = inFlight
            self.focus = focus
            self.strokeMs = strokeMs
        }
    }

    /// Strokes per five-character word over English, the top 20,000 words weighted by
    /// frequency, as `bbq_orsy::Writer` divides them.  Only for saying what a stroke time
    /// comes to in words a minute.
    public static let strokesPerWord = 1.55

    /// What a time per stroke comes to in words a minute, on average.
    public static func equivalentWpm(strokeMs: UInt32) -> Double {
        60_000 / Double(max(1, strokeMs)) / strokesPerWord
    }

    /// One word, with what the logs say about it against its target.
    public struct Item: Equatable, Sendable {
        public let text: String
        /// The strokes in its division.
        public let strokes: Int
        /// The time it is allowed, from the end of the word before.
        public let targetMs: UInt32
        public let skill: WordSkill?

        /// How long it takes a stroke now.
        public var strokeMs: UInt32? { skill?.medianMs.map { $0 / UInt32(max(1, strokes)) } }

        /// It has been under the target, cleanly, over a full window.
        public var passed: Bool { skill?.bestMs.map { $0 <= targetMs } ?? false }

        /// How fast it is now, in words a minute.
        public var wpm: Double? {
            skill?.medianMs.map { Double(text.count + 1) / 5 * 60_000 / Double(max(1, $0)) }
        }

        /// How near it is to the target now, from 0 to 1: speed against the target,
        /// discounted for too few uses and for corrections past the bar.
        public var confidence: Double {
            guard let s = skill, let median = s.medianMs else { return 0 }
            let speed = min(1, Double(targetMs) / Double(max(1, median)))
            let exposure = min(1, Double(s.count) / Double(WordSamples.minUses))
            let bar = WordSamples.maxErrorRate
            let accuracy = s.errorRate <= bar ? 1 : bar / s.errorRate
            return speed * exposure * accuracy
        }
    }

    /// The ladder's words, in order.
    public let words: [OrsyWords.Word]
    public let unlockedCount: Int
    /// What lines work on, weakest first.
    public let focus: [Item]
    public let options: Options
    private let skill: OrsySkillModel

    public var unlocked: ArraySlice<OrsyWords.Word> { words.prefix(unlockedCount) }
    /// The word that comes out next.
    public var next: OrsyWords.Word? {
        unlockedCount < words.count ? words[unlockedCount] : nil
    }

    public init(words: OrsyWords, skill: OrsySkillModel, options: Options = Options()) {
        self.init(ladder: words.ladder, skill: skill, options: options)
    }

    init(ladder: [OrsyWords.Word], skill: OrsySkillModel, options: Options) {
        self.words = ladder
        self.skill = skill
        self.options = options
        var byText = [String: OrsyWords.Word]()
        for word in ladder where byText[word.text] == nil { byText[word.text] = word }
        self.byText = byText
        func item(_ word: OrsyWords.Word) -> Item { Self.item(word, skill: skill, options: options) }
        var count = min(options.initial, ladder.count)
        var below = ladder.prefix(count).filter { !item($0).passed }.count
        while count < ladder.count, below < options.inFlight {
            if !item(ladder[count]).passed { below += 1 }
            count += 1
        }
        self.unlockedCount = count
        let items = ladder.prefix(count).enumerated().map { ($0.offset, item($0.element)) }
        let ranked = items.sorted { a, b in
            if a.1.passed != b.1.passed { return !a.1.passed }
            if a.1.confidence != b.1.confidence { return a.1.confidence < b.1.confidence }
            return a.0 < b.0
        }
        self.focus = ranked.prefix(options.focus).map(\.1)
    }

    private let byText: [String: OrsyWords.Word]

    /// The time a word is allowed: the time per stroke for each stroke of its division.
    public static func targetMs(_ word: OrsyWords.Word, strokeMs: UInt32) -> UInt32 {
        UInt32(max(1, word.strokes.count)) * strokeMs
    }

    static func item(_ word: OrsyWords.Word, skill: OrsySkillModel, options: Options) -> Item {
        Item(
            text: word.text, strokes: word.strokes.count,
            targetMs: targetMs(word, strokeMs: options.strokeMs), skill: skill.word(word.text))
    }

    /// One of the ladder's words, with how it stands; nil for a word not on it.
    public func item(_ text: String) -> Item? {
        byText[text].map { Self.item($0, skill: skill, options: options) }
    }

    /// How many unlocked words are still under the target.
    public var below: Int {
        unlocked.filter { !Self.item($0, skill: skill, options: options).passed }.count
    }

    /// Where the ladder stands.
    public func headline() -> String {
        let place = "\(unlockedCount) of \(words.count) words"
        return "Orsy words — \(place) · \(below) below \(options.strokeMs)ms a stroke"
    }

    public func title() -> String {
        "\(headline()); on " + focus.map { "\"\($0.text)\"" }.joined(separator: " ")
    }
}

/// Practice lines for the word ladder.
///
/// Every word of a line is unlocked.  An opener that is not a focus word, since the first
/// word of a line follows the move to it and is not timed; `perFocus` of each focus word;
/// and `background` words already out, drawn toward the frequent end, so the words that
/// passed keep coming round.  In a random order, so the focus words do not come round the
/// same way every line, and never the same word twice running.
public struct OrsyWordLadderMaker {
    public init() {}

    static let perFocus = 2
    static let background = 3
    static let maxCharacters = 90

    public func drill(_ ladder: OrsyWordLadder, lines: Int = 16, seed: UInt64 = 0x0A51) -> Drill {
        var rng = DrillRandom(seed: seed)
        let out = (0..<lines).map { _ in line(ladder, using: &rng) }.filter { !$0.isEmpty }
        return Drill(title: ladder.title(), lines: out)
    }

    public func line(_ ladder: OrsyWordLadder, using rng: inout DrillRandom) -> String {
        let unlocked = ladder.unlocked.map(\.text)
        guard !unlocked.isEmpty else { return "" }
        let focus = ladder.focus.map(\.text)
        let rest = unlocked.filter { !focus.contains($0) }
        let pool = rest.isEmpty ? unlocked : rest

        let opener = pick(pool, using: &rng)
        var practice = [String]()
        for _ in 0..<Self.perFocus { practice += focus }
        var extra = (0..<Self.background).map { _ in pick(pool, using: &rng) }
        // Over length: the background goes first, then the second round of focus words.
        func length() -> Int { ([opener] + practice + extra).joined(separator: " ").count }
        while length() > Self.maxCharacters, !extra.isEmpty { extra.removeLast() }
        while length() > Self.maxCharacters, practice.count > focus.count { practice.removeLast() }

        // Drawn in a random order, never the same word twice running where there is a
        // choice.  A word with half of what is left has to go now, or there will be no
        // way to keep its copies apart; otherwise any word but the last will do.
        var remaining = practice + extra
        var words = [opener]
        while !remaining.isEmpty {
            let fresh = remaining.indices.filter { remaining[$0] != words.last }
            guard !fresh.isEmpty else {
                words += remaining
                break
            }
            let copies = Dictionary(fresh.map { (remaining[$0], 1) }, uniquingKeysWith: +)
            let most = copies.max { $0.value != $1.value ? $0.value < $1.value : $0.key > $1.key }!
            let from =
                most.value * 2 >= remaining.count
                ? fresh.filter { remaining[$0] == most.key } : fresh
            words.append(remaining.remove(at: from[Int(rng.next() % UInt64(from.count))]))
        }
        return words.joined(separator: " ")
    }

    /// A word, biased toward the frequent end, as the other makers draw.
    private func pick(_ pool: [String], using rng: inout DrillRandom) -> String {
        let u = Double(rng.next() >> 11) / Double(1 << 53)
        return pool[min(pool.count - 1, Int(u * u * Double(pool.count)))]
    }
}
