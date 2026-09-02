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
    @Environment(\.openWindow) private var openWindow

    var body: some Scene {
        // The menu bar is the app's real home.  Collecting has to keep going whether or
        // not a window is open, and the keyboard's vendor interface can only be claimed by
        // one process, so this app has to be the one that holds it -- an always-on CLI
        // collector would lock the trainer out of its own keyboard.
        MenuBarExtra {
            MenuContents(monitor: monitor)
        } label: {
            Image(systemName: monitor.menuBarSymbol)
        }

        // No `.windowResizability(.contentSize)`.  It makes the window track the content's
        // size extrema, which has `NSHostingView` re-measuring the whole view tree on every
        // display cycle; with a min size on the content and nothing wanting to be sized to
        // fit, it bought nothing and cost the main thread dearly.
        Window("Taipo Teacher", id: "main") {
            MainView(monitor: monitor)
                .frame(minWidth: 680, minHeight: 460)
        }
    }
}

struct MenuContents: View {
    @ObservedObject var monitor: DeviceMonitor
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Text(monitor.status.summary)
        Text(monitor.activity)

        Divider()
        Button(monitor.paused ? "Resume recording" : "Pause recording") {
            monitor.paused.toggle()
        }
        Button("Practice…") {
            openWindow(id: "main")
            NSApp.activate(ignoringOtherApps: true)
        }
        // Flat, not a submenu.  A `Menu` inside `MenuBarExtra` opens and closes again
        // before it can be clicked, which makes the one item here that has to work when it
        // is wanted the one item that does not.
        Button("Discard the last minute") { monitor.scrub(minutes: 1) }
        Button("Discard the last 5 minutes") { monitor.scrub(minutes: 5) }
        Button("Discard the last 30 minutes") { monitor.scrub(minutes: 30) }
        if let scrub = monitor.lastScrub {
            Text(scrub)
        }
        Button("Reveal logs in Finder") {
            NSWorkspace.shared.selectFile(
                nil, inFileViewerRootedAtPath: monitor.logDirectory.path)
        }
        Divider()
        Button("Quit") { NSApp.terminate(nil) }
            .keyboardShortcut("q")
    }
}

/// The window: a tab strip, the status line, and whichever screen is showing.
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
            tabs
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

    /// The tab strip: two buttons, drawn to look like the segmented control it replaces.
    ///
    /// It *was* a segmented `Picker`, and that turned out to be what made the window
    /// unusable after a day or two.  The AppKit segmented control behind
    /// `.pickerStyle(.segmented)` sizes itself by asking AppKit, which re-enters the
    /// SwiftUI view graph to measure the label inside each segment -- and every one of
    /// those passes left the picker's tag machinery behind.  A day and a half of ordinary
    /// collecting had leaked 223,683 copies of it and twice that many observation
    /// registrars, and since each redraw walks what the last one leaked, the app got
    /// steadily slower until the main thread was doing nothing else.
    ///
    /// Plain buttons carry none of that: no tags, no AppKit control, no measurement that
    /// re-enters the graph.
    private var tabs: some View {
        HStack(spacing: 2) {
            ForEach(Mode.allCases) { tab in
                Button {
                    mode = tab
                } label: {
                    Text(tab.rawValue)
                        .font(.system(size: 13, weight: mode == tab ? .semibold : .regular))
                        .foregroundStyle(mode == tab ? Color.primary : Color.secondary)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 3)
                        .background(
                            RoundedRectangle(cornerRadius: 5)
                                .fill(Color(nsColor: .controlBackgroundColor))
                                .opacity(mode == tab ? 1 : 0)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(2)
        .background(RoundedRectangle(cornerRadius: 7).fill(.quaternary))
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
                // Spelled out where it happens, rather than left as a label to be asked
                // about.  It names both fingerprints because which side is behind is the
                // whole question, and it is not always the keyboard.
                if let mismatch = monitor.status.mismatch {
                    Text(mismatch.detail)
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .textSelection(.enabled)
                        .padding(.top, 4)
                }
            }
            Spacer()
        }
        .padding(12)
    }

    private var statusColor: Color {
        switch monitor.status {
        case .connected(_, let mismatch): return mismatch == nil ? .green : .orange
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
