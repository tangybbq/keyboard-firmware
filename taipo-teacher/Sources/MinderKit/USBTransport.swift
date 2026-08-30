import Foundation
import IOKit
import IOUSBHost

/// A connection to a keyboard's vendor bulk interface.
///
/// # Two things that cost the spike most of its time
///
/// `IOUSBHostInterface.createMatchingDictionary(...)` puts `idVendor` and friends at the
/// **top level** of the matching dictionary, where `IOServiceGetMatchingServices` matches
/// nothing for this class -- and it fails silently, returning no results rather than an
/// error.  The properties have to be nested under `kIOPropertyMatchKey`.
///
/// IOUSBHost ships no Swift overlay, so the API is reached through its `NS_REFINED_FOR_SWIFT`
/// `__` spellings.  `copyPipe(withAddress:)` is the exception and keeps its plain name.
///
/// See `docs/spikes/swift-usb/` for the standalone program that established both.
public final class MinderDevice {
    private let interface: IOUSBHostInterface
    private let outPipe: IOUSBHostPipe
    private let inPipe: IOUSBHostPipe

    /// USB packet size for these endpoints, and the size at which a message needs a
    /// zero-length packet after it.
    public static let packetSize = 64

    public enum DeviceError: Error, CustomStringConvertible {
        case notFound(serial: String?)
        case noPipes
        case shortWrite(sent: Int, expected: Int)
        case replyTooLarge

        public var description: String {
            switch self {
            case .notFound(let serial):
                return "no keyboard found" + (serial.map { " with serial \($0)" } ?? "")
            case .noPipes: return "the vendor interface has no bulk pipes"
            case .shortWrite(let sent, let expected):
                return "wrote \(sent) of \(expected) bytes"
            case .replyTooLarge: return "reply exceeded the size limit"
            }
        }
    }

    /// Open the first matching keyboard, or the one with this serial.
    public init(serial: String? = nil) throws {
        let matching = IOServiceMatching("IOUSBHostInterface") as NSMutableDictionary
        matching[kIOPropertyMatchKey] = [
            "idVendor": 0xc0de,
            "idProduct": 0xcafe,
            "bInterfaceClass": 0xff,
        ]

        var iterator: io_iterator_t = 0
        guard IOServiceGetMatchingServices(kIOMainPortDefault, matching, &iterator) == KERN_SUCCESS
        else { throw DeviceError.notFound(serial: serial) }
        defer { IOObjectRelease(iterator) }

        var opened: IOUSBHostInterface?
        while case let service = IOIteratorNext(iterator), service != 0 {
            defer { IOObjectRelease(service) }
            // Check the serial before opening: it lives on the parent device node, and
            // asking the registry is far less work than going through IOUSBHost for it.
            if let serial, MinderDevice.serialNumber(of: service) != serial {
                continue
            }
            guard let candidate = try? IOUSBHostInterface(
                __ioService: service, options: [], queue: nil, interestHandler: nil)
            else { continue }
            opened = candidate
            break
        }
        guard let interface = opened else { throw DeviceError.notFound(serial: serial) }
        self.interface = interface

        // Find the bulk pair by probing.  The descriptors would be tidier, but the C
        // descriptor walk from Swift is a lot of unsafe pointer work for two addresses
        // that a probe finds in microseconds.
        var out: IOUSBHostPipe?
        var inp: IOUSBHostPipe?
        for n in 1...15 {
            if out == nil { out = try? interface.copyPipe(withAddress: n) }
            if inp == nil { inp = try? interface.copyPipe(withAddress: 0x80 | n) }
        }
        guard let outPipe = out, let inPipe = inp else { throw DeviceError.noPipes }
        self.outPipe = outPipe
        self.inPipe = inPipe
    }

    /// The serial number of the device an interface belongs to.
    ///
    /// Searched up the registry rather than read from the interface node, because the
    /// property belongs to the device the interface hangs off.
    static func serialNumber(of service: io_service_t) -> String? {
        IORegistryEntrySearchCFProperty(
            service,
            kIOServicePlane,
            "USB Serial Number" as CFString,
            kCFAllocatorDefault,
            IOOptionBits(kIORegistryIterateRecursively | kIORegistryIterateParents)
        ) as? String
    }

    /// Send a request.
    ///
    /// A message whose length is an exact multiple of the packet size is followed by a
    /// zero-length packet.  Without it the device waits for a continuation that never
    /// comes, and then swallows the *next* request as part of the stuck one -- a silently
    /// lost request rather than a visible stall.  The Rust host had this bug; see
    /// `keyminder`'s `send`.
    public func send(_ request: Request) throws {
        let bytes = request.encode()
        let sent = try write(bytes)
        guard sent == bytes.count else {
            throw DeviceError.shortWrite(sent: sent, expected: bytes.count)
        }
        if bytes.count % Self.packetSize == 0 {
            _ = try write([])
        }
    }

    private func write(_ bytes: [UInt8]) throws -> Int {
        let data = NSMutableData(bytes: bytes, length: bytes.count)
        var transferred = 0
        try outPipe.__sendIORequest(
            with: data, bytesTransferred: &transferred, completionTimeout: 2.0)
        return transferred
    }

    /// The largest reply the device sends: `Reply::EventLog` with a full batch of records,
    /// plus its other fields and the CBOR framing.
    static let maxReply = 4400

    /// Read one reply.
    ///
    /// One transfer with a buffer big enough for the whole message, rather than a packet at
    /// a time: a bulk transfer ends at the first short packet, so the controller assembles
    /// it, and each separate request would otherwise cost a USB frame.  Measured against the
    /// mesa1, reading packet by packet took a Hello round trip from 0.4 ms to 3.6 ms.
    public func receive(timeout: TimeInterval = 20.0) throws -> Reply {
        let buffer = NSMutableData(length: Self.maxReply)!
        var got = 0
        try inPipe.__sendIORequest(
            with: buffer, bytesTransferred: &got, completionTimeout: timeout)
        let message = Array(Data(bytes: buffer.bytes, count: got))
        return try Reply.decode(message)
    }

    /// Send and wait for the reply.
    public func call(_ request: Request, timeout: TimeInterval = 20.0) throws -> Reply {
        try send(request)
        return try receive(timeout: timeout)
    }

    /// Throw away anything the device still owes, so a session starts in step.
    ///
    /// A client killed mid-conversation leaves a reply outstanding; without this the first
    /// reply read is that one and everything after is off by one.
    public func drain() {
        while true {
            let buffer = NSMutableData(length: Self.packetSize)!
            var got = 0
            do {
                try inPipe.__sendIORequest(
                    with: buffer, bytesTransferred: &got, completionTimeout: 0.05)
            } catch {
                return
            }
            if got == 0 { return }
        }
    }
}
