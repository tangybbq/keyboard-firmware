import Foundation

/// Turning what the writer keeps getting wrong into something to practise.
///
/// The material is derived rather than written down: the collector's own logs say which
/// chords get confused with which, so the drills follow the writer's hands instead of a
/// syllabus.  Nothing is generated ahead of time and nothing is stored -- re-reading the
/// logs is cheap, and a file that had to be regenerated would be a second copy of the
/// truth to keep in step.

/// Two chords the writer confuses, and how often.
///
/// Unordered.  Typing `o` for `s` and `s` for `o` is one problem with the ring finger, not
/// two, and a drill would treat it as one.
public struct ConfusionPair: Equatable, Sendable {
    public let a: UInt16
    public let b: UInt16
    public let shape: Confusion
    public let count: Int
}

/// What the logs say about which chords get mixed up.
public struct ConfusionModel {
    /// Ranked, most confused first.
    public let pairs: [ConfusionPair]
    /// How many corrections were seen in all, so a thin model can say it is thin.
    public let corrections: Int
    /// How many sessions were read.
    public let sessions: Int

    /// Only the shapes a drill can do anything about.
    ///
    /// A row slip and a finger slip are things the hand can be taught.  A chord recalled
    /// wrongly is a different problem -- vocabulary, not technique -- and practising the
    /// pair would not touch it.
    public static let drillable: Set<Confusion> = [.wrongRow, .wrongFinger]

    /// Replay every log in a directory and count the confusions.
    ///
    /// Files are read in name order, which for `yyyy-MM-dd.txt` is date order.  Each is
    /// split into sessions and each session replayed on its own engine: nothing is
    /// measured across a join, and a backspace does not correct something typed yesterday.
    public static func build(
        logDirectory: URL, layouts: Layouts, variant: String = "taipo",
        shapes: Set<Confusion> = drillable
    ) -> ConfusionModel {
        let files =
            (try? FileManager.default.contentsOfDirectory(
                at: logDirectory, includingPropertiesForKeys: nil))?
            .filter { $0.pathExtension == "txt" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent } ?? []

        var counts: [ConfusionKey: Int] = [:]
        var corrections = 0
        var sessions = 0
        let scanner = CorrectionScanner(layouts: layouts, variant: variant)

        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            for session in KeyLogFile.sessions(from: text, layouts: layouts) {
                sessions += 1
                let engine = ChordEngine(layouts: layouts)
                var codes = [UInt16]()
                for entry in session.entries {
                    switch entry {
                    case .marker(let m): engine.marker(m.name, value: m.value)
                    case .key(let e):
                        codes += engine.feed(key: e.key, press: e.press, timeMs: e.timeMs)
                            .map(\.code)
                    }
                }
                codes += engine.finish().map(\.code)

                for c in scanner.scan(codes) {
                    corrections += 1
                    guard let shape = c.confusion, shapes.contains(shape),
                        let d = c.deleted, let r = c.replacement
                    else { continue }
                    counts[ConfusionKey(min(d, r), max(d, r), shape), default: 0] += 1
                }
            }
        }

        let pairs = counts
            .map { ConfusionPair(a: $0.key.a, b: $0.key.b, shape: $0.key.shape, count: $0.value) }
            .sorted {
                // Ties broken by the chord codes so the order is stable run to run; a
                // drill list that reshuffles itself is hard to trust.
                ($0.count, $1.a, $1.b) > ($1.count, $0.a, $0.b)
            }
        return ConfusionModel(pairs: pairs, corrections: corrections, sessions: sessions)
    }

    struct ConfusionKey: Hashable {
        let a: UInt16
        let b: UInt16
        let shape: Confusion
        init(_ a: UInt16, _ b: UInt16, _ shape: Confusion) {
            self.a = a
            self.b = b
            self.shape = shape
        }
    }
}

/// One practice item: a heading and the lines to type.
public struct Drill: Equatable, Sendable {
    public let title: String
    public let lines: [String]
}

/// Building practice material for a confusion.
public struct DrillMaker {
    private let layouts: Layouts
    private let variant: String
    /// The word list, most frequent first.
    private let words: [String]
    /// Each word's cheapest chord sequence, worked out once.
    private let segmented: [(word: String, codes: [UInt16])]

    public init(layouts: Layouts, variant: String = "taipo", words: [String]? = nil) {
        self.layouts = layouts
        self.variant = variant
        let list = words ?? DrillMaker.bundledWords()
        self.words = list
        self.segmented = list.map {
            ($0, DrillTarget(text: $0, layouts: layouts, variant: variant).units.map(\.code))
        }
    }

    /// The MonkeyType `english_10k` list, ordered by frequency, shipped in this module.
    public static func bundledWords() -> [String] {
        struct List: Decodable { let words: [String] }
        guard let url = Bundle.module.url(forResource: "english_10k", withExtension: "json"),
            let data = try? Data(contentsOf: url),
            let list = try? JSONDecoder().decode(List.self, from: data)
        else { return [] }
        return list.words
    }

    /// What a chord types, or nil if it types no characters.
    private func text(_ code: UInt16) -> String? {
        layouts.chord(code, variant: variant)?.action.types
    }

    /// How the pair reads as a heading: `o / s -- ring finger, wrong row`.
    public func title(_ pair: ConfusionPair) -> String {
        let name = { (c: UInt16) -> String in
            self.text(c).map { "\"\($0)\"" }
                ?? self.layouts.chord(c, variant: self.variant)?.keys.joined(separator: "+")
                ?? String(format: "0x%03x", c)
        }
        return "\(name(pair.a)) / \(name(pair.b)) — \(fingers(pair)), \(pair.shape.rawValue)"
    }

    /// The fingers the two chords disagree about, named.
    private func fingers(_ pair: ConfusionPair) -> String {
        let t = Confusion.fingerStates(pair.a)
        let m = Confusion.fingerStates(pair.b)
        let names = ["pinky", "ring", "middle", "index", "left thumb", "right thumb"]
        let differ = (0..<6).filter { t[$0] != m[$0] }.map { names[$0] }
        switch differ.count {
        case 0: return "same fingers"
        case 1: return differ[0]
        default: return differ.joined(separator: " and ")
        }
    }

    /// The warmup: the two chords against each other and nothing else.
    ///
    /// Short groups rather than one long alternation, because the discrimination is what
    /// is being trained and a run of `ososos` becomes a rhythm the fingers can ride.  Every
    /// group uses both, and the doubles are in there on purpose: `oo` before `s` is where
    /// the wrong row actually gets pressed.
    public func warmup(_ pair: ConfusionPair) -> String? {
        guard let a = text(pair.a), let b = text(pair.b), !a.isEmpty, !b.isEmpty else {
            return nil
        }
        let shapes = ["ab", "ba", "aba", "bab", "aab", "bba", "abb", "baa"]
        return shapes
            .map { $0.map { $0 == "a" ? a : b }.joined() }
            .joined(separator: " ")
    }

    /// Words whose cheapest chord sequence uses both of the confused chords.
    ///
    /// Both, and as close together as possible: the mistake is a discrimination, so a word
    /// that asks for one chord and then the other a letter later trains it and a word that
    /// merely contains each somewhere does not.
    public func words(_ pair: ConfusionPair, count: Int, perLine: Int = 6) -> [String] {
        // How far apart the two chords are, capped: past four chords the word is no longer
        // asking the hand to tell them apart, it just happens to contain both.
        let maxGap = 4
        var buckets: [Int: [(word: String, rank: Int)]] = [:]
        for (rank, entry) in segmented.enumerated() {
            guard entry.word.allSatisfy({ $0.isLowercase && $0.isLetter }) else { continue }
            let ia = entry.codes.indices.filter { entry.codes[$0] == pair.a }
            let ib = entry.codes.indices.filter { entry.codes[$0] == pair.b }
            guard !ia.isEmpty, !ib.isEmpty else { continue }
            guard let gap = ia.flatMap({ x in ib.map { abs($0 - x) } }).min(), gap <= maxGap
            else { continue }
            buckets[gap, default: []].append((entry.word, rank))
        }
        for gap in buckets.keys { buckets[gap]?.sort { $0.rank < $1.rank } }

        // Round robin across the distances, nearest first, so the list is not twenty
        // repetitions of one spelling.  Taking strictly by distance gave `d`/`g` as
        // `knowledge budget edge bridge judge lodge ridge` -- perfectly on target and
        // almost the same word seven times.
        var chosen = [String]()
        var offset = 0
        let gaps = buckets.keys.sorted()
        while chosen.count < count, gaps.contains(where: { offset < (buckets[$0]?.count ?? 0) })
        {
            for gap in gaps where chosen.count < count {
                if let words = buckets[gap], offset < words.count {
                    chosen.append(words[offset].word)
                }
            }
            offset += 1
        }
        return stride(from: 0, to: chosen.count, by: perLine).map {
            chosen[$0..<min($0 + perLine, chosen.count)].joined(separator: " ")
        }
    }

    /// A pair's whole drill: the warmup, then the words.
    public func drill(_ pair: ConfusionPair, wordCount: Int = 24) -> Drill {
        var lines = [String]()
        if let warmup = warmup(pair) { lines.append(warmup) }
        lines += words(pair, count: wordCount)
        return Drill(title: title(pair), lines: lines)
    }

    /// The programme: a drill for each of the most confused pairs, worst first.
    public func programme(_ model: ConfusionModel, pairs: Int = 6, wordCount: Int = 24)
        -> [Drill]
    {
        model.pairs.prefix(pairs).map { drill($0, wordCount: wordCount) }
            .filter { !$0.lines.isEmpty }
    }
}
