import Foundation

/// Where each chord bit sits under a hand, so a chord can be drawn.
///
/// There is nothing written on these keys and no way to look one up, which is the whole
/// argument for showing a picture: a writer who cannot remember a chord has nowhere to
/// find it but a cheat sheet in another window.
///
/// The positions come from `layouts.json`'s `bits`, which carry the finger and the row for
/// every one of the ten -- so this cannot describe a keyboard the firmware does not have,
/// and a bit moving is a change to the exported tables rather than to a drawing.
///
/// **The right hand.**  Both hands type every chord, and the code says nothing about which
/// one was used -- that is `Chord.side`, kept separately.  So one hand is enough to show a
/// chord with, and it is drawn as the right because that is the one the developer thinks
/// in.  The consequence is the column order: on the right hand the index finger is
/// innermost, so the columns read index, middle, ring, pinky from the middle of the board
/// outwards, which is the mirror of the left.
public struct ChordDiagram {
    /// A column of the hand, in the order they are drawn left to right.
    public enum Column: Int, CaseIterable, Sendable {
        case index, middle, ring, pinky, thumb
    }

    /// One key of one hand.
    public struct Key: Equatable, Sendable {
        public let mask: UInt16
        /// What the key is called, which is also what it types alone in both tables --
        /// except the upper pinky, which Dosh does not use.
        public let name: String
        public let column: Column
        /// The far row of the two, which is the top one as the hand sits on the board.
        public let top: Bool
    }

    /// The ten keys, in drawing order: the far row left to right, then the near row.
    public let keys: [Key]

    public init(layouts: Layouts) {
        var out = [Key]()
        for bit in layouts.bits {
            let column: Column
            switch bit.finger {
            case "index": column = .index
            case "middle": column = .middle
            case "ring": column = .ring
            case "pinky": column = .pinky
            default: column = .thumb
            }
            // The thumbs share the finger columns' two rows, in a fifth column of their
            // own -- `COL_5` in the mesa2's matrix.  On the right hand the backspace thumb
            // is the far one and the space thumb the near one, which is the mirror of the
            // left hand's arrangement.
            let top = column == .thumb ? bit.name == "Bk" : bit.row == "top"
            out.append(Key(mask: bit.mask, name: bit.name, column: column, top: top))
        }
        keys = out.sorted {
            ($0.top ? 0 : 1, $0.column.rawValue) < ($1.top ? 0 : 1, $1.column.rawValue)
        }
    }

    /// The keys a chord asks for.
    public func pressed(_ code: UInt16) -> [Key] {
        keys.filter { code & $0.mask != 0 }
    }

    /// A chord written out as the keys it uses, for a label or a tooltip.
    ///
    /// In bit order rather than drawing order, so it reads the way the log spells a chord
    /// and the way DOSH.md and TAIPO.md name one: `a+e+Bk`, not `Bk+e+a`.
    public func spell(_ code: UInt16) -> String {
        pressed(code).sorted { $0.mask < $1.mask }.map(\.name).joined(separator: "+")
    }
}
