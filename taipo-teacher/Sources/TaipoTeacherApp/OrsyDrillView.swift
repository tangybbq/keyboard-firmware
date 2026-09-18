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
                Spacer()
                newSwitch
                hintSwitch
            }
            ForEach(ladder.focus, id: \.key) { item in
                focusRow(item)
            }
        }
    }

    /// One item being worked on.
    ///
    /// A shape is two readings, onset and coda, measured apart, and the row reports the
    /// weaker one -- which is the one worth practising, but has to be named: a coda `s`
    /// at 50% back sits there unmoved by any number of onset `s`, and read as "s" that
    /// looks like a display that has stopped updating.
    /// How many uses of a stroke count as new, and so keep the picture up in `learn`.
    ///
    /// Two: the picture is up the first time a stroke comes round and the second, and
    /// gone by the third.
    ///
    /// Eight was the number while this counted *patterns*, where it was right -- a pattern
    /// recurs across every stroke that uses it, so eight comes quickly and forty is the
    /// gate it is measured against.  A stroke is not like that.  There are hundreds of
    /// them and they are composed by rule rather than memorised one at a time, so the
    /// picture's job is to get the learner over the first meeting with a combination, not
    /// to drill each combination to automaticity.  Measured over 670 strokes and 6,168
    /// uses of them, the median stroke had been made **twice** and only a quarter had ever
    /// reached eight, so eight left the picture up for 58% of a block -- which is not a
    /// hint, it is a wall chart.  Two draws it for 16%, about one stroke in six.
    ///
    /// Erring low is cheap because a stumble puts the picture straight back up.
    static let hintUses = 2

    /// Whether the ladder may put the next item out, or the block is review only.
    ///
    /// The ladder unlocks on reach, not on choice, so the only say the writer has in when
    /// the next thing arrives is this: turned off, the block is spent on what has already
    /// been reached, for the days when the last few items have not settled or nothing new
    /// is wanted.  Nothing is lost by it -- where the ladder stands is read from the logs
    /// -- so the item waiting comes out unchanged whenever it is turned back on.
    private var newSwitch: some View {
        Toggle("new", isOn: $monitor.orsyIntroduce)
            .toggleStyle(.checkbox)
            .font(.caption)
            .help(
                "Whether the ladder may introduce its next item.  Off, the block reviews "
                + "what you have already reached; the item waiting is unaffected and "
                + "comes out when you turn this back on.")
    }

    /// Draw the next stroke always, while it is new, or never; and name its keys or not.
    /// Plain buttons rather than a segmented picker, for the reason `MainView.tabs` gives.
    private var hintSwitch: some View {
        HStack(spacing: 8) {
            Text("hint").font(.caption).foregroundStyle(.secondary)
            ForEach(DeviceMonitor.OrsyHint.allCases) { choice in
                Button { monitor.orsyHint = choice } label: {
                    Text(choice.rawValue)
                        .font(.caption.weight(monitor.orsyHint == choice ? .semibold : .regular))
                        .foregroundStyle(
                            monitor.orsyHint == choice ? Color.accentColor : Color.secondary)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            Toggle("keys", isOn: $monitor.orsyHintKeys)
                .toggleStyle(.checkbox)
                .font(.caption)
        }
        .help(
            "Show keeps the stroke drawn.  Learn draws it while it is new and after a "
            + "stumble, then takes it away.  Hide never draws it.  Keys names the stroke "
            + "in writing either way, which is the half you can say to yourself.")
    }

    @ViewBuilder
    private func focusRow(_ item: OrsyLadderItem) -> some View {
        if let skill = monitor.orsySkill {
            let parts = skill.parts(item.key)
            let s = skill.skill(item.key)
            let options = skill.options
            HStack(spacing: 8) {
                Text(item.label)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(width: 46, alignment: .leading)
                Text(item.stage.rawValue)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .frame(width: 44, alignment: .leading)
                // What the pattern is made of, and what Dosh does there.  The rule is
                // sayable, which the picture is not.
                Text(mnemonic(item) ?? "")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(width: 190, alignment: .leading)
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

    /// The sayable rule for an item, if there is one.
    private func mnemonic(_ item: OrsyLadderItem) -> String? {
        guard let tables = monitor.orsyTheory?.tables, let layouts = monitor.layouts
        else { return nil }
        return OrsyMnemonic.of(item.key, tables: tables, layouts: layouts)
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
                // The division stays whatever the hint is doing.  Phoenix underlined its
                // phrases throughout and it was worth having; where a word breaks is a
                // separate skill from producing the stroke, and hiding the underline would
                // only make every wrong division look like a wrong stroke, which the
                // diagnosis cannot yet tell apart.
                return FlowText.Char(
                    char: ch, state: state,
                    gram: unitOf[i] >= 0 && unitOf[i] % 2 == 1, sameHand: false)
            }
        )
    }

    /// The next stroke, drawn on both hands, for as long as the stroke itself is still
    /// being learned.
    @ViewBuilder
    private func hint(_ drill: OrsyDrillSession) -> some View {
        if let layouts = monitor.layouts, let unit = drill.wantedStroke {
            StrokeHint(
                diagram: ChordDiagram(layouts: layouts),
                left: unit.left, right: unit.right,
                text: unit.text.trimmingCharacters(in: .whitespaces),
                showHands: drawHands(unit, stumbled: drill.stumbled),
                showKeys: monitor.orsyHintKeys)
        } else {
            Color.clear.frame(width: StrokeHint.width, height: 1)
        }
    }

    /// Whether the picture is drawn for this stroke.
    ///
    /// The question is whether *this stroke* has been made before, not whether its
    /// patterns are familiar.  It used to ask the second, taking the least-typed of the
    /// stroke's four Series, and that answer goes wrong the moment the lessons stop
    /// introducing patterns: `ion` is Series 2 `i`, the closing `o` and a coda `n`, all
    /// three of them long since past the count, so a stroke nobody had ever made arrived
    /// with no picture.  Every pattern in a stroke is used at least as often as the
    /// stroke itself, so counting the stroke draws the picture everywhere the old test
    /// did and in the cases it missed.
    private func drawHands(_ unit: OrsyDrillUnit, stumbled: Bool) -> Bool {
        switch monitor.orsyHint {
        case .always: return true
        case .never: return false
        case .learn:
            guard !stumbled else { return true }
            let uses = monitor.orsySkill?.strokeUses(left: unit.left, right: unit.right) ?? 0
            return uses < Self.hintUses
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
            } else if drill.wordClosedEarly {
                Label(
                    "Word closed early: that stroke used the ending form, so the next "
                        + "one will get a space.  Undo and use the plain vowel.",
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
    /// Whether the keys are drawn.
    let showHands: Bool
    /// Whether the stroke is named in writing.
    let showKeys: Bool

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
            if showHands {
                HStack(alignment: .top, spacing: Self.gap * 5) {
                    hand(code: left, isLeft: true)
                    hand(code: right, isLeft: false)
                }
            }
            if showKeys {
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
        }
        .frame(width: Self.width, alignment: .leading)
        .help("The next stroke, both hands.")
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
