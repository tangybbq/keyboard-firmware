import Foundation

/// Adaptive practice: an ordered ladder of things to learn, unlocked as the ones already
/// out get learned, with the material weighted toward whichever are weakest.
///
/// This is keybr's idea with two deliberate changes.
///
/// **Several items in flight, not one.**  keybr introduces a letter and then hammers it
/// until its confidence clears the bar, which is effective and wearing.  Here up to
/// `Options.focus` items are short of the gate at a time and the material rotates between
/// them, so a stubborn one holds up nothing: the next unlocks as soon as there is room,
/// and the stubborn one keeps its place in the focus set.  The unlock rule is exactly
/// "fewer than `focus` not yet reached", which makes that true by construction.
///
/// **Reach unlocks, speed does not.**  The gate is `SkillModel.reached` -- typed enough,
/// and not often taken back -- and says nothing about how fast.  Two reasons, both
/// measured rather than supposed.  A mark lands at a clause boundary, where the writer is
/// deciding what comes next, so the pause to think is charged to the punctuation: the
/// period and the comma measured at 786ms and 1012ms after 154 and 120 uses, against
/// letters at 385ms with far less practice.  And what decides whether the writer has to
/// break off and switch back to a table they already know is whether they can type a thing
/// *at all*; being slow at it is what practice is for, and practice needs it in the
/// material first.  Speed still decides the focus set, the hint's fade and every number on
/// the screen, so a reached-but-slow item goes on being drilled after the ladder has moved
/// past it.
///
/// **Numbers and punctuation are on the same ladder.**  They are not a mode you switch on
/// once the letters are done; they are spliced into the order, one every
/// `Options.interleaveEvery` items after the first `Options.interleaveAfter`, so the
/// period and the comma arrive while there are still letters to come.  Material for them
/// is a different shape -- a digit wants a number and a bracket wants something to wrap --
/// so `LadderMaker` decorates ordinary words rather than trying to find words containing a
/// `%`.
///
/// Nothing is stored.  The unlocked set is a pure function of `SkillModel`, which is a
/// pure function of the logs, so progress survives without a progress file and cannot
/// disagree with the typing it came from.

/// What kind of thing a ladder item teaches.
public enum LadderStage: String, Sendable {
    case letter
    case digit
    case punctuation
}

/// One thing to learn: usually one chord, sometimes a matched pair.
///
/// Brackets and quotes come as pairs because that is how they are typed and how they are
/// useful; `(` on its own has nothing to practise on.  A pair is learned when both of its
/// chords are.
public struct LadderItem: Equatable, Sendable {
    public let label: String
    public let codes: [UInt16]
    public let stage: LadderStage

    public init(label: String, codes: [UInt16], stage: LadderStage) {
        self.label = label
        self.codes = codes
        self.stage = stage
    }
}

public struct Ladder: Sendable {
    public struct Options: Sendable {
        /// How many items are unlocked before any typing has been done.
        ///
        /// Seven, which for both tables is the single-key chords -- and, because the
        /// layout put the commonest letters on the easiest keys, also `e t a o i n s`,
        /// enough of an alphabet for a real vocabulary from the first line.
        public var initial: Int
        /// How many items may be unlearned at once.  Also the size of the focus set.
        public var focus: Int
        /// How many letters come before the first digit or mark.
        public var interleaveAfter: Int
        /// After that, one digit or mark every this many items.
        public var interleaveEvery: Int

        public init(
            initial: Int = 7, focus: Int = 3, interleaveAfter: Int = 10,
            interleaveEvery: Int = 3
        ) {
            self.initial = initial
            self.focus = focus
            self.interleaveAfter = interleaveAfter
            self.interleaveEvery = interleaveEvery
        }
    }

    /// The whole ladder, in the order it is taught.
    public let items: [LadderItem]
    /// How many of them are unlocked.
    public let unlockedCount: Int
    /// The weakest unlocked items, which the material is weighted toward.
    public let focus: [LadderItem]
    public let options: Options

    public var unlocked: ArraySlice<LadderItem> { items.prefix(unlockedCount) }
    public var complete: Bool { unlockedCount >= items.count }

    /// Build the ladder for a table, and walk it as far as the skill model allows.
    public init(
        layouts: Layouts, variant: String = "taipo", skill: SkillModel,
        options: Options = Options()
    ) {
        self.options = options
        let items = Ladder.order(layouts: layouts, variant: variant, options: options)
        self.items = items

        func reached(_ item: LadderItem) -> Bool { item.codes.allSatisfy(skill.reached) }
        func confidence(_ item: LadderItem) -> Double {
            item.codes.map(skill.confidence).min() ?? 0
        }

        // Unlock while there is room.  A new item cannot be reached until it has been
        // typed, so this settles at exactly `focus` short of it and cannot run away.
        var count = min(options.initial, items.count)
        while count < items.count {
            let short = items.prefix(count).filter { !reached($0) }.count
            guard short < options.focus else { break }
            count += 1
        }
        self.unlockedCount = count

        // The focus set is the weakest unlocked items, and weakest is by confidence --
        // which does count speed, even though the unlock gate does not.  So an item that
        // has been reached but is still slow goes on being practised after the ladder has
        // moved past it.
        //
        // At most `focus - 1` of the set may be items not yet reached, so that a slow one
        // keeps a place rather than being crowded out by whatever was unlocked last.  The
        // top-up lifts that when there is nothing else to point at, which is how a cold
        // start still gets a full set.
        let out = Array(items.prefix(count))
        let ranked = out.indices.sorted {
            let (a, b) = (confidence(out[$0]), confidence(out[$1]))
            // Ties broken by position, so the set does not reshuffle between two chords
            // nothing is known about.
            return a != b ? a < b : $0 < $1
        }
        let notReached = ranked.filter { !reached(out[$0]) }
        // The weakest item that is past the gate but not yet fluent -- which is what the
        // reserved slot is for.  An item that is fully learned does not want the slot: it
        // would be spending the line on something already done.
        let reserve = ranked.first { reached(out[$0]) && confidence(out[$0]) < 1 }

        var chosen = Array(notReached.prefix(reserve == nil ? options.focus : options.focus - 1))
        if let reserve, chosen.count < options.focus { chosen.append(reserve) }
        // Top up when there was nothing to reserve for and nothing new to work on, which
        // is a ladder with everything reached: practice still has to go somewhere.
        for i in ranked where chosen.count < options.focus && !chosen.contains(i) {
            chosen.append(i)
        }
        self.focus = chosen.map { out[$0] }
    }

    /// Where the ladder has got to, without saying what it is working on.
    ///
    /// Split from `title()` because a screen that lists the focus items in their own right
    /// does not want them named twice.
    public func headline() -> String { "Ladder — \(unlockedCount) of \(items.count)" }

    /// How the ladder reads as a heading, for somewhere with room for only one line.
    public func title() -> String {
        let names = focus.map { "\"\($0.label)\"" }.joined(separator: " ")
        return "\(headline()); on \(names)"
    }

    // MARK: - The order

    /// Marks in the order prose needs them.  Anything the table types that is not in here
    /// follows, in table order, so a new symbol chord joins the ladder without being
    /// listed twice.
    static let markOrder = [
        ".", ",", "'", "-", "\"", "?", ":", "!", ";", "()", "/", "_", "*", "&", "%", "$",
        "#", "@", "+", "=", "[]", "{}", "<>", "\\", "|", "~", "`", "^",
    ]

    /// The two halves of each pair, so they can be taught as one item.
    static let pairs: [String: (String, String)] = [
        "()": ("(", ")"), "[]": ("[", "]"), "{}": ("{", "}"), "<>": ("<", ">"),
    ]

    /// Every letter, digit and mark the table can type, in teaching order.
    ///
    /// Letters come by English frequency, which is derived from the bundled word list
    /// rather than written down, so it cannot drift from the corpus the material is drawn
    /// from.  Capitals are left out: they are a thumb away from a letter already on the
    /// ladder, and sentence material teaches them without a slot of their own.
    static func order(layouts: Layouts, variant: String, options: Options) -> [LadderItem] {
        // What types each single character, first chord wins -- the same rule
        // `DrillTarget` segments with, so a ladder item is a chord a drill can ask for.
        var byText: [String: UInt16] = [:]
        for chord in layouts.variants[variant]?.chords ?? [] {
            guard let types = chord.action.types, types.count == 1 else { continue }
            if byText[types] == nil { byText[types] = chord.code }
        }

        let letters = letterFrequency()
            .filter { byText[$0] != nil }
            .map { LadderItem(label: $0, codes: [byText[$0]!], stage: .letter) }

        let digits = (0...9).map(String.init)
            .filter { byText[$0] != nil }
            .map { LadderItem(label: $0, codes: [byText[$0]!], stage: .digit) }

        // The marks: the listed order first, then anything else the table types.
        var marks = [LadderItem]()
        var used = Set<String>()
        func mark(_ label: String) -> LadderItem? {
            let parts = pairs[label].map { [$0.0, $0.1] } ?? [label]
            let codes = parts.compactMap { byText[$0] }
            guard codes.count == parts.count else { return nil }
            parts.forEach { used.insert($0) }
            return LadderItem(label: label, codes: codes, stage: .punctuation)
        }
        for label in markOrder {
            if let item = mark(label) { marks.append(item) }
        }
        for chord in layouts.variants[variant]?.chords ?? [] {
            guard let types = chord.action.types, types.count == 1,
                let ch = types.first, ch.isASCII, !ch.isLetter, !ch.isNumber,
                !ch.isWhitespace, ch.isPunctuation || ch.isSymbol,
                !used.contains(types), byText[types] == chord.code
            else { continue }
            used.insert(types)
            marks.append(LadderItem(label: types, codes: [chord.code], stage: .punctuation))
        }

        // Interleave.  Letters carry the first stretch on their own, then a digit or a
        // mark every few items -- alternating between the two so neither waits for the
        // other to run out.
        var extras = [LadderItem]()
        var d = digits.makeIterator()
        var m = marks.makeIterator()
        // Two marks to every digit, marks first.  Prose wants a period and a comma long
        // before it wants a 7, and an even split put a `0` on the ladder ahead of the
        // comma.
        var sinceDigit = 0
        while true {
            let wantDigit = sinceDigit >= 2
            let next = wantDigit ? d.next() ?? m.next() : m.next() ?? d.next()
            guard let next else { break }
            extras.append(next)
            sinceDigit = next.stage == .digit ? 0 : sinceDigit + 1
        }

        var out = [LadderItem]()
        var letter = letters.makeIterator()
        var extra = extras.makeIterator()
        var sinceExtra = 0
        while true {
            let wantExtra =
                out.count >= options.interleaveAfter && sinceExtra >= options.interleaveEvery
            if wantExtra, let next = extra.next() {
                out.append(next)
                sinceExtra = 0
                continue
            }
            if let next = letter.next() {
                out.append(next)
                sinceExtra += 1
                continue
            }
            guard let next = extra.next() else { break }
            out.append(next)
        }
        return out
    }

    /// Letters by how much of the bundled word list they account for.
    ///
    /// Weighted by `1/rank`, a stand-in for the frequency the list itself does not carry;
    /// it only has to get the order roughly right, and the order it gives is the familiar
    /// `e t a o i n s ...`.
    static func letterFrequency(words: [String]? = nil) -> [String] {
        let list = words ?? DrillMaker.bundledWords()
        var weight = [Character: Double]()
        for (rank, word) in list.enumerated() {
            let w = 1.0 / Double(rank + 1)
            for ch in word where ch.isLetter { weight[ch, default: 0] += w }
        }
        let all = "abcdefghijklmnopqrstuvwxyz".map { $0 }
        return all
            .sorted { (weight[$0] ?? 0, $1) > (weight[$1] ?? 0, $0) }
            .map(String.init)
    }
}
