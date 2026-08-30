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

    /// A chord, with what it typed, for display.
    public struct LiveChord: Identifiable {
        public let id = UUID()
        public let chord: TaipoKit.Chord
        public let types: String?
        public let dead: Bool
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

    public init() {}

    public func start() {
        guard !running else { return }
        running = true
        status = .connecting
        queue.async { [weak self] in self?.run() }
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

    /// The whole device loop: connect, greet, enable logging, drain forever.
    private nonisolated func run() {
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
            Task { @MainActor [weak self] in
                self?.status = .failed("\(error)")
                self?.running = false
            }
            return
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
            return
        }

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

            while self.isRunning {
                // A poll that expires is not an error, just a quiet moment.
                _ = try device.call(.getEvent(timeoutMs: 1000), timeout: 5.0)
                try self.drain(device, engine: engine, clock: &clock)
                // Nothing arrived, but time still passed: a chord held past the window
                // commits on the timer, and seven in ten do.
                self.publish(engine.advance(toMs: clock.estimatedNowMs), engine: engine)
            }
        } catch {
            Task { @MainActor [weak self] in self?.status = .failed("\(error)") }
        }

        _ = try? device.call(.setLogging(enabled: false, watermark: 0))
        Task { @MainActor [weak self] in
            self?.recording = false
            self?.running = false
        }
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
            guard case .eventLog(let batch) = try device.call(.getEventLog(maxBytes: 800))
            else { return }
            let records = LogRecord.decodeAll(batch.records)
            if records.isEmpty { return }

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
        let live = chords.map { chord -> LiveChord in
            let entry = layouts?.chord(chord.code, variant: chord.variant)
            return LiveChord(chord: chord, types: entry?.action.types, dead: entry == nil)
        }
        Task { @MainActor [weak self] in self?.append(live) }
    }

    private nonisolated var layoutsSync: Layouts? {
        var value: Layouts?
        DispatchQueue.main.sync { value = MainActor.assumeIsolated { self.layouts } }
        return value
    }

    private func append(_ new: [LiveChord]) {
        chords.append(contentsOf: new)
        if chords.count > historyLimit {
            chords.removeFirst(chords.count - historyLimit)
        }
        // A finished drill keeps its result on screen until the next one is asked for.
        if let drill, !drill.finished {
            for live in new {
                drill.feed(live.chord)
            }
            // DrillSession is a class, so SwiftUI needs telling that it changed.
            objectWillChange.send()
        }
    }
}
