import AppKit
import Foundation
import MinderKit
import TaipoKit

/// Connects to the keyboard, streams the key log, and turns it into chords.
///
/// The long poll blocks a thread, so all of it runs on a private queue and publishes onto
/// the main one.  There is no async/await here on purpose: `MinderKit`'s transport is
/// synchronous because IOUSBHost's synchronous form is what the spike proved, and wrapping
/// it in actors would add concurrency without adding capability.
@MainActor
public final class DeviceMonitor: ObservableObject {
    @Published public private(set) var status: Status = .disconnected
    @Published public private(set) var chords: [LiveChord] = []
    @Published public private(set) var recording = false
    /// Paused by the user.  Stops the *device* recording, not just this end, so a pause
    /// means the keyboard is not keeping anything either.
    @Published public var paused = false {
        didSet { pauseRequested = paused }
    }
    /// Chords seen today, for the menu bar.
    @Published public private(set) var chordsToday = 0
    /// Recording is suspended because macOS has secure keyboard entry on: a password field,
    /// a `sudo` prompt, the login window, or the lock screen.
    @Published public private(set) var secureInput = false
    /// Where the logs are being written.
    public let logDirectory = LogWriter.defaultDirectory
    /// The drill in progress, if the practice screen is showing.
    @Published public private(set) var drill: DrillSession?

    /// Practice lines.
    ///
    /// A fixed list for now.  taipo-teacher.md's adaptive sampler wants the model store and
    /// a corpus, and neither exists yet; these are chosen to put the n-gram chords in front
    /// of the fingers, since phase 3 found six grams spelled out in the first 76 chords of
    /// real typing.
    private static let corpus = [
        "the other thing is that",
        "information for the nation",
        "another one of these",
        "he said that there were",
        "in the morning and the evening",
        "we should consider the question",
    ]
    private var corpusIndex = 0
    /// The alternation rule, applied to everything that arrives.  The same class the drill
    /// uses, so the strip and the score cannot disagree about what a fault is.
    private let alternation = AlternationTracker()

    /// A chord, with what it typed, for display.
    public struct LiveChord: Identifiable {
        public let id = UUID()
        public let chord: TaipoKit.Chord
        public let types: String?
        public let dead: Bool
        /// Stayed on the previous chord's hand when it need not have.
        ///
        /// Judged here rather than only in the drill, so that a chord which types nothing
        /// -- a backspace, a modifier -- can still show its fault.  There is nowhere in the
        /// target line to mark a backspace.
        public let sameHand: Bool
    }

    public enum Status: Equatable {
        case disconnected
        case connecting
        case connected(device: String, fingerprintMatches: Bool)
        case failed(String)

        public var summary: String {
            switch self {
            case .disconnected: return "Not connected"
            case .connecting: return "Connecting…"
            case .connected(let device, let ok):
                return ok ? device : "\(device) — layout mismatch"
            case .failed(let why): return "Failed: \(why)"
            }
        }
    }

    /// How many chords to keep on screen.
    private let historyLimit = 40

    private let queue = DispatchQueue(label: "org.davidb.taipo-teacher.device")
    private var running = false
    private var layouts: Layouts?
    /// Only ever touched from `queue`, which is a single serial queue, so this is safe
    /// outside the actor.  Saying so explicitly rather than letting the isolation be
    /// implied: it is the kind of invariant that quietly stops being true.
    nonisolated(unsafe) private let log = LogWriter()
    /// Read from the device thread; written from the main one.
    private var pauseRequested = false

    public init() {
        // Collecting starts with the app, not with a window: the point of the menu bar is
        // that it runs whether or not anything is on screen.
        start()

        // Quitting has to tell the keyboard to stop, or it goes on recording into its own
        // RAM with nobody draining it.  Nothing reaches disk -- the buffer is discarded on
        // the next connect -- but "the keyboard is still logging" is not a state to leave
        // behind when the app that asked for it is gone.
        NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.shutDown() }
        }
    }

    /// Stop, and wait for the device thread to put the keyboard back as it found it.
    ///
    /// Bounded: a keyboard that has been unplugged will never answer, and hanging the quit
    /// on it would be worse than leaving it recording into RAM it loses at power off.
    public func shutDown() {
        running = false
        let done = DispatchSemaphore(value: 0)
        queue.async { done.signal() }
        _ = done.wait(timeout: .now() + 2.0)
    }

    /// What the menu bar shows at a glance.
    public var menuBarSymbol: String {
        if case .failed = status { return "keyboard.badge.exclamationmark" }
        if secureInput { return "keyboard.badge.eye" }
        if paused { return "keyboard" }
        return recording ? "keyboard.fill" : "keyboard"
    }

    /// What the app is doing, in a phrase.
    public var activity: String {
        if secureInput { return "Paused — password field" }
        if paused { return "Paused" }
        if recording { return "\(chordsToday) chords today" }
        return "Not recording"
    }

    public func start() {
        guard !running else { return }
        running = true
        status = .connecting
        queue.async { [weak self] in self?.reconnectLoop() }
    }

    public func stop() {
        running = false
    }

    /// Start the next practice line.
    public func nextDrill() {
        guard let layouts else { return }
        let text = Self.corpus[corpusIndex % Self.corpus.count]
        corpusIndex += 1
        drill = DrillSession(target: DrillTarget(text: text, layouts: layouts), layouts: layouts)
    }

    /// Start the current line over.
    public func restartDrill() {
        guard let layouts, let text = drill?.target.text else { return }
        drill = DrillSession(target: DrillTarget(text: text, layouts: layouts), layouts: layouts)
    }

    /// Reconnect until told to stop.
    ///
    /// A keyboard gets unplugged, and a keyboard gets reflashed.  Either way the poll fails
    /// and the right answer is to wait and try again rather than to give up until the app
    /// is restarted -- a collector that quietly stops collecting is worse than one that
    /// never started.
    private nonisolated func reconnectLoop() {
        var lastBootID: UInt64?
        while isRunning {
            let boot = runSession(previousBootID: lastBootID)
            if let boot { lastBootID = boot }
            // Backoff, in short steps, so quitting does not wait it out.
            for _ in 0..<20 {
                guard isRunning else { break }
                Thread.sleep(forTimeInterval: 0.1)
            }
        }
        Task { @MainActor [weak self] in
            self?.recording = false
            self?.running = false
        }
    }

    /// One connection's worth of collecting.  Returns the boot id it saw.
    @discardableResult
    private nonisolated func runSession(previousBootID: UInt64?) -> UInt64? {
        let device: MinderDevice
        let hello: HelloInfo
        let layouts: Layouts
        do {
            layouts = try Layouts.bundled()
            device = try MinderDevice()
            device.drain()
            guard case .hello(let info) = try device.call(.hello(version: MinderVersion.current))
            else { throw MinderDevice.DeviceError.notFound(serial: nil) }
            hello = info
        } catch {
            Task { @MainActor [weak self] in self?.status = .failed("\(error)") }
            return nil
        }

        // The tables the app names chords with have to be the ones the keyboard is running,
        // or every chord it shows is a plausible lie.  Reported rather than fatal: the app
        // is still useful, and saying so is better than refusing.
        let matches = hello.layoutFingerprint == layouts.fingerprintValue
        Task { @MainActor [weak self] in
            self?.layouts = layouts
            self?.status = .connected(device: hello.info, fingerprintMatches: matches)
            self?.nextDrill()
        }

        guard hello.supports(Capability.keyLog) else {
            Task { @MainActor [weak self] in
                self?.status = .failed("firmware has no key log")
                self?.running = false
            }
            return hello.bootID
        }

        // A different boot id means the keyboard restarted: whatever it was buffering is
        // gone, and the timeline it was writing has a hole in it.  Say so in the file
        // rather than letting the deltas imply continuity that is not there.
        if let previous = previousBootID, previous != hello.bootID {
            self.log.noteReset()
        }
        self.log.beginSession(
            device: hello.info, bootID: hello.bootID,
            fingerprint: hello.layoutFingerprint)

        let engine = ChordEngine(layouts: layouts)
        // The device's own timeline, rebuilt from the record deltas.  `Clock` also tracks
        // how long ago that was in host time, so the engine's window can expire between
        // keystrokes rather than waiting for the next one.
        var clock = Clock()

        do {
            // Anything already buffered predates this session.  Records survive a
            // `SetLogging(false)` -- disabling stops recording, it does not discard what
            // was never acked -- so without this the first drain replays an earlier
            // session's typing into the drill, and the target goes red before a key is
            // touched.  `keyminder log` learned the same lesson; this is the same fix.
            try self.discardBuffered(device)

            // Watermark 1: tell us as soon as there is anything.  Batches cost a USB frame
            // per packet, so many small drains beat one big one for latency.
            _ = try device.call(.setLogging(enabled: true, watermark: 1))
            Task { @MainActor [weak self] in self?.recording = true }

            var wasPaused = false
            while self.isRunning {
                // Pausing stops the device recording, not just this end: the point of a
                // pause is that nothing is being kept, and the records live on the
                // keyboard until they are fetched.
                // Checked before anything is fetched, which is what makes it safe: a
                // password keystroke is still sitting in the device's buffer at this
                // point, so it is discarded rather than drained to disk.
                let secure = SecureInput.isEnabled
                Task { @MainActor [weak self] in self?.secureInput = secure }

                let pause = self.isPaused || secure
                if pause != wasPaused {
                    _ = try device.call(.setLogging(enabled: !pause, watermark: 1))
                    // Discard on the way into a pause as well as out of one.  Going in,
                    // the buffer may hold the first characters of a password; coming out,
                    // it may hold whatever the device recorded before it was told to stop.
                    try self.discardBuffered(device)
                    wasPaused = pause
                    Task { @MainActor [weak self] in self?.recording = !pause }
                }
                if pause {
                    Thread.sleep(forTimeInterval: 0.25)
                    continue
                }

                // A poll that expires is not an error, just a quiet moment.
                _ = try device.call(.getEvent(timeoutMs: 300), timeout: 5.0)
                try self.drain(device, engine: engine, clock: &clock)
                // Nothing arrived, but time still passed: a chord held past the window
                // commits on the timer, and seven in ten do.
                self.publish(engine.advance(toMs: clock.estimatedNowMs), engine: engine)
            }
        } catch {
            // An unplug looks exactly like this.  Report it and let the outer loop retry.
            Task { @MainActor [weak self] in
                self?.status = .failed("\(error)")
                self?.recording = false
            }
            return hello.bootID
        }

        _ = try? device.call(.setLogging(enabled: false, watermark: 0))
        Task { @MainActor [weak self] in self?.recording = false }
        return hello.bootID
    }

    private nonisolated var isPaused: Bool {
        var value = false
        DispatchQueue.main.sync { value = MainActor.assumeIsolated { self.pauseRequested } }
        return value
    }

    private nonisolated var isRunning: Bool {
        var value = false
        DispatchQueue.main.sync { value = MainActor.assumeIsolated { self.running } }
        return value
    }

    /// Fetch, decode, feed, and ack, until the device says there is no more.
    /// The device's millisecond timeline, and how to guess where it is now.
    struct Clock {
        /// Sum of the record deltas seen so far.
        var deviceMs: UInt64 = 0
        /// Host time when `deviceMs` was last known to be current.
        var syncedAt = Date()
        /// Whether `deviceMs` has ever been anchored to the present.
        var anchored = false

        /// Where the device's clock is now, as well as can be told.
        ///
        /// Between drains this is the last anchor plus host elapsed time.  Drift over a
        /// second or two of quiet is irrelevant: it is only used to decide that a 100ms
        /// window has expired.
        var estimatedNowMs: UInt32 {
            let elapsed = anchored ? Date().timeIntervalSince(syncedAt) * 1000 : 0
            return UInt32(truncatingIfNeeded: deviceMs + UInt64(max(0, elapsed)))
        }
    }

    private nonisolated func drain(
        _ device: MinderDevice, engine: ChordEngine, clock: inout Clock
    ) throws {
        while true {
            // Re-checked each time round: a batch can span the moment a password field
            // takes focus, and the rest of it must not be written.
            if SecureInput.isEnabled { return }
            guard case .eventLog(let batch) = try device.call(.getEventLog(maxBytes: 800))
            else { return }
            let records = LogRecord.decodeAll(batch.records)
            if records.isEmpty { return }

            // To disk before acking: acking is what lets the device forget, so anything
            // written after it would be lost by a crash in between.
            if batch.dropped > 0 {
                self.log.noteGap(dropped: batch.dropped, beforeSeq: batch.seq)
            }
            if let layouts = self.layoutsSync {
                self.log.append(records, layouts: layouts)
            }

            var produced: [TaipoKit.Chord] = []
            for record in records {
                clock.deviceMs += record.delta.milliseconds
                switch record {
                case .key(let code, let press, _):
                    produced += engine.feed(
                        key: Int(code), press: press,
                        timeMs: UInt32(truncatingIfNeeded: clock.deviceMs))
                case .marker(let marker, let value, _):
                    engine.marker(marker.name, value: value)
                case .unknown:
                    break
                }
            }

            let count = UInt32(records.count)
            _ = try device.call(.eventLogAck(throughSeq: batch.seq &+ count &- 1))

            // `anchor_ms` measures from the newest record in the batch, which is exactly
            // what ties the device's timeline to the present.  It is only meaningful for a
            // complete batch; a partial one reports a sentinel, because the device keeps no
            // timestamp per record and a plausible wrong number would be worse than none.
            if batch.remaining == 0 && batch.anchorMs != UInt32.max {
                clock.deviceMs += UInt64(batch.anchorMs)
                clock.syncedAt = Date()
                clock.anchored = true
                produced += engine.advance(toMs: UInt32(truncatingIfNeeded: clock.deviceMs))
            }

            self.publish(produced, engine: engine)
            if batch.remaining == 0 { return }
        }
    }

    /// Throw away whatever the device still holds, so a session starts clean.
    private nonisolated func discardBuffered(_ device: MinderDevice) throws {
        while true {
            guard case .eventLog(let batch) = try device.call(.getEventLog(maxBytes: 3200))
            else { return }
            let count = batch.records.count / LogRecord.size
            if count == 0 { return }
            _ = try device.call(
                .eventLogAck(throughSeq: batch.seq &+ UInt32(count) &- 1))
            if batch.remaining == 0 { return }
        }
    }

    /// Send finished chords to the display.
    private nonisolated func publish(_ chords: [TaipoKit.Chord], engine: ChordEngine) {
        guard !chords.isEmpty else { return }
        let layouts = self.layoutsSync
        Task { @MainActor [weak self] in
            guard let self else { return }
            let live = chords.map { chord -> LiveChord in
                let entry = layouts?.chord(chord.code, variant: chord.variant)
                return LiveChord(
                    chord: chord, types: entry?.action.types, dead: entry == nil,
                    sameHand: self.alternation.note(chord))
            }
            self.append(live)
        }
    }

    private nonisolated var layoutsSync: Layouts? {
        var value: Layouts?
        DispatchQueue.main.sync { value = MainActor.assumeIsolated { self.layouts } }
        return value
    }

    private func append(_ new: [LiveChord]) {
        chordsToday += new.count
        chords.append(contentsOf: new)
        if chords.count > historyLimit {
            chords.removeFirst(chords.count - historyLimit)
        }
        guard let drill else { return }
        for live in new {
            // The control chords are checked first and never reach the score.  Both type
            // nothing anyway: Enter is not in any target, and the null chord exists to
            // release modifiers.
            switch drill.control(for: live.chord) {
            case .next where drill.finished:
                nextDrill()
                return
            case .restart:
                restartDrill()
                return
            case .next, .none:
                break
            }
            if !drill.finished {
                drill.feed(live.chord)
            }
        }
        // DrillSession is a class, so SwiftUI needs telling that it changed.
        objectWillChange.send()
    }
}
