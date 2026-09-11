import SwiftUI
import TaipoKit

/// The Orsy drill screen: a line of words, typed against in strokes, judged by the text.
///
/// The same shape as the Taipo screen with the parts that do not apply taken out -- there
/// is no hand alternation to mark in a layout where every stroke uses both hands -- and
/// two things added: the stroke hint draws both hands, and a wrong stroke is explained by
/// Series, since "coda: l for t" is what the writer can act on.
struct OrsyDrillView: View {
    @ObservedObject var monitor: DeviceMonitor

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let drill = monitor.orsyDrill {
                if let ladder = monitor.orsyLadder {
                    heading(ladder)
                } else {
                    Text("Orsy").font(.callout.weight(.medium)).foregroundStyle(.secondary)
                }
                HStack(alignment: .top, spacing: 20) {
                    target(drill)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    hint(drill)
                }
                Divider()
                scoreboard(drill.stats)
                controls(drill)
            } else {
                Text("Connecting…").foregroundStyle(.secondary)
            }
            Spacer()
        }
    }

    // MARK: - The ladder

    private func heading(_ ladder: OrsyLadder) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 10) {
                Text(ladder.headline())
                    .font(.callout.weight(.medium))
                    .foregroundStyle(.secondary)
                ProgressView(
                    value: Double(ladder.unlockedCount), total: Double(max(1, ladder.items.count))
                )
                .frame(width: 90)
                if let lesson = ladder.lesson {
                    Text(lesson.title).font(.caption).foregroundStyle(.secondary)
                }
            }
            ForEach(ladder.focus, id: \.keys) { item in
                focusRow(item)
            }
        }
    }

    @ViewBuilder
    private func focusRow(_ item: OrsyLadderItem) -> some View {
        if let skill = monitor.orsySkill {
            let key = item.keys.min { skill.confidence($0) < skill.confidence($1) } ?? item.keys[0]
            let parts = skill.parts(key)
            let s = skill.skill(key)
            let options = skill.options
            HStack(spacing: 8) {
                Text(item.label)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(width: 60, alignment: .leading)
                Text(item.stage.rawValue)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: 44, alignment: .leading)
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary).frame(width: 54, height: 4)
                    Capsule().fill(Color.accentColor)
                        .frame(width: 54 * max(0, min(1, parts.confidence)), height: 4)
                }
                measure("\(s?.count ?? 0)/\(options.minSamples)", unit: "typed", met: parts.exposure >= 1)
                measure(
                    (s.map { $0.medianMs == .max ? "—" : "\($0.medianMs)" } ?? "—")
                        + "/\(options.targetMs)",
                    unit: "ms", met: parts.speed >= 1)
                measure(
                    s.map { String(format: "%.0f", $0.errorRate * 100) } ?? "0",
                    unit: "% back", met: parts.accuracy >= 1)
            }
        }
    }

    private func measure(_ value: String, unit: String, met: Bool) -> some View {
        HStack(spacing: 2) {
            Text(value).font(.caption.monospacedDigit())
            Text(unit).font(.caption2)
        }
        .foregroundStyle(met ? Color.secondary : Color.orange)
    }

    // MARK: - The target

    /// The target, coloured by what has been typed against it, with the table's division
    /// shown by underlining every other stroke.
    private func target(_ drill: OrsyDrillSession) -> some View {
        let chars = Array(drill.target.text)
        let typed = Array(drill.typed)
        // Which stroke of the division each character belongs to, for the underline.
        var unitOf = [Int](repeating: -1, count: chars.count)
        for (n, unit) in drill.target.units.enumerated() {
            for i in unit.offset..<min(chars.count, unit.offset + unit.text.count) {
                unitOf[i] = n
            }
        }
        return FlowText(
            chars: chars.enumerated().map { i, ch in
                let state: FlowText.State
                if i < typed.count {
                    state = typed[i] == ch ? .correct : .wrong
                } else if i == typed.count {
                    state = .cursor
                } else {
                    state = .pending
                }
                return FlowText.Char(
                    char: ch, state: state, gram: unitOf[i] >= 0 && unitOf[i] % 2 == 1,
                    sameHand: false)
            }
        )
    }

    /// The next stroke, drawn on both hands, for as long as its weakest pattern is still
    /// being learned.
    @ViewBuilder
    private func hint(_ drill: OrsyDrillSession) -> some View {
        if let layouts = monitor.layouts, let theory = monitor.orsyTheory,
            let unit = drill.wantedStroke
        {
            let t = theory.translate(left: unit.left, right: unit.right)
            let keys = t?.patterns.keys ?? []
            let confidence = keys.map { monitor.orsySkill?.confidence($0) ?? 0 }.min() ?? 0
            StrokeHint(
                diagram: ChordDiagram(layouts: layouts),
                left: unit.left, right: unit.right,
                text: unit.text.trimmingCharacters(in: .whitespaces),
                confidence: confidence, stumbled: drill.stumbled)
        } else {
            Color.clear.frame(width: StrokeHint.width, height: 1)
        }
    }

    // MARK: - The score

    private func scoreboard(_ s: OrsyDrillStats) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 22) {
            stat("wpm", String(format: "%.0f", s.wordsPerMinute), big: true)
            stat("accuracy", String(format: "%.0f%%", s.accuracy * 100))
            stat("wrong", "\(s.wrong)", bad: s.wrong > 0)
            stat("corrections", "\(s.corrections)", bad: s.corrections > 0)
            if s.dead > 0 { stat("not syllables", "\(s.dead)", bad: true) }
            Spacer()
            stat("strokes/min", String(format: "%.0f", s.strokesPerMinute))
            stat("keys/stroke", String(format: "%.1f", s.keysPerStroke))
            // Spread by stroke size: the question the layout's design left open, whether
            // a big stroke forms as fast as a small one, measured on every line.
            let sizes = s.spreadByKeys.keys.sorted()
            if !sizes.isEmpty {
                stat(
                    "spread by keys",
                    sizes.compactMap { k in
                        s.meanSpread(keys: k).map { String(format: "%d:%.0f", k, $0) }
                    }.joined(separator: " "))
            }
        }
    }

    private func stat(_ label: String, _ value: String, bad: Bool = false, big: Bool = false)
        -> some View
    {
        VStack(alignment: .leading, spacing: 1) {
            Text(value)
                .font(.system(big ? .largeTitle : .title3, design: .monospaced))
                .foregroundStyle(bad ? Color.orange : Color.primary)
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
    }

    // MARK: - Controls

    private func controls(_ drill: OrsyDrillSession) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let diagnosis = drill.diagnosis {
                Label(diagnosis, systemImage: "hand.point.up.left")
                    .font(.callout)
                    .foregroundStyle(.orange)
            } else if drill.wordOpen {
                Label(
                    "Word not closed: the next stroke will run on.  Undo and use the "
                        + "ending form (add Bk), or strike the space.",
                    systemImage: "space")
                    .font(.callout)
                    .foregroundStyle(.orange)
            }
            HStack(spacing: 14) {
                Label(
                    drill.finished
                        ? "Escape + \(enterKeys) for the next line"
                        : "Escape + \(enterKeys) to move on",
                    systemImage: "return"
                )
                .font(.callout)
                .foregroundStyle(drill.finished ? Color.accentColor : Color.secondary)
                if !drill.finished {
                    Text("Escape + both thumbs to start over")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if !monitor.focused {
                    Label("Paused — this window doesn't have the keyboard.",
                          systemImage: "pause.circle")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                if !drill.onTrack {
                    Label("Off the target — undo to fix it, or move on.",
                          systemImage: "exclamationmark.triangle")
                        .font(.callout)
                        .foregroundStyle(.orange)
                }
            }
            .help("The escape is the left i+Sp+Bk held while the right hand plays a Dosh chord.")
        }
    }

    /// The Dosh chord for Enter, named, so the control can be found.
    private var enterKeys: String {
        guard let layouts = monitor.layouts,
            let entry = layouts.variants["dosh"]?.chords.first(where: {
                $0.action.kind == "key" && $0.action.key == "ReturnEnter"
            })
        else { return "Enter" }
        return entry.keys.joined(separator: "+") + " (Enter)"
    }
}

/// The next stroke, drawn on both hands.
///
/// Each hand is its four finger columns in two rows, pinky to index from the outside in,
/// and the two thumbs in a row below, under the index side; the right hand is the mirror,
/// so the thumbs meet in the middle.  No column stagger: the board has one, but drawn it
/// misleads more than it helps.
///
/// The four Series have four colours, so a stroke reads as onset, second, vowel, coda
/// from left to right, which is the order the letters come out in.
struct StrokeHint: View {
    let diagram: ChordDiagram
    let left: UInt16
    let right: UInt16
    let text: String
    let confidence: Double
    let stumbled: Bool

    private var strength: Double {
        if stumbled { return 1 }
        return max(0, min(1, 1 - confidence))
    }

    private static let keySize: CGFloat = 12
    private static let gap: CGFloat = 3
    /// Two hands of four columns, a gap between them.
    static let width: CGFloat = 2 * (4 * keySize + 3 * gap) + 5 * gap

    private static let outerMask: UInt16 = 0x067

    /// The left hand, row by row, pinky on the left; nil is empty board.
    private static let leftHand: [[String?]] = [
        [nil, "s", "n", "i"],
        ["a", "o", "t", "e"],
        [nil, nil, "Sp", "Bk"],
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .top, spacing: Self.gap * 5) {
                hand(code: left, isLeft: true)
                hand(code: right, isLeft: false)
            }
            HStack(spacing: 5) {
                Text(text.isEmpty ? "space" : text)
                    .font(.system(size: 14, design: .monospaced))
                Text("\(diagram.spell(left))-\(diagram.spell(right))")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .lineLimit(1)
            .minimumScaleFactor(0.6)
        }
        .frame(width: Self.width, alignment: .leading)
        .opacity(0.15 + 0.85 * strength)
        .animation(.easeInOut(duration: 0.25), value: strength)
        .help("The next stroke, both hands, as the board lays them out.  It fades as its patterns are learned.")
    }

    private func hand(code: UInt16, isLeft: Bool) -> some View {
        VStack(spacing: Self.gap) {
            ForEach(Self.leftHand.indices, id: \.self) { row in
                let cells = isLeft ? Self.leftHand[row] : Self.leftHand[row].reversed()
                HStack(spacing: Self.gap) {
                    ForEach(cells.indices, id: \.self) { column in
                        key(code: code, isLeft: isLeft, name: cells[column])
                    }
                }
            }
        }
    }

    private func key(code: UInt16, isLeft: Bool, name: String?) -> some View {
        let key = name.flatMap { n in diagram.keys.first { $0.name == n } }
        let down = key.map { code & $0.mask != 0 } ?? false
        let outer = key.map { $0.mask & Self.outerMask != 0 } ?? false
        // Onset blue, second teal, vowel orange, coda purple.
        let colour: Color =
            isLeft ? (outer ? .blue : .teal) : (outer ? .purple : .orange)
        return RoundedRectangle(cornerRadius: 3)
            .fill(key == nil ? Color.clear : (down ? colour : Color.secondary.opacity(0.18)))
            .frame(width: Self.keySize, height: Self.keySize)
            .overlay {
                if down, let key, key.name == "Sp" || key.name == "Bk" {
                    Text(key.name)
                        .font(.system(size: 7, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
    }
}
