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
        didSet { pausedFlag.current = paused }
    }
    /// Chords seen today, for the menu bar.
    @Published public private(set) var chordsToday = 0
    /// Recording is suspended because macOS has secure keyboard entry on: a password field,
    /// a `sudo` prompt, the login window, or the lock screen.
    @Published public private(set) var secureInput = false
    /// What the last scrub did, so the menu can confirm it happened.
    @Published public private(set) var lastScrub: String?
    /// Where the logs are being written.
    public let logDirectory = LogWriter.defaultDirectory
    /// The drill in progress, if the practice screen is showing.
    @Published public private(set) var drill: DrillSession?

    /// The chord table the keyboard is actually in, as its own log reports it.
    ///
    /// Everything the practice screen builds is per-variant -- the segmentation of a
    /// target, the confusions, the ladder -- because Taipo and Dosh are different skills
    /// that happen to share an engine.  Drilling Dosh against Taipo's table would score
    /// the right typing as wrong and, worse, teach the wrong chords.
    ///
    /// Seeded from `layouts.default_variant` as soon as the tables load, and thereafter
    /// whatever the log's markers last said.  The keyboard announces its state when the
    /// collector connects, so this placeholder is only what the menu shows for the moment
    /// before the tables are in hand.
    @Published public private(set) var variant: String = "taipo"

    /// Practice lines.
    ///
    /// A fixed list for now.  taipo-teacher.md's adaptive sampler wants the model store and
    /// a corpus, and neither exists yet; these are chosen to put the n-gram chords in front
    /// of the fingers, since phase 3 found six grams spelled out in the first 76 chords of
    /// real typing.
    /// Something to type before the logs have anything to say.
    ///
    /// Only used until there are corrections to build a drill from -- a keyboard on its
    /// first day has nothing the writer is known to get wrong, and an empty practice
    /// screen would be worse than an arbitrary one.
    private static let corpus = [
        "the other thing is that",
        "information for the nation",
        "another one of these",
        "he said that there were",
        "in the morning and the evening",
        "we should consider the question",
    ]
    private var corpusIndex = 0

    /// Which kind of practice the screen is giving.
    public enum PracticeMode: String, CaseIterable, Identifiable, Sendable {
        /// The ladder: unlock as you learn, weighted toward the weakest.
        case ladder = "Ladder"
        /// The pairs the writer's own corrections say get mixed up.
        case confusions = "Confusions"
        public var id: String { rawValue }
    }

    @Published public var practiceMode: PracticeMode = .ladder {
        didSet {
            guard practiceMode != oldValue else { return }
            programme = []
            drillIndex = 0
            lineIndex = 0
            rebuildProgramme()
        }
    }

    /// Where the ladder has got to, for the screen to draw.  Nil in confusion mode, and
    /// until the first rebuild finishes.
    @Published public private(set) var ladder: Ladder?

    /// What the logs say about each chord, for the hint to fade by.  Nil in confusion
    /// mode, and until the first rebuild finishes.
    @Published public private(set) var skill: SkillModel?

    /// How many logged sessions were passed over as recorded against other chord tables.
    ///
    /// Worth saying out loud.  Reflashing with a changed table resets everything derived
    /// from before it, and a writer who finds the ladder back at the beginning deserves
    /// to be told why rather than left to wonder whether it is broken.
    @Published public private(set) var skippedSessions = 0

    /// The practice programme, built from what the writer keeps correcting.
    ///
    /// Rebuilt when the practice screen opens rather than kept up to date continuously:
    /// the logs only grow, so the model can only be stale by one sitting, and re-reading
    /// three days of them takes well under a second.  There is nothing stored between
    /// runs, which is the point -- the logs are the model.
    @Published public private(set) var programme: [Drill] = []
    /// Which line of which drill is up.
    private var drillIndex = 0
    private var lineIndex = 0
    /// The heading for the line being typed, for the practice screen to show.
    @Published public private(set) var drillTitle: String?
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
        case connected(device: String, mismatch: LayoutMismatch?)
        case failed(String)

        public var summary: String {
            switch self {
            case .disconnected: return "Not connected"
            case .connecting: return "Connecting…"
            case .connected(let device, let mismatch):
                guard mismatch != nil else { return device }
                return "\(device) — wrong chord tables"
            case .failed(let why): return "Failed: \(why)"
            }
        }

        /// The two fingerprints, when they disagree, so the label can say which is which.
        public var mismatch: LayoutMismatch? {
            if case .connected(_, let mismatch) = self { return mismatch }
            return nil
        }
    }

    /// The keyboard's tables and the app's, when they are not the same tables.
    ///
    /// Both halves, because which one is behind is the whole question and the answer is
    /// usually not the obvious one.  The first time this fired in earnest the keyboard was
    /// running exactly the right firmware and the app was shipping a `layouts.json` two
    /// commits out of date -- and "layout mismatch" on its own reads as an accusation
    /// against the keyboard, which sent the developer looking in the wrong place.
    public struct LayoutMismatch: Equatable, Sendable {
        public let device: UInt64?
        public let app: UInt64?

        public var detail: String {
            let name = { (v: UInt64?) in v.map { String(format: "%#018llx", $0) } ?? "unknown" }
            return """
                The keyboard is running chord tables this app does not have a copy of, so \
                every chord it names may be wrong.

                keyboard  \(name(device))
                app       \(name(app))

                If the keyboard is the newer one, the app needs rebuilding from a checkout \
                that has those tables.  If the app is, the keyboard needs reflashing.
                """
        }
    }

    /// How many chords to keep on screen.
    private let historyLimit = 40

    /// A flag the device thread reads and the main actor writes.
    ///
    /// These are checked once or twice per poll.  They used to be fetched with
    /// `DispatchQueue.main.sync`, which meant the collector stopped dead for as long as the
    /// main thread was busy -- and a stalled collector does not merely lag: the keyboard's
    /// ring buffer fills and drops the records nobody arrived to fetch.  A sample taken
    /// while the window was misbehaving found the device thread spending four fifths of its
    /// time waiting on exactly this.
    private final class Flag: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Bool
        init(_ value: Bool) { self.value = value }
        var current: Bool {
            get { lock.lock(); defer { lock.unlock() }; return value }
            set { lock.lock(); value = newValue; lock.unlock() }
        }
    }

    private let queue = DispatchQueue(label: "org.davidb.taipo-teacher.device")
    private let runningFlag = Flag(false)
    private let pausedFlag = Flag(false)
    private(set) var layouts: Layouts?
    /// Only ever touched from `queue`, which is a single serial queue, so this is safe
    /// outside the actor.  Saying so explicitly rather than letting the isolation be
    /// implied: it is the kind of invariant that quietly stops being true.
    nonisolated(unsafe) private let log = LogWriter()

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

    /// Throw away everything logged in the last `minutes`, on the device and on disk.
    ///
    /// For the cases prevention cannot reach.  macOS only asserts secure input when the
    /// app that owns the text field asks it to, and a password prompt inside tmux never
    /// does: Terminal sees tmux's tty, not sudo's.  Rather than pretend the cover is
    /// complete, this is the way to take something back.
    public func scrub(minutes: Int) {
        let cutoff = Date().addingTimeInterval(-Double(minutes) * 60)
        chords.removeAll()
        drill.map { _ in restartDrill() }
        queue.async { [weak self] in
            guard let self else { return }
            let bytes = self.log.discard(since: cutoff)
            Task { @MainActor [weak self] in
                self?.lastScrub = "Discarded \(bytes) bytes from the last \(minutes) min"
            }
        }
    }

    /// Stop, and wait for the device thread to put the keyboard back as it found it.
    ///
    /// Bounded: a keyboard that has been unplugged will never answer, and hanging the quit
    /// on it would be worse than leaving it recording into RAM it loses at power off.
    public func shutDown() {
        runningFlag.current = false
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
        guard !runningFlag.current else { return }
        runningFlag.current = true
        status = .connecting
        queue.async { [weak self] in self?.reconnectLoop() }
    }

    public func stop() {
        runningFlag.current = false
    }

    /// Whether the practice screen is on show.
    ///
    /// The drill only consumes chords while it is.  It used to consume every chord the
    /// keyboard produced, window open or not, so a day at work fed the whole day's typing
    /// into whichever line happened to be up: the target went red within seconds and stayed
    /// there, the score turned to nonsense, and `typed`, `events` and `sameHandOffsets`
    /// grew without bound behind it.  Practice is something you sit down to, not something
    /// that happens to you while you work.
    private var practicing = false

    /// Whether the practice window has the keyboard.
    ///
    /// The screen being up is not the same as being typed into, and the same argument
    /// applies a second time: a visible but unfocused window went on scoring everything
    /// typed in the editor next to it.  Kept apart from `practicing` because that one also
    /// gates building the material, and a full rebuild on every click away would be
    /// absurd.
    ///
    /// Regaining the keyboard starts the line again.  A line half typed twenty minutes ago
    /// cannot be resumed honestly -- the clock the speed is measured against has been
    /// running the whole time -- and half a line with a broken clock is worth less than
    /// retyping it.
    @Published public var focused = true {
        didSet {
            guard focused != oldValue, practicing else { return }
            if focused { restartDrill() }
        }
    }

    /// The practice screen has appeared: start a fresh line and begin scoring.
    ///
    /// The programme is rebuilt off the main thread first, since replaying the logs and
    /// segmenting the word list takes a couple of seconds and the window is already up.
    /// Until it arrives the fallback corpus is what gets typed.
    public func beginPractice() {
        practicing = true
        nextDrill()
        rebuildProgramme()
    }

    /// Replay the logs and build the practice material for the variant now live.
    ///
    /// The variant is captured rather than read again on the way back: the keyboard can
    /// switch tables while the logs are being replayed, and material built for the table
    /// it has left is worse than none.
    private func rebuildProgramme() {
        guard practicing, let layouts else { return }
        let directory = logDirectory
        let variant = self.variant
        let mode = self.practiceMode
        // A fresh seed each time, so a block of ladder lines is never the block before.
        let seed = UInt64(Date().timeIntervalSince1970)
        Task.detached(priority: .userInitiated) {
            var ladder: Ladder?
            var model: SkillModel?
            let programme: [Drill]
            var skipped = 0
            switch mode {
            case .ladder:
                let skill = SkillStore.model(
                    logDirectory: directory,
                    cache: SkillStore.defaultURL(forLogsIn: directory),
                    layouts: layouts, variant: variant)
                let built = Ladder(layouts: layouts, variant: variant, skill: skill)
                ladder = built
                model = skill
                let drill = LadderMaker(layouts: layouts, variant: variant)
                    .drill(built, lines: Self.ladderBlock, seed: seed)
                programme = drill.lines.isEmpty ? [] : [drill]
                skipped = skill.skipped
            case .confusions:
                let model = ConfusionModel.build(
                    logDirectory: directory, layouts: layouts, variant: variant)
                programme = DrillMaker(layouts: layouts, variant: variant).programme(model)
                skipped = model.skipped
            }
            let result = (ladder, programme, skipped, model)
            await MainActor.run { [weak self] in
                guard let self, self.practicing, self.variant == variant,
                    self.practiceMode == mode
                else { return }
                self.skippedSessions = result.2
                self.skill = result.3
                guard !result.1.isEmpty else { return }
                self.ladder = result.0
                self.programme = result.1
                self.drillIndex = 0
                self.lineIndex = 0
                self.nextDrill()
            }
        }
    }

    /// Re-read the logs and update the measurements, leaving the lines alone.
    ///
    /// Split from `rebuildProgramme` because the two want different rhythms.  The
    /// *material* has to hold still while it is being typed -- regenerating it every line
    /// would move the target out from under the writer -- but the *measurements* are what
    /// the screen is reporting, and they were only refreshed when a block of sixteen lines
    /// ran out.  A few drills therefore changed nothing on screen while the model behind
    /// it moved by thousands of chords.
    ///
    /// New material is asked for only when the ladder actually unlocks something, which is
    /// rare and is exactly when the lines are out of date.
    private func refreshSkill() {
        guard practicing, practiceMode == .ladder, !refreshing, let layouts else { return }
        refreshing = true
        let directory = logDirectory
        let variant = self.variant
        Task.detached(priority: .utility) {
            let skill = SkillStore.model(
                logDirectory: directory,
                cache: SkillStore.defaultURL(forLogsIn: directory),
                layouts: layouts, variant: variant)
            let built = Ladder(layouts: layouts, variant: variant, skill: skill)
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.refreshing = false
                guard self.practicing, self.practiceMode == .ladder, self.variant == variant
                else { return }
                let unlocked = self.ladder?.unlockedCount
                self.skill = skill
                self.ladder = built
                self.skippedSessions = skill.skipped
                if let unlocked, built.unlockedCount != unlocked { self.rebuildProgramme() }
            }
        }
    }

    /// Whether a refresh is already in flight, so they cannot pile up on a fast writer.
    private var refreshing = false

    /// How many lines a ladder block holds before the model is asked again.
    ///
    /// The block is the unit of progress: finishing one sends the logs -- which now
    /// include the block just typed -- back through the skill model, so the ladder moves
    /// on exactly as much as the typing earned.  Long enough to be worth measuring, short
    /// enough that a newly learned item does not have to wait out a whole sitting.
    nonisolated static let ladderBlock = 16

    /// The practice screen has gone away.  Collecting carries on; scoring does not.
    public func endPractice() {
        practicing = false
        ladder = nil
        skill = nil
        drill = nil
        drillTitle = nil
    }

    /// Start the next practice line.
    ///
    /// Walks the programme a line at a time and then moves on to the next confusion, so a
    /// pair is warmed up and then practised in words before the next one starts.
    public func nextDrill() {
        guard let layouts else { return }
        let text: String
        if programme.isEmpty {
            text = Self.corpus[corpusIndex % Self.corpus.count]
            corpusIndex += 1
            drillTitle = nil
        } else {
            if lineIndex >= programme[drillIndex].lines.count {
                drillIndex = (drillIndex + 1) % programme.count
                lineIndex = 0
                // A ladder block is finished: re-read the logs, which now hold the block
                // itself, and take whatever progress it earned.  The rebuild is not
                // instant, so the current block is typed again meanwhile rather than
                // leaving the screen empty.
                if practiceMode == .ladder { rebuildProgramme() }
            }
            text = programme[drillIndex].lines[lineIndex]
            drillTitle = programme[drillIndex].title
            lineIndex += 1
            // A line has just been finished with, so what the logs say about it has
            // changed.  Cheap: only the day in progress is replayed.
            refreshSkill()
        }
        drill = DrillSession(
            target: DrillTarget(text: text, layouts: layouts, variant: variant),
            layouts: layouts)
    }

    /// Start the current line over.
    public func restartDrill() {
        guard let layouts, let text = drill?.target.text else { return }
        drill = DrillSession(
            target: DrillTarget(text: text, layouts: layouts, variant: variant),
            layouts: layouts)
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
        runningFlag.current = false
        Task { @MainActor [weak self] in self?.recording = false }
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
        let mismatch =
            hello.layoutFingerprint == layouts.fingerprintValue
            ? nil
            : LayoutMismatch(device: hello.layoutFingerprint, app: layouts.fingerprintValue)
        Task { @MainActor [weak self] in
            self?.layouts = layouts
            self?.variant = layouts.defaultVariant
            self?.status = .connected(device: hello.info, mismatch: mismatch)
            self?.nextDrill()
        }

        guard hello.supports(Capability.keyLog) else {
            runningFlag.current = false
            Task { @MainActor [weak self] in
                self?.status = .failed("firmware has no key log")
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
            // Unknown to start with, so the first pass always publishes and a session that
            // begins where the last one left off cannot inherit a stale flag.
            var wasSecure: Bool?
            while self.isRunning {
                // Pausing stops the device recording, not just this end: the point of a
                // pause is that nothing is being kept, and the records live on the
                // keyboard until they are fetched.
                // Checked before anything is fetched, which is what makes it safe: a
                // password keystroke is still sitting in the device's buffer at this
                // point, so it is discarded rather than drained to disk.
                let secure = SecureInput.isEnabled
                // Only when it changes.  This is read three or four times a second and the
                // answer is the same almost every time; assigning it regardless still fires
                // `objectWillChange`, and every one of those redrew the whole window and
                // the menu bar for nothing.  That was not merely wasted work -- each pass
                // leaked a little SwiftUI observation state, so an idle poll made the app
                // slower the longer it ran.
                if secure != wasSecure {
                    wasSecure = secure
                    Task { @MainActor [weak self] in self?.secureInput = secure }
                }

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
                try self.drain(device, engine: engine, clock: &clock, layouts: layouts)
                // Nothing arrived, but time still passed: a chord held past the window
                // commits on the timer, and seven in ten do.
                self.publish(engine.advance(toMs: clock.estimatedNowMs), layouts: layouts)
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

    private nonisolated var isPaused: Bool { pausedFlag.current }

    private nonisolated var isRunning: Bool { runningFlag.current }

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
        _ device: MinderDevice, engine: ChordEngine, clock: inout Clock, layouts: Layouts
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
            self.log.append(records, layouts: layouts)

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

            self.publish(produced, layouts: layouts)
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
    private nonisolated func publish(_ chords: [TaipoKit.Chord], layouts: Layouts) {
        guard !chords.isEmpty else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let live = chords.map { chord -> LiveChord in
                let entry = layouts.chord(chord.code, variant: chord.variant)
                return LiveChord(
                    chord: chord, types: entry?.action.types, dead: entry == nil,
                    sameHand: self.alternation.note(chord))
            }
            self.append(live)
        }
    }

    private func append(_ new: [LiveChord]) {
        chordsToday += new.count
        if let last = new.last?.chord.variant, last != variant {
            variant = last
            // Everything built for the old table is now about a layout the keyboard is not
            // in, so it goes rather than being scored against the wrong chords.
            programme = []
            drillIndex = 0
            lineIndex = 0
            if practicing {
                nextDrill()
                rebuildProgramme()
            }
        }
        chords.append(contentsOf: new)
        if chords.count > historyLimit {
            chords.removeFirst(chords.count - historyLimit)
        }
        guard practicing, focused, let drill else { return }
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
