import SwiftUI
import TaipoKit

/// The drill screen: a target, typed against, scored as it goes.
struct DrillView: View {
    @ObservedObject var monitor: DeviceMonitor

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            modes
            if let drill = monitor.drill {
                // What this line is for.  A drill built from the writer's own mistakes is
                // worth naming: `"o" / "s" — ring, wrong-row` says why these words, and
                // without it the material looks arbitrary.
                if let title = monitor.drillTitle {
                    HStack(spacing: 10) {
                        Text(title)
                            .font(.callout.weight(.medium))
                            .foregroundStyle(.secondary)
                        if let ladder = monitor.ladder { progress(ladder) }
                    }
                }
                target(drill)
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
        .onAppear { monitor.beginPractice() }
        .onDisappear { monitor.endPractice() }
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

    var body: some View {
        Text(attributed)
            .font(.system(size: 26, design: .monospaced))
            .lineSpacing(6)
            .textSelection(.disabled)
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
