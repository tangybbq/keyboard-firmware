import Foundation

/// Reading back the key logs the collector writes.
///
/// The app is the only thing that writes these files and, once it wants to know what its
/// writer has been getting wrong, the only thing that reads them.  That is the plan's
/// answer to where the model lives: the logs are append-only and replaying them is cheap,
/// so there is no store to keep in step with them.  Three days of typing replays in well
/// under a second.
///
/// The format and the session rules are `bbq_keyboard::replay`'s, and the parsing here has
/// to agree with `sessions_from_text`: a file holds several timelines, not one.

/// One raw key event, as the device recorded it.
public struct KeyLogEvent: Equatable {
    public let timeMs: UInt32
    public let key: Int
    public let press: Bool
}

/// A marker line: the mode, the chord table, or the row position changing.
public struct KeyLogMarker: Equatable {
    public let timeMs: UInt32
    public let name: String
    public let value: UInt8
}

/// One stretch of log with a single, continuous timeline.
public struct KeyLogSession {
    /// The `layout=` fingerprint from the `# session` header, when it had one.
    public let layout: UInt64?
    /// Key events and markers together, in the order they were logged.
    public let entries: [Entry]

    public enum Entry: Equatable {
        case key(KeyLogEvent)
        case marker(KeyLogMarker)
    }

    /// Whether this session was recorded against the tables in hand.
    ///
    /// The whole reason the fingerprint is in the header.  A session recorded against
    /// other tables replays into chords that mean something else -- when Dosh's letters
    /// moved, the chord that used to type `a` began typing `s` -- and nothing about the
    /// replay looks wrong while it does it.  That is the "quietly wrong" the fingerprint
    /// exists to catch, so anything deriving from a log has to ask.
    ///
    /// A session with no fingerprint at all is not recorded against these tables either:
    /// it came from a firmware too old to say, and that is not the same as agreeing.
    ///
    /// Deriving from a log asks [`LayoutHistory`] rather than this, because a session from
    /// an older revision is usually still worth most of what is in it -- everything but
    /// the chords that actually changed.  This is the exact question, for the places that
    /// want it.
    public func recorded(with layouts: Layouts) -> Bool {
        layout != nil && layout == layouts.fingerprintValue
    }

    public var keys: [KeyLogEvent] {
        entries.compactMap { if case .key(let e) = $0 { return e } else { return nil } }
    }
}

public enum KeyLogFile {
    /// Split a text log into its sessions.
    ///
    /// A session ends at any of the three comment lines the collector writes, and also
    /// wherever the times step backwards -- a restart nothing announced, which the day
    /// rollover used to produce.  The offsets either side of such a break count from
    /// different zeros, so replaying across one would invent intervals that never elapsed.
    public static func sessions(from text: String, layouts: Layouts) -> [KeyLogSession] {
        var out = [KeyLogSession]()
        var entries = [KeyLogSession.Entry]()
        var layout: UInt64?
        var lastTime: UInt32?

        func flush(next: UInt64?) {
            if !entries.isEmpty { out.append(KeyLogSession(layout: layout, entries: entries)) }
            entries = []
            layout = next
            lastTime = nil
        }

        for raw in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty { continue }
            if line.hasPrefix("#") {
                if breaksTimeline(line) { flush(next: layoutFingerprint(line)) }
                continue
            }
            // `1234 + L.a` for a key, `1381 = mode 0` for a marker: three fields or four.
            let fields = line.split(separator: " ").map(String.init)
            guard fields.count >= 3, let time = UInt32(fields[0]) else { continue }

            // A step backwards is a new timeline, header or no header.  The tables carry
            // across it: it is a rollover inside one session, not a new one.
            if let last = lastTime, time < last { flush(next: layout) }
            lastTime = time

            switch fields[1] {
            case "=":
                guard fields.count == 4, let value = UInt8(fields[3]) else { continue }
                entries.append(
                    .marker(KeyLogMarker(timeMs: time, name: fields[2], value: value)))
            case "+", "-":
                guard fields.count == 3, let key = keyCode(fields[2], layouts) else { continue }
                entries.append(
                    .key(KeyLogEvent(timeMs: time, key: key, press: fields[1] == "+")))
            default:
                continue
            }
        }
        flush(next: nil)
        return out
    }

    /// The three comment lines that mean the offsets after them count from a new zero.
    private static func breaksTimeline(_ line: String) -> Bool {
        line.hasPrefix("# session") || line.hasPrefix("# scrubbed")
            || line.hasPrefix("# device reset")
    }

    /// `layout=0x...` from a `# session` header.  `unknown` reads as absent.
    private static func layoutFingerprint(_ line: String) -> UInt64? {
        guard line.hasPrefix("# session") else { return nil }
        guard
            let field = line.split(separator: " ").first(where: { $0.hasPrefix("layout=") })
        else { return nil }
        let value = field.dropFirst("layout=".count)
        return UInt64(value.hasPrefix("0x") ? value.dropFirst(2) : value, radix: 16)
    }

    /// `L.t` and `R.Sp` back to a key code, and `k12` for a key the layout ignores.
    ///
    /// The same names `bbq_keyboard::replay::key_for_name` produces, searched the same way:
    /// the log's key codes are in the upper row position, whatever the board.
    public static func keyCode(_ name: String, _ layouts: Layouts) -> Int? {
        if name.hasPrefix("k") { return Int(name.dropFirst()) }
        let parts = name.split(separator: ".")
        guard parts.count == 2 else { return nil }
        let side = parts[0] == "L" ? "left" : "right"
        return layouts.scanMap.upper.first { $0.side == side && $0.name == String(parts[1]) }?
            .key
    }
}
