import SwiftUI
import TaipoKit

/// The drill screen: a target, typed against, scored as it goes.
struct DrillView: View {
    @ObservedObject var monitor: DeviceMonitor

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let drill = monitor.drill {
                target(drill)
                Divider()
                scoreboard(drill.stats)
                HStack(spacing: 12) {
                    if drill.finished {
                        Button("Next", action: monitor.nextDrill)
                    }
                    Button("Restart", action: monitor.restartDrill)
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
                return FlowText.Char(char: ch, state: state, gram: inGram(i, drill))
            }
        )
    }

    private func inGram(_ index: Int, _ drill: DrillSession) -> Bool {
        drill.target.units.contains {
            $0.text.count > 1 && index >= $0.offset && index < $0.offset + $0.text.count
        }
    }

    private func scoreboard(_ s: DrillStats) -> some View {
        HStack(spacing: 22) {
            stat("chords/min", String(format: "%.0f", s.chordsPerMinute))
            stat("accuracy", String(format: "%.0f%%", s.accuracy * 100))
            stat("wrong", "\(s.wrong)", bad: s.wrong > 0)
            stat("corrections", "\(s.corrections)", bad: s.corrections > 0)
            stat("spelled out", "\(s.spelled)", bad: s.spelled > 0)
            stat("same hand", "\(s.sameHand)", bad: s.sameHand > 0)
            if s.deadChords > 0 { stat("dead", "\(s.deadChords)", bad: true) }
        }
    }

    private func stat(_ label: String, _ value: String, bad: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(value)
                .font(.system(.title3, design: .monospaced))
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
            }
            Spacer()
        }
        .frame(height: 22)
        .help("Blue left, purple right.  Short bars waited out the chord window.")
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
