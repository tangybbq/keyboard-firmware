import Foundation

/// The Orsy ladder: patterns and rules, in the lesson plan's order, unlocked as the ones
/// already out are reached.
///
/// The same idea as `Ladder`, with the same two departures from keybr -- several items in
/// flight, and reach rather than speed as the gate -- and the same unlock rule, so the two
/// behave alike.  The order is the lesson plan in `orsy-words.json`, which is also what
/// the printed drill sheets follow.
///
/// **One item, one reading.**  A shape on the outer five keys spells an onset on the left
/// hand and a coda on the right, and those are two items here, adjacent in the order.  The
/// movement is the same, so it was tempting to teach them as one, but what the two readings
/// spell is not always the same thing -- `h` and `st`, `ind` and `nd` -- and their purposes
/// diverge further the more of the theory is in play.  Teaching them apart also means a
/// word that exercises one cannot be counted as practice for the other, which is what was
/// quietly happening.
///
/// **An item the material cannot reach is not held against the ladder.**  The onset `x` has
/// exactly one word in the whole table, `anxiety`, which also needs the last lesson's rule,
/// while the coda `x` has words from halfway up; the onset `ck` has no word at all, ever.
/// Such an item counts as reached, so it neither stalls the unlock nor takes a focus place,
/// and it comes back into the reckoning -- unreached, at the front of the focus ranking --
/// as soon as the pool grows to include a word that uses it.
///
/// Nothing is stored: the unlocked set is a pure function of `OrsySkillModel`.

/// What kind of thing an Orsy ladder item teaches, which is also the reading it measures.
public enum OrsyStage: String, Sendable {
    case onset, coda, second, vowel, rule
}

public struct OrsyLadderItem: Equatable, Sendable {
    /// What it spells, or the rule's name.
    public let label: String
    /// The skill key it is measured by.
    public let key: String
    public let stage: OrsyStage
    /// The lesson it belongs to.
    public let lesson: Int

    public init(label: String, key: String, stage: OrsyStage, lesson: Int) {
        self.label = label
        self.key = key
        self.stage = stage
        self.lesson = lesson
    }
}

public struct OrsyLadder: Sendable {
    public struct Options: Sendable {
        /// How many items are unlocked before any typing has been done: the first lesson.
        public var initial: Int?
        /// How many items may be unlearned at once.  Also the size of the focus set.
        ///
        /// Two, which with the allowance of one less leaves exactly one item short of the
        /// gate at a time and one place for polishing something past it.  Three was
        /// inherited from the Taipo ladder, where an item is one chord that types one
        /// letter; an Orsy item is bigger and interferes with a Dosh habit, so it wants
        /// the line to itself.
        public var focus: Int

        /// Whether the ladder may put new material out.
        ///
        /// On, which is the ladder working as it always has: one item beyond what has
        /// been reached is unlocked, and the focus set is mostly that item.  Off, the
        /// ladder stops at the last item the writer has reached, and the block is spent
        /// on what is already out -- for an afternoon when the ones lately acquired are
        /// not settled yet, or simply when nothing new is wanted.
        ///
        /// Nothing is lost by turning it off: the ladder's place is a pure function of
        /// the logs either way, so the item it was about to introduce is waiting, in the
        /// same order, whenever it is turned back on.
        public var introduce: Bool

        public init(initial: Int? = nil, focus: Int = 2, introduce: Bool = true) {
            self.initial = initial
            self.focus = focus
            self.introduce = introduce
        }
    }

    public let items: [OrsyLadderItem]
    public let unlockedCount: Int
    public let focus: [OrsyLadderItem]
    public let options: Options
    public let lessons: [OrsyWords.Lesson]

    public var unlocked: ArraySlice<OrsyLadderItem> { items.prefix(unlockedCount) }
    public var complete: Bool { unlockedCount >= items.count }

    /// Every skill key an unlocked item measures, which is what a word must be made of.
    public var unlockedKeys: Set<String> { Set(unlocked.map(\.key)) }

    /// The lesson the ladder has reached: the one holding the last unlocked item.
    public var lesson: OrsyWords.Lesson? {
        guard let last = unlocked.last else { return lessons.first }
        return lessons[last.lesson]
    }

    public init(
        words: OrsyWords, theory: OrsyTheory, skill: OrsySkillModel,
        options: Options = Options()
    ) {
        self.options = options
        self.lessons = words.lessons
        let items = OrsyLadder.order(words: words, theory: theory)
        self.items = items

        /// The keys some word writable with `unlocked` uses: what a line can ask for.
        func exercisable(_ unlocked: ArraySlice<OrsyLadderItem>) -> Set<String> {
            let keys = Set(unlocked.map(\.key))
            var out = Set<String>()
            for word in words.words where word.patterns.isSubset(of: keys) {
                out.formUnion(word.patterns)
            }
            return out
        }
        func reached(_ item: OrsyLadderItem, _ exercisable: Set<String>) -> Bool {
            !exercisable.contains(item.key) || skill.reached(item.key)
        }

        // Unlock while there is room: see `Ladder` for why the allowance is one less than
        // the focus set holds.  The pool grows with each unlock, so what counts as short
        // is re-read each time round.
        let allowance = max(1, options.focus - 1)
        let initial =
            options.initial ?? words.lessons.first?.items.reduce(0) { $0 + $1.count } ?? 7
        var count = min(initial, items.count)
        var reach = exercisable(items.prefix(count))
        while count < items.count {
            let short = items.prefix(count).filter { !reached($0, reach) }.count
            guard short < allowance else { break }
            let grown = exercisable(items.prefix(count + 1))
            // Reviewing only: the item about to come out is new material exactly when the
            // pool would then ask for it and the writer has not reached it, so that is
            // where the ladder stops.  One the material cannot reach yet is not new in
            // any sense the writer would notice, and is stepped over as usual.
            if !options.introduce, !reached(items[count], grown) { break }
            count += 1
            reach = grown
        }
        self.unlockedCount = count
        let exercisableNow = reach

        let out = Array(items.prefix(count))
        let ranked = out.indices.sorted {
            let (a, b) = (skill.confidence(out[$0].key), skill.confidence(out[$1].key))
            return a != b ? a < b : $0 < $1
        }
        // Only items a line can actually work: one the pool cannot reach would rank first
        // on its confidence of zero and then get no practice at all.
        let eligible = ranked.filter { exercisableNow.contains(out[$0].key) }
        var chosen = Array(eligible.filter { !skill.reached(out[$0].key) }.prefix(options.focus))
        for i in eligible
        where chosen.count < options.focus && skill.reached(out[i].key)
            && skill.confidence(out[i].key) < 1
        {
            chosen.append(i)
        }
        if chosen.isEmpty { chosen = Array(eligible.prefix(options.focus)) }
        if chosen.isEmpty { chosen = Array(ranked.prefix(options.focus)) }
        self.focus = chosen.map { out[$0] }
    }

    public func headline() -> String { "Orsy — \(unlockedCount) of \(items.count)" }

    public func title() -> String {
        let names = focus.map { "\"\($0.label)\"" }.joined(separator: " ")
        return "\(headline()); on \(names)"
    }

    /// The items, in the lesson plan's order, labelled by what they spell.
    ///
    /// One per key, so a shape's onset and coda are two adjacent items.
    static func order(words: OrsyWords, theory: OrsyTheory) -> [OrsyLadderItem] {
        let tables = theory.tables
        var out = [OrsyLadderItem]()
        for (number, lesson) in words.lessons.enumerated() {
            for key in lesson.items.flatMap({ $0 }) {
                let parts = key.split(separator: ":", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                let (series, name) = (parts[0], parts[1])
                let label: String
                let stage: OrsyStage
                switch series {
                case "s1":
                    guard let onset = tables.outer.first(where: { $0.michela == name })?.onset
                    else { continue }
                    label = onset
                    stage = .onset
                case "s4":
                    let coda = tables.outer.first { $0.michela == name }?.coda ?? ""
                    guard !coda.isEmpty else { continue }
                    label = coda
                    stage = .coda
                case "s2":
                    label = tables.second.first { $0.michela == name }?.spells ?? name
                    stage = .second
                case "s3":
                    let vowel = tables.vowel.first { $0.michela == name }
                    label = (vowel?.text ?? name) + (vowel?.endsWord == true ? "␣" : "")
                    stage = .vowel
                default:
                    label = name.replacingOccurrences(of: "_", with: " ")
                    stage = .rule
                }
                out.append(OrsyLadderItem(label: label, key: key, stage: stage, lesson: number))
            }
        }
        return out
    }
}
