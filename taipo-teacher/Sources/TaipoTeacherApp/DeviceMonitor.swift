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
        }

        guard hello.supports(Capability.keyLog) else {
            Task { @MainActor [weak self] in
                self?.status = .failed("firmware has no key log")
                self?.running = false
            }
            return
        }

        let engine = ChordEngine(layouts: layouts)
        var clock: UInt64 = 0

        do {
            // Watermark 1: tell us as soon as there is anything.  Batches cost a USB frame
            // per packet, so many small drains beat one big one for latency.
            _ = try device.call(.setLogging(enabled: true, watermark: 1))
            Task { @MainActor [weak self] in self?.recording = true }

            while self.isRunning {
                // A poll that expires is not an error, just a quiet moment.
                _ = try device.call(.getEvent(timeoutMs: 1000), timeout: 5.0)
                try self.drain(device, engine: engine, clock: &clock)
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
    private nonisolated func drain(
        _ device: MinderDevice, engine: ChordEngine, clock: inout UInt64
    ) throws {
        while true {
            guard case .eventLog(let batch) = try device.call(.getEventLog(maxBytes: 800))
            else { return }
            let records = LogRecord.decodeAll(batch.records)
            if records.isEmpty { return }

            var produced: [TaipoKit.Chord] = []
            for record in records {
                clock += record.delta.milliseconds
                switch record {
                case .key(let code, let press, _):
                    produced += engine.feed(
                        key: Int(code), press: press, timeMs: UInt32(truncatingIfNeeded: clock))
                case .marker(let marker, let value, _):
                    engine.marker(marker.name, value: value)
                case .unknown:
                    break
                }
            }

            let count = UInt32(records.count)
            _ = try device.call(.eventLogAck(throughSeq: batch.seq &+ count &- 1))

            if !produced.isEmpty {
                let live = produced.map { chord -> LiveChord in
                    let entry = self.layoutsSync?.chord(chord.code, variant: chord.variant)
                    return LiveChord(
                        chord: chord, types: entry?.action.types, dead: entry == nil)
                }
                Task { @MainActor [weak self] in self?.append(live) }
            }
            if batch.remaining == 0 { return }
        }
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
    }
}
