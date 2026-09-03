import SwiftUI
import TaipoKit

/// The drill screen: a target, typed against, scored as it goes.
struct DrillView: View {
    @ObservedObject var monitor: DeviceMonitor

    /// Whether this window has the keyboard: `.key` when it does, `.active` when the app
    /// is frontmost but another window has it, `.inactive` when the app is not frontmost.
    @Environment(\.controlActiveState) private var activeState

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            modes
            if let drill = monitor.drill {
                // What this line is for.  A drill built from the writer's own mistakes is
                // worth naming: `"o" / "s" — ring, wrong-row` says why these words, and
                // without it the material looks arbitrary.
                if let ladder = monitor.ladder {
                    ladderHeading(ladder)
                } else if let title = monitor.drillTitle {
                    Text(title)
                        .font(.callout.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                HStack(alignment: .top, spacing: 20) {
                    target(drill)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    hint(drill)
                }
                Divider()
                scoreboard(drill.stats)
                HStack(spacing: 14) {
                    if drill.finished {
                        Label("Enter for the next line", systemImage: "return")
                            .font(.callout)
                            .foregroundStyle(Color.accentColor)
                    } else {
                        Text("Both thumbs to start over")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if !monitor.focused {
                        Label("Paused — this window doesn't have the keyboard.",
                              systemImage: "pause.circle")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    if monitor.skippedSessions > 0 {
                        Label(
                            "Ignoring \(monitor.skippedSessions) session"
                                + (monitor.skippedSessions == 1 ? "" : "s")
                                + " typed on older chord tables.",
                            systemImage: "clock.arrow.circlepath"
                        )
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .help(
                            "Those logs replay into different letters on these tables, so "
                            + "nothing is derived from them.  New typing rebuilds it.")
                    }
                    if !drill.onTrack {
                        Label(
                            "Off the target — backspace to fix it.",
                            systemImage: "exclamationmark.triangle"
                        )
                        .font(.callout)
                        .foregroundStyle(.orange)
                    }
                }
            } else {
                Text("Connecting…").foregroundStyle(.secondary)
            }
            Spacer()
            technique
        }
        .padding(16)
        // Scoring belongs to the screen, not to the app.  The collector runs whether or not
        // this is showing; the drill only consumes chords while it is.
        .onAppear {
            monitor.focused = activeState == .key
            monitor.beginPractice()
        }
        .onDisappear { monitor.endPractice() }
        .onChange(of: activeState) { _, state in monitor.focused = state == .key }
    }

    /// The two kinds of practice.
    ///
    /// Plain buttons rather than a segmented `Picker`, for the reason `MainView.tabs`
    /// spells out: the AppKit control behind `.pickerStyle(.segmented)` leaks its tag
    /// machinery on every measurement pass.
    private var modes: some View {
        HStack(spacing: 8) {
            ForEach(DeviceMonitor.PracticeMode.allCases) { mode in
                Button { monitor.practiceMode = mode } label: {
                    Text(mode.rawValue)
                        .font(.callout.weight(monitor.practiceMode == mode ? .semibold : .regular))
                        .foregroundStyle(
                            monitor.practiceMode == mode ? Color.accentColor : Color.secondary)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            Spacer()
        }
        .help(
            "Ladder unlocks letters, then numbers and marks, as each is learned.  "
            + "Confusions drills the pairs your own corrections say you mix up.")
    }

    /// Where the ladder is, and where each item it is working on has got to.
    ///
    /// The three measurements are shown apart rather than as one number, because one
    /// number does not say what to do about it: a chord halfway there might be one you
    /// have hardly typed, one you type slowly, or one you keep taking back, and those are
    /// three different afternoons.  Each is drawn against the threshold it has to clear,
    /// and the one still short of it is the one coloured.
    private func ladderHeading(_ ladder: Ladder) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 10) {
                Text(ladder.headline())
                    .font(.callout.weight(.medium))
                    .foregroundStyle(.secondary)
                progress(ladder)
            }
            ForEach(ladder.focus, id: \.label) { item in
                focusRow(item)
            }
        }
    }

    /// One item being worked on: what it is, how far along, and what is holding it back.
    @ViewBuilder
    private func focusRow(_ item: LadderItem) -> some View {
        if let skill = monitor.skill {
            // A pair is only as learned as its weaker half, which is the one worth
            // reporting -- the same rule the ladder ranks by.
            let code = item.codes.min { skill.confidence($0) < skill.confidence($1) }
                ?? item.codes[0]
            let parts = skill.parts(code)
            let s = skill.skill(code)
            let options = skill.options

            HStack(spacing: 8) {
                Text(item.label == " " ? "space" : item.label)
                    .font(.system(size: 12, design: .monospaced))
                    .frame(width: 34, alignment: .leading)

                bar(parts.confidence)

                measure(
                    "\(s?.count ?? 0)/\(options.minSamples)", unit: "typed",
                    met: parts.exposure >= 1)
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

    /// A measurement and the threshold it is being held to, coloured only when it is what
    /// is still missing.
    private func measure(_ value: String, unit: String, met: Bool) -> some View {
        HStack(spacing: 2) {
            Text(value).font(.caption.monospacedDigit())
            Text(unit).font(.caption2)
        }
        .foregroundStyle(met ? Color.secondary : Color.orange)
    }

    private func bar(_ fraction: Double) -> some View {
        ZStack(alignment: .leading) {
            Capsule().fill(.quaternary).frame(width: 54, height: 4)
            Capsule()
                .fill(Color.accentColor)
                .frame(width: 54 * max(0, min(1, fraction)), height: 4)
        }
    }

    /// How far up the ladder this is, as a bar and a count.
    ///
    /// Worth drawing because the ladder's whole promise is that it moves: a writer who
    /// cannot see the next item coming has no reason to believe the drill is going
    /// anywhere.
    private func progress(_ ladder: Ladder) -> some View {
        HStack(spacing: 6) {
            ProgressView(
                value: Double(ladder.unlockedCount), total: Double(max(1, ladder.items.count))
            )
            .frame(width: 90)
            Text("\(ladder.unlockedCount)/\(ladder.items.count)")
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }

    /// The next chord, drawn, for as long as it is still being learned.
    ///
    /// Driven by the skill model rather than by the drill, so what fades is a chord the
    /// logs say is known -- not one that happens to have gone right twice in this line.
    /// The slot is always there, whether or not there is a chord to put in it.
    ///
    /// An empty hint is drawn invisible rather than left out: the target text is laid out
    /// beside it, so a hint that came and went would rewrap the line under the writer's
    /// hands at the moment they were reading it.
    @ViewBuilder
    private func hint(_ drill: DrillSession) -> some View {
        if let layouts = monitor.layouts, let unit = drill.wantedChord {
            ChordHint(
                diagram: ChordDiagram(layouts: layouts),
                code: unit.code,
                types: unit.text,
                confidence: monitor.skill?.confidence(unit.code) ?? 0,
                stumbled: drill.stumbled)
        } else {
            Color.clear.frame(width: ChordHint.width, height: 1)
        }
    }

    /// The target, coloured by what has been typed against it.
    ///
    /// Units the segmentation wants chorded are underlined, so it is visible *before*
    /// typing them that they are one chord rather than three -- which is the thing the
    /// MonkeyType drills could only hint at in prose.
    private func target(_ drill: DrillSession) -> some View {
        let chars = Array(drill.target.text)
        let typed = Array(drill.typed)
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
                    char: ch, state: state, gram: inGram(i, drill),
                    sameHand: drill.sameHandOffsets.contains(i))
            }
        )
    }

    private func inGram(_ index: Int, _ drill: DrillSession) -> Bool {
        drill.target.units.contains {
            $0.text.count > 1 && index >= $0.offset && index < $0.offset + $0.text.count
        }
    }

    private func scoreboard(_ s: DrillStats) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 22) {
            stat("wpm", String(format: "%.0f", s.wordsPerMinute), big: true)
            stat("accuracy", String(format: "%.0f%%", s.accuracy * 100))
            stat("wrong", "\(s.wrong)", bad: s.wrong > 0)
            stat("corrections", "\(s.corrections)", bad: s.corrections > 0)
            stat("spelled out", "\(s.spelled)", bad: s.spelled > 0)
            stat(
                "same hand",
                s.eligiblePairs > 0
                    ? String(format: "%d (%.0f%%)", s.sameHand, s.sameHandRate * 100)
                    : "\(s.sameHand)",
                bad: s.sameHand > 0)
            if s.sameHandCorrections > 0 {
                stat("…on backspace", "\(s.sameHandCorrections)", bad: true)
            }
            if s.deadChords > 0 { stat("dead", "\(s.deadChords)", bad: true) }
            Spacer()
            stat("chords/min", String(format: "%.0f", s.chordsPerMinute))
        }
    }

    private func stat(
        _ label: String, _ value: String, bad: Bool = false, big: Bool = false
    ) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(value)
                .font(.system(big ? .largeTitle : .title3, design: .monospaced))
                .foregroundStyle(bad ? Color.orange : Color.primary)
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
    }

    /// A bead per chord: which hand, and whether it stayed on the same one.
    ///
    /// The thing a software-only trainer cannot draw, and the reason for all of this.
    private var technique: some View {
        HStack(spacing: 3) {
            ForEach(monitor.chords.suffix(60)) { live in
                RoundedRectangle(cornerRadius: 2)
                    .fill(live.chord.side == .left ? Color.blue : Color.purple)
                    .frame(width: 8, height: live.chord.end == .timerExpired ? 14 : 20)
                    .opacity(live.dead ? 0.3 : 1)
                    // Every chord appears here, so this is where a fault on a chord that
                    // types nothing -- a backspace, a modifier -- can be seen at all.
                    .overlay(alignment: .top) {
                        if live.sameHand {
                            Circle().fill(Color.orange).frame(width: 5, height: 5)
                                .offset(y: -7)
                        }
                    }
            }
            Spacer()
        }
        .frame(height: 22)
        .help(
            "Blue left, purple right.  Short bars waited out the chord window.  "
            + "An orange dot means that chord stayed on the previous hand.")
    }
}

/// The target text, wrapped, with per-character state.
struct FlowText: View {
    enum State { case pending, correct, wrong, cursor }
    struct Char: Identifiable {
        let id = UUID()
        let char: Character
        let state: State
        /// Part of a gram the table would type in one chord.
        let gram: Bool
        /// Typed on the same hand as the chord before it, close enough to have been
        /// avoidable.  Marked where it happened, because a counter in the corner is not
        /// something anyone reacts to mid-line.
        let sameHand: Bool
    }

    let chars: [Char]

    static let fontSize: CGFloat = 26
    static let lineSpacing: CGFloat = 6
    /// One wrapped line of the target, near enough for reserving room.
    static let lineHeight: CGFloat = fontSize * 1.2 + lineSpacing
    /// How many wrapped lines the target always has room for.
    ///
    /// Reserved rather than left to the content, so that a short line and a long one do
    /// not move the scoreboard and the technique strip up and down the window between
    /// them.  Three, which is what an ordinary line comes to; longer ones grow past it.
    static let reservedLines = 3

    var body: some View {
        Text(attributed)
            .font(.system(size: Self.fontSize, design: .monospaced))
            .lineSpacing(Self.lineSpacing)
            .textSelection(.disabled)
            // Take the height the text actually needs.  Without this the VStack compresses
            // it when the window is tight and the line ends in an ellipsis -- which is not
            // a thing a drill can do, since the writer has to type what it is hiding.
            .fixedSize(horizontal: false, vertical: true)
            .frame(
                minHeight: Self.lineHeight * CGFloat(Self.reservedLines),
                alignment: .topLeading)
    }

    private var attributed: AttributedString {
        var out = AttributedString()
        for c in chars {
            var piece = AttributedString(String(c.char))
            switch c.state {
            case .pending: piece.foregroundColor = .secondary
            case .correct: piece.foregroundColor = .primary
            case .wrong:
                piece.foregroundColor = .white
                piece.backgroundColor = .red
            case .cursor:
                piece.foregroundColor = .primary
                piece.backgroundColor = Color.accentColor.opacity(0.35)
            }
            if c.gram {
                piece.underlineStyle = .single
            }
            if c.sameHand {
                piece.backgroundColor = .orange.opacity(0.35)
            }
            out += piece
        }
        return out
    }
}

/// Swallows key events so the keyboard's own output does not make the app beep.
///
/// The characters are typed into whatever has focus, which is this window; nothing here
/// wants them, because the chords come from the keyboard's log instead.
struct KeySwallower: NSViewRepresentable {
    final class View: NSView {
        override var acceptsFirstResponder: Bool { true }
        override func keyDown(with event: NSEvent) {}
        override func viewDidMoveToWindow() {
            window?.makeFirstResponder(self)
        }
    }
    func makeNSView(context: Context) -> View { View() }
    func updateNSView(_ nsView: View, context: Context) {}
}
