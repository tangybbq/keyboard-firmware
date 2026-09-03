import SwiftUI
import TaipoKit

/// The next chord, drawn as a right hand, fading out as it becomes known.
///
/// keybr's idea, and much more necessary here: there is nothing written on these keys, so
/// a writer who cannot remember a chord has nowhere to look it up but a cheat sheet in
/// another window.  It fades rather than switching off, so the last stage of learning a
/// chord is reading a hint you can barely see, and then not needing it.
///
/// The right hand only, because both hands type every chord -- the code says nothing about
/// which one was used -- and the developer thinks in that one.
struct ChordHint: View {
    let diagram: ChordDiagram
    /// The chord to show.
    let code: UInt16
    /// What it types, drawn beside the hand.
    let types: String
    /// How well it is known, `0...1`.  The hint is the inverse of this.
    let confidence: Double
    /// Whether the last chord was a mistake, which brings the hint back whatever the
    /// confidence says.
    let stumbled: Bool

    /// Never quite invisible while a chord is unlearned, and never in the way once it is.
    ///
    /// The floor matters: a hint that reached zero would leave the writer with nothing at
    /// exactly the moment the model was wrong about them.
    private var strength: Double {
        if stumbled { return 1 }
        return max(0, min(1, 1 - confidence))
    }

    private let keySize: CGFloat = 13
    private let gap: CGFloat = 3

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            hand
            VStack(alignment: .leading, spacing: 1) {
                Text(types == " " ? "space" : types)
                    .font(.system(size: 15, design: .monospaced))
                Text(diagram.spell(code))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .opacity(0.15 + 0.85 * strength)
        .animation(.easeInOut(duration: 0.25), value: strength)
        .help("The next chord, on the right hand.  It fades as the chord is learned.")
    }

    /// Two rows of five: the four fingers, then the thumb a little apart.
    private var hand: some View {
        VStack(spacing: gap) {
            row(top: true)
            row(top: false)
        }
    }

    private func row(top: Bool) -> some View {
        HStack(spacing: gap) {
            ForEach(ChordDiagram.Column.allCases, id: \.rawValue) { column in
                // The thumb sits in its own column, set off from the fingers.
                if column == .thumb { Spacer().frame(width: gap * 2) }
                key(column: column, top: top)
            }
        }
    }

    private func key(column: ChordDiagram.Column, top: Bool) -> some View {
        let key = diagram.keys.first { $0.column == column && $0.top == top }
        let down = key.map { code & $0.mask != 0 } ?? false
        return RoundedRectangle(cornerRadius: 3)
            .fill(down ? Color.accentColor : Color.secondary.opacity(0.18))
            .frame(width: keySize, height: keySize)
            .overlay {
                if down, let key, key.column == .thumb {
                    // The thumbs are the two that change what a chord means rather than
                    // which chord it is, so they are worth naming even in a picture.
                    Text(key.name)
                        .font(.system(size: 7, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
    }
}
