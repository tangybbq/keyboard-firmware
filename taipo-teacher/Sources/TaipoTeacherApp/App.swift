import SwiftUI
import TaipoKit

/// Taipo Teacher.
///
/// The first screen is deliberately the one that proves the whole stack: connect to the
/// keyboard, stream its key log, and show the chords as they are typed -- with the hand,
/// the keys, the spread, and how the chord ended.  None of that is visible to a host that
/// only sees HID, which is the reason the project exists.
@main
struct TaipoTeacherApp: App {
    @StateObject private var monitor = DeviceMonitor()

    var body: some Scene {
        WindowGroup("Taipo Teacher") {
            MainView(monitor: monitor)
                .frame(minWidth: 680, minHeight: 460)
                .onAppear { monitor.start() }
                .onDisappear { monitor.stop() }
        }
        .windowResizability(.contentSize)
    }
}

/// The window: a mode picker, the status line, and whichever screen is showing.
struct MainView: View {
    @ObservedObject var monitor: DeviceMonitor
    @State private var mode: Mode = .practice

    enum Mode: String, CaseIterable, Identifiable {
        case practice = "Practice"
        case live = "Live"
        var id: String { rawValue }
    }

    var body: some View {
        VStack(spacing: 0) {
            Picker("", selection: $mode) {
                ForEach(Mode.allCases) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, 12)
            .padding(.top, 10)

            switch mode {
            case .practice: DrillView(monitor: monitor)
            case .live: LiveView(monitor: monitor)
            }
        }
        // The keyboard types into whichever window has focus, which is this one.  Nothing
        // here wants the characters -- the chords come from the log -- so they are
        // swallowed rather than left to beep.
        .background(KeySwallower().frame(width: 0, height: 0))
    }
}

struct LiveView: View {
    @ObservedObject var monitor: DeviceMonitor

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider()
            if monitor.chords.isEmpty {
                VStack {
                    Spacer()
                    Text(monitor.recording ? "Type something." : "Waiting for the keyboard…")
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            } else {
                chordList
            }
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(statusColor)
                .frame(width: 10, height: 10)
            VStack(alignment: .leading, spacing: 2) {
                Text(monitor.status.summary).font(.headline)
                Text(monitor.recording
                     ? "Recording — everything typed on the keyboard is being read."
                     : "Not recording.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(12)
    }

    private var statusColor: Color {
        switch monitor.status {
        case .connected(_, let ok): return ok ? .green : .orange
        case .connecting: return .yellow
        case .failed: return .red
        case .disconnected: return .secondary
        }
    }

    private var chordList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(monitor.chords) { live in
                        ChordRow(live: live)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
            }
            .onChange(of: monitor.chords.count) { _, _ in
                if let last = monitor.chords.last {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
        }
    }
}

struct ChordRow: View {
    let live: DeviceMonitor.LiveChord

    var body: some View {
        HStack(spacing: 10) {
            // The hand, which the host cannot see at all.
            Text(live.chord.side.letter)
                .font(.system(.body, design: .monospaced).bold())
                .foregroundStyle(live.chord.side == .left ? Color.blue : Color.purple)
                .frame(width: 16)

            Text(live.types.map { $0 == " " ? "␣" : $0 } ?? "—")
                .font(.system(size: 20, design: .monospaced))
                .frame(width: 44, alignment: .leading)
                .foregroundStyle(live.dead ? Color.red : Color.primary)

            Text(String(format: "0x%03x", live.chord.code))
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)

            // How the chord ended, and how much it was assembled rather than struck.
            // Both are technique, and neither reaches the host.
            Text(live.chord.end.rawValue)
                .font(.caption)
                .foregroundStyle(live.chord.end == .timerExpired ? .orange : .secondary)
                .frame(width: 72, alignment: .leading)

            Text("spread \(live.chord.spreadMs)ms")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.secondary)

            Spacer()
            if live.dead {
                Text("dead chord").font(.caption).foregroundStyle(.red)
            }
        }
        .id(live.id)
        .padding(.vertical, 1)
    }
}
