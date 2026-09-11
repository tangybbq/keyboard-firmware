import Foundation

/// The Orsy ladder: patterns and rules, in the lesson plan's order, unlocked as the ones
/// already out are reached.
///
/// The same idea as `Ladder`, with the same two departures from keybr -- several items in
/// flight, and reach rather than speed as the gate -- and the same unlock rule, so the two
/// behave alike.  What differs is the item: a shape on the outer five keys is one item
/// with two readings, its onset and its coda, learned together because it is the same
/// movement of the same fingers; a second character, a vowel and a rule are one key each.
/// The order is the lesson plan in `orsy-words.json`, which is also what the printed
/// drill sheets follow.
///
/// **An item is judged by the readings the material can reach.**  A shape's two readings
/// are not equally available: the onset `l` has a hundred and fifty words in the table
/// and not one of them is writable until the mirrored vowel and the vowel `o` are out,
/// while the coda `l` has words from the start.  Judged by both readings the item sat
/// in the focus set waiting on a reading no line could contain, taking a place and an
/// unlock allowance with it.  So reach, confidence and the weak key all look only at the
/// keys some word of the current pool uses; a reading the pool cannot exercise yet is
/// not held against the item, and comes back into the reckoning when the pool grows to
/// include it.
///
/// Nothing is stored: the unlocked set is a pure function of `OrsySkillModel`.

/// What kind of thing an Orsy ladder item teaches.
public enum OrsyStage: String, Sendable {
    case shape, second, vowel, rule
}

public struct OrsyLadderItem: Equatable, Sendable {
    /// What it spells, or the rule's name.
    public let label: String
    /// The skill keys it is measured by.
    public let keys: [String]
    public let stage: OrsyStage
    /// The lesson it belongs to.
    public let lesson: Int

    public init(label: String, keys: [String], stage: OrsyStage, lesson: Int) {
        self.label = label
        self.keys = keys
        self.stage = stage
        self.lesson = lesson
    }
}

public struct OrsyLadder: Sendable {
    public struct Options: Sendable {
        /// How many items are unlocked before any typing has been done: the first lesson.
        public var initial: Int?
        /// How many items may be unlearned at once.  Also the size of the focus set.
        public var focus: Int

        public init(initial: Int? = nil, focus: Int = 3) {
            self.initial = initial
            self.focus = focus
        }
    }

    public let items: [OrsyLadderItem]
    public let unlockedCount: Int
    public let focus: [OrsyLadderItem]
    /// For each focus item, the reading the material should work: its weakest key.
    ///
    /// A shape is two readings, and a word that uses either would satisfy the item, so
    /// a line built for "the item" can drill the coda every time and leave the onset --
    /// the one the ladder is actually waiting on -- never typed.
    public let focusKeys: [String]
    public let options: Options
    public let lessons: [OrsyWords.Lesson]

    public var unlocked: ArraySlice<OrsyLadderItem> { items.prefix(unlockedCount) }
    public var complete: Bool { unlockedCount >= items.count }

    /// Every skill key an unlocked item measures, which is what a word must be made of.
    public var unlockedKeys: Set<String> { Set(unlocked.flatMap(\.keys)) }

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

        /// The keys some word writable with `unlocked` uses: the readings a line can ask
        /// for.
        func exercisable(_ unlocked: ArraySlice<OrsyLadderItem>) -> Set<String> {
            let keys = Set(unlocked.flatMap(\.keys))
            var out = Set<String>()
            for word in words.words where word.patterns.isSubset(of: keys) {
                out.formUnion(word.patterns)
            }
            return out
        }
        /// An item's keys that can be practised, or all of them when none can.
        func live(_ item: OrsyLadderItem, _ exercisable: Set<String>) -> [String] {
            let usable = item.keys.filter(exercisable.contains)
            return usable.isEmpty ? item.keys : usable
        }
        func reached(_ item: OrsyLadderItem, _ exercisable: Set<String>) -> Bool {
            let usable = item.keys.filter(exercisable.contains)
            return usable.isEmpty || usable.allSatisfy(skill.reached)
        }
        func confidence(_ item: OrsyLadderItem, _ exercisable: Set<String>) -> Double {
            live(item, exercisable).map(skill.confidence).min() ?? 0
        }

        // Unlock while there is room: see `Ladder` for why the allowance is one less than
        // the focus set holds.  The pool grows with each unlock, so what counts as short
        // is re-read each time round.
        let allowance = max(1, options.focus - 1)
        let initial = options.initial ?? words.lessons.first?.items.count ?? 7
        var count = min(initial, items.count)
        var reach = exercisable(items.prefix(count))
        while count < items.count {
            let short = items.prefix(count).filter { !reached($0, reach) }.count
            guard short < allowance else { break }
            count += 1
            reach = exercisable(items.prefix(count))
        }
        self.unlockedCount = count
        let exercisableNow = reach
        func reached(_ item: OrsyLadderItem) -> Bool { reached(item, exercisableNow) }
        func confidence(_ item: OrsyLadderItem) -> Double { confidence(item, exercisableNow) }

        let out = Array(items.prefix(count))
        let ranked = out.indices.sorted {
            let (a, b) = (confidence(out[$0]), confidence(out[$1]))
            return a != b ? a < b : $0 < $1
        }
        var chosen = Array(ranked.filter { !reached(out[$0]) }.prefix(options.focus))
        for i in ranked where chosen.count < options.focus && reached(out[i]) && confidence(out[i]) < 1 {
            chosen.append(i)
        }
        if chosen.isEmpty { chosen = Array(ranked.prefix(options.focus)) }
        self.focus = chosen.map { out[$0] }
        self.focusKeys = self.focus.map { item in
            let usable = live(item, exercisableNow)
            return usable.min { skill.confidence($0) < skill.confidence($1) } ?? item.keys[0]
        }
    }

    public func headline() -> String { "Orsy — \(unlockedCount) of \(items.count)" }

    public func title() -> String {
        let names = focus.map { "\"\($0.label)\"" }.joined(separator: " ")
        return "\(headline()); on \(names)"
    }

    /// The items, in the lesson plan's order, labelled by what they spell.
    static func order(words: OrsyWords, theory: OrsyTheory) -> [OrsyLadderItem] {
        let tables = theory.tables
        var out = [OrsyLadderItem]()
        for (number, lesson) in words.lessons.enumerated() {
            for keys in lesson.items {
                guard let first = keys.first else { continue }
                let parts = first.split(separator: ":", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                let (series, name) = (parts[0], parts[1])
                let label: String
                let stage: OrsyStage
                switch series {
                case "s1", "s4":
                    let shape = tables.outer.first { $0.michela == name }
                    let onset = shape?.onset
                    let coda = shape?.coda ?? ""
                    // The onset reading, or the coda's when there is none, or both when
                    // they differ: `h/st`.
                    if let onset, onset != coda, !coda.isEmpty {
                        label = "\(onset)/\(coda)"
                    } else {
                        label = onset ?? "-\(coda)"
                    }
                    stage = .shape
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
                out.append(OrsyLadderItem(label: label, keys: keys, stage: stage, lesson: number))
            }
        }
        return out
    }
}
