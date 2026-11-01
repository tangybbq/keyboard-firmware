import CoreBluetooth
import Foundation

/// A connection to a keyboard's minder service over Bluetooth, the GATT service in jolt's
/// `minder_ble.c`.
///
/// The keyboard is already connected to the Mac, as a keyboard, and does not advertise
/// while it is, so it is looked for among the system's connected peripherals rather than by
/// a scan.  Connecting to it from here only adds this process as a user of that link:
/// macOS's HID goes on using it, and closing this leaves the keyboard connected.
///
/// The service has two characteristics: requests are written to one, without response,
/// and replies come back as notifications on the other.  Each write and each notification
/// is one packet of `PacketFraming`'s framing.
///
/// CoreBluetooth is callbacks on a queue; this is synchronous, like `MinderDevice`, by
/// having the caller's thread wait while the callbacks run on a queue of this object's own.
/// It must therefore not be called from that queue, which nothing outside this file sees.
public final class MinderBLEDevice: NSObject, MinderLink {
    /// The service and its characteristics, as in the minder crate's `ble` module.
    public static let serviceUUID = CBUUID(string: "9FA70800-5FD7-4BE9-B4C7-0E6E6FCF33F3")
    static let requestUUID = CBUUID(string: "9FA70801-5FD7-4BE9-B4C7-0E6E6FCF33F3")
    static let replyUUID = CBUUID(string: "9FA70802-5FD7-4BE9-B4C7-0E6E6FCF33F3")

    /// The keyboard's Bluetooth name, "Jolt Keyboard 1" and the like.
    public private(set) var name = ""

    public enum DeviceError: Error, CustomStringConvertible {
        /// No connected keyboard has the service, or Bluetooth is off.
        case notFound
        /// The user has not let this program use Bluetooth.
        case unauthorized
        /// The keyboard takes writes too short for a packet: the ATT MTU is too small.
        case mtuTooSmall(name: String, writes: Int)
        case disconnected
        case timedOut
        /// A step of opening the service failed.
        case failed(String)

        public var description: String {
            switch self {
            case .notFound: return "no keyboard with the minder service is connected"
            case .unauthorized: return "not allowed to use Bluetooth"
            case .mtuTooSmall(let name, let writes):
                return "\(name) takes writes of \(writes) bytes, and minder's packets are "
                    + "\(PacketFraming.packetSize)"
            case .disconnected: return "the keyboard disconnected"
            case .timedOut: return "timed out waiting for the keyboard"
            case .failed(let why): return why
            }
        }
    }

    private let queue = DispatchQueue(label: "org.davidb.minderkit.ble")
    /// Held for as long as the link: releasing the central manager ends this process's
    /// connection to the keyboard.
    private var central: CBCentralManager!
    /// Signalled by every callback, for the waiting thread to look again.
    private let wake = DispatchSemaphore(value: 0)

    // Everything below belongs to `queue`.

    private var peripheral: CBPeripheral?
    private var request: CBCharacteristic?
    private var reply: CBCharacteristic?
    /// The outcome of the opening step under way, set by its callback.
    private var step: Result<Void, Error>?
    private var decoder = PacketDecoder()
    /// Whole replies, oldest first.
    private var replies: [[UInt8]] = []
    private var gone = false

    /// Open the minder service on the first connected keyboard that has it, or the first
    /// whose name contains `name`.
    public init(name: String? = nil, timeout: TimeInterval = 5.0) throws {
        let deadline = Date().addingTimeInterval(timeout)
        var opened: String?
        super.init()
        central = CBCentralManager(
            delegate: self, queue: queue,
            options: [CBCentralManagerOptionShowPowerAlertKey: false])

        try wait(until: deadline) { self.central.state != .unknown && self.central.state != .resetting }
        switch queue.sync(execute: { central.state }) {
        case .poweredOn: break
        case .unauthorized: throw DeviceError.unauthorized
        default: throw DeviceError.notFound
        }

        // By the minder service alone.  Looking for the HID Service as well would find
        // nothing more: macOS keeps that service to itself and lists no peripheral by it.
        let candidates = queue.sync {
            central.retrieveConnectedPeripherals(withServices: [Self.serviceUUID])
        }
        for candidate in candidates {
            let found = candidate.name ?? ""
            if let name, !found.contains(name) { continue }
            do {
                try open(candidate, deadline: deadline)
                opened = found
                break
            } catch DeviceError.notFound {
                // Listed by the service, but without it once connected.
                queue.sync { central.cancelPeripheralConnection(candidate) }
                continue
            }
        }
        guard let opened else { throw DeviceError.notFound }
        self.name = opened
    }

    deinit {
        // Only this process's use of the link; the keyboard stays connected to the Mac.
        let central = central
        if let peripheral = queue.sync(execute: { self.peripheral }) {
            queue.async { central?.cancelPeripheralConnection(peripheral) }
        }
    }

    /// Connect to a peripheral and set up the service on it.  `DeviceError.notFound` if it
    /// has no minder service.
    private func open(_ candidate: CBPeripheral, deadline: Date) throws {
        // Connected to the system is not connected for this process; this is quick.
        try run(until: deadline) {
            self.peripheral = candidate
            candidate.delegate = self
            self.central.connect(candidate)
        }
        try run(until: deadline) { candidate.discoverServices([Self.serviceUUID]) }
        guard let service = queue.sync(execute: {
            candidate.services?.first { $0.uuid == Self.serviceUUID }
        }) else { throw DeviceError.notFound }

        try run(until: deadline) {
            candidate.discoverCharacteristics([Self.requestUUID, Self.replyUUID], for: service)
        }
        let (request, reply) = queue.sync {
            (service.characteristics?.first { $0.uuid == Self.requestUUID },
             service.characteristics?.first { $0.uuid == Self.replyUUID })
        }
        guard let request, let reply else {
            throw DeviceError.failed("the minder service lacks its characteristics")
        }
        try run(until: deadline) {
            self.request = request
            self.reply = reply
            candidate.setNotifyValue(true, for: reply)
        }

        let writes = queue.sync { candidate.maximumWriteValueLength(for: .withoutResponse) }
        guard writes >= PacketFraming.packetSize else {
            throw DeviceError.mtuTooSmall(name: candidate.name ?? "the keyboard", writes: writes)
        }
    }

    /// Start an opening step on the queue, and wait for its callback to say how it went.
    private func run(until deadline: Date, _ start: @escaping () -> Void) throws {
        queue.sync {
            step = nil
            start()
        }
        try wait(until: deadline) { self.step != nil }
        if case .failure(let error) = queue.sync(execute: { step! }) {
            throw error
        }
    }

    /// Wait for `ready`, looked at on the queue, to come true.
    private func wait(until deadline: Date, _ ready: @escaping () -> Bool) throws {
        while !queue.sync(execute: ready) {
            let left = deadline.timeIntervalSinceNow
            if left <= 0 || wake.wait(timeout: .now() + left) == .timedOut {
                if queue.sync(execute: ready) { return }
                throw DeviceError.timedOut
            }
        }
    }

    /// Send a request, a packet per write.
    ///
    /// Without response, which lets the packets go in successive connection events rather
    /// than a round trip each; CoreBluetooth's flow control says when it has room for the
    /// next.
    public func send(_ request: Request) throws {
        let deadline = Date().addingTimeInterval(5.0)
        for packet in PacketFraming.encode(request.encode()) {
            try wait(until: deadline) {
                self.gone || self.peripheral?.canSendWriteWithoutResponse == true
            }
            try queue.sync {
                guard !gone, let peripheral, let characteristic = self.request else {
                    throw DeviceError.disconnected
                }
                peripheral.writeValue(Data(packet), for: characteristic, type: .withoutResponse)
            }
        }
    }

    /// Read one reply.
    public func receive(timeout: TimeInterval = 20.0) throws -> Reply {
        try wait(until: Date().addingTimeInterval(timeout)) {
            self.gone || !self.replies.isEmpty
        }
        let message: [UInt8] = try queue.sync {
            guard !replies.isEmpty else { throw DeviceError.disconnected }
            return replies.removeFirst()
        }
        return try Reply.decode(message)
    }

    /// Throw away anything already notified: the rest of a reply owed to an earlier client.
    public func drain() {
        repeat {
            queue.sync {
                replies.removeAll()
                decoder = PacketDecoder()
            }
        } while wake.wait(timeout: .now() + 0.05) == .success
    }
}

extension MinderBLEDevice: CBCentralManagerDelegate, CBPeripheralDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state != .poweredOn, peripheral != nil {
            gone = true
        }
        wake.signal()
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        step = .success(())
        wake.signal()
    }

    public func centralManager(
        _ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?
    ) {
        step = .failure(DeviceError.failed("connecting: \(error.map { "\($0)" } ?? "failed")"))
        wake.signal()
    }

    public func centralManager(
        _ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        gone = true
        step = .failure(DeviceError.disconnected)
        wake.signal()
    }

    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        finish(error, doing: "discovering services")
    }

    public func peripheral(
        _ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        finish(error, doing: "discovering characteristics")
    }

    public func peripheral(
        _ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        finish(error, doing: "subscribing to replies")
    }

    public func peripheral(
        _ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard characteristic.uuid == Self.replyUUID, error == nil,
              let value = characteristic.value, !value.isEmpty
        else { return }
        if let message = decoder.add(Array(value)) {
            replies.append(message)
        }
        wake.signal()
    }

    public func peripheralIsReady(toSendWriteWithoutResponse peripheral: CBPeripheral) {
        wake.signal()
    }

    private func finish(_ error: Error?, doing what: String) {
        step = error.map { .failure(DeviceError.failed("\(what): \($0)")) } ?? .success(())
        wake.signal()
    }
}

/// Minder's packet framing, from its days on a HID transport, which the Bluetooth service
/// keeps (`minder/src/encode/hid.rs` and `decode/hid.rs`): a message goes in packets of up
/// to 64 bytes, each starting with its sequence number, the top bit set on the last.
public enum PacketFraming {
    public static let packetSize = 64
    static let payload = packetSize - 1
    static let last: UInt8 = 0x80
    static let sequence: UInt8 = 0x7f

    /// A message's packets.  The last is as long as it needs to be, not padded: ATT keeps
    /// the length, and the keyboard takes a short packet.  An empty message is still one.
    public static func encode(_ message: [UInt8]) -> [[UInt8]] {
        var packets: [[UInt8]] = []
        var start = 0
        repeat {
            let end = min(start + payload, message.count)
            let seq = UInt8(packets.count) & sequence
            packets.append([seq | (end == message.count ? last : 0)] + message[start..<end])
            start = end
        } while start < message.count
        return packets
    }
}

/// Reassembles messages from `PacketFraming` packets.
///
/// As in the Rust decoder, a packet out of sequence throws away the message it was part of,
/// and everything up to the next packet 0.  A packet 0 always starts a new message, where
/// the Rust one would throw that away too.
public struct PacketDecoder {
    private var buffer: [UInt8] = []
    /// The sequence number the next packet must have, or nil while waiting for a packet 0.
    private var expected: UInt8?

    public init() {}

    /// Add a packet, and give back the message if that was its last.
    public mutating func add(_ packet: [UInt8]) -> [UInt8]? {
        guard let head = packet.first else { return nil }
        let seq = head & PacketFraming.sequence
        if seq == 0 {
            buffer = []
        } else if seq != expected {
            buffer = []
            expected = nil
            return nil
        }
        buffer += packet.dropFirst()
        if head & PacketFraming.last != 0 {
            expected = nil
            defer { buffer = [] }
            return buffer
        }
        expected = seq &+ 1
        return nil
    }
}
