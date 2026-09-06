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
/// **The focus set can be narrowed by hand.**  `Options.stages` says which kinds of item
/// may hold a place in it.  Once the ladder is complete every item competes for the three
/// places on confidence alone, and the marks win that contest permanently: a mark is
/// measured at a clause boundary where the writer is deciding what comes next, so the
/// thinking pause is charged to it and its median stays two to three times a letter's
/// however well it is known.  The result is a finished ladder that drills nothing but
/// punctuation and digits.  Turning a stage off takes it out of the running for the focus
/// set -- and out of the material -- without touching what has been learned, since the
/// unlocked set is derived from the logs and not from what is being drilled today.
///
/// Nothing is stored.  The unlocked set is a pure function of `SkillModel`, which is a
/// pure function of the logs, so progress survives without a progress file and cannot
/// disagree with the typing it came from.

/// What kind of thing a ladder item teaches.
public enum LadderStage: String, CaseIterable, Codable, Sendable {
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
        /// Which kinds of item may be drilled.
        ///
        /// A lens on the focus set and on the material, not on the ladder: an item of a
        /// stage that is turned off keeps whatever it has learned, and comes back to the
        /// same place when it is turned on again.  What it does not do is take one of the
        /// three focus places, or get decorated into a line.
        ///
        /// It does not touch unlocking either, so turning a stage off while the ladder is
        /// still climbing can stall it -- the item it is waiting on stops being drilled
        /// and so never reaches the gate.  Those items are reported in `deferred` rather
        /// than left to be puzzled over.
        public var stages: Set<LadderStage>

        public init(
            initial: Int = 7, focus: Int = 3, interleaveAfter: Int = 10,
            interleaveEvery: Int = 3, stages: Set<LadderStage> = Set(LadderStage.allCases)
        ) {
            self.initial = initial
            self.focus = focus
            self.interleaveAfter = interleaveAfter
            self.interleaveEvery = interleaveEvery
            self.stages = stages
        }
    }

    /// The whole ladder, in the order it is taught.
    public let items: [LadderItem]
    /// How many of them are unlocked.
    public let unlockedCount: Int
    /// The weakest unlocked items, which the material is weighted toward.
    public let focus: [LadderItem]
    /// Unlocked items short of the gate that `Options.stages` keeps out of the focus set.
    ///
    /// These are what the ladder is waiting on, so while there are any of them it cannot
    /// advance.  Worth reporting: a writer who has turned digits off and finds the count
    /// stuck should be told which switch is holding it, not left to conclude the ladder
    /// is broken.
    public let deferred: [LadderItem]
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
        // typed, so this settles at the allowance and cannot run away.
        //
        // The allowance is one *less* than the focus set holds, and that one place is the
        // whole point: every item short of the gate has to be in the focus set, or it gets
        // no deliberate practice and can never reach it -- an item unlocked and then left
        // out stalls the ladder behind it.  Filling the set with those alone leaves
        // nothing for the item that is past the gate but still slow, so the ladder carries
        // one fewer at a time and keeps the last place for polish.
        let allowance = max(1, options.focus - 1)
        var count = min(options.initial, items.count)
        while count < items.count {
            let short = items.prefix(count).filter { !reached($0) }.count
            guard short < allowance else { break }
            count += 1
        }
        self.unlockedCount = count

        // The focus set: everything short of the gate first, then the weakest item that is
        // past it but not yet fluent.
        //
        // That order matters.  The items short of the gate are what the ladder is waiting
        // on, so leaving one out costs the writer every item behind it; polish only fills
        // what they leave.  Weakest is by confidence, which does count speed even though
        // the gate does not, so a chord that is reached but slow is what the spare place
        // goes to -- and it goes on being practised after the ladder has moved past it.
        let out = Array(items.prefix(count))
        let ranked = out.indices.sorted {
            let (a, b) = (confidence(out[$0]), confidence(out[$1]))
            // Ties broken by position, so the set does not reshuffle between two chords
            // nothing is known about.
            return a != b ? a < b : $0 < $1
        }
        // Both rules draw from the stages that are switched on, and only those.
        let eligible = ranked.filter { options.stages.contains(out[$0].stage) }
        var chosen = Array(eligible.filter { !reached(out[$0]) }.prefix(options.focus))
        // Then whatever is past the gate and still short of fluent.  An item that is fully
        // learned is not offered the place: spending a line on something already done
        // teaches nothing.
        for i in eligible
        where chosen.count < options.focus && reached(out[i]) && confidence(out[i]) < 1 {
            chosen.append(i)
        }
        // A ladder with everything reached and fluent has nothing to point at by either
        // rule, and practice still has to go somewhere.
        if chosen.isEmpty { chosen = Array(eligible.prefix(options.focus)) }
        // And a filter that matches nothing at all -- every stage switched off, or a stage
        // whose items are all still locked -- is not a reason to stop drilling.
        if chosen.isEmpty { chosen = Array(ranked.prefix(options.focus)) }
        self.focus = chosen.map { out[$0] }
        self.deferred = out.indices
            .filter { !options.stages.contains(out[$0].stage) && !reached(out[$0]) }
            .map { out[$0] }
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
