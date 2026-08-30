// A thin CLI over MinderKit, so the Swift stack can be exercised without the app.
//
// This is what proves the Swift protocol implementation against a real keyboard rather
// than only against the checked-in vectors.
import Foundation
import MinderKit

let serial = CommandLine.arguments.dropFirst().first

do {
    let device = try MinderDevice(serial: serial)
    device.drain()

    guard case .hello(let hello) = try device.call(.hello(version: MinderVersion.current)) else {
        print("unexpected reply to Hello")
        exit(1)
    }
    print("device:       \(hello.info)")
    print("protocol:     \(hello.version) (host \(MinderVersion.current))")
    print("boot id:      \(hello.bootID.map { String(format: "0x%016llx", $0) } ?? "not reported")")
    print("layout:       \(hello.layoutFingerprint.map { String(format: "0x%016llx", $0) } ?? "not reported")")
    print("capabilities: \(hello.capabilities?.joined(separator: ", ") ?? "not reported")")
    print("  key log:    \(hello.supports(Capability.keyLog))")
    print("  events:     \(hello.supports(Capability.events))")

    // A round trip that exercises the event path, and a batch reply large enough to span
    // packets, which is where the framing actually gets tested.
    let start = Date()
    _ = try device.call(.getEventLog(maxBytes: 3200))
    print(String(format: "GetEventLog round trip: %.2f ms", Date().timeIntervalSince(start) * 1000))

    func bench(_ label: String, _ req: Request) throws {
        var times: [Double] = []
        for _ in 0..<100 {
            let t = Date()
            _ = try device.call(req)
            times.append(Date().timeIntervalSince(t) * 1000)
        }
        times.sort()
        print(String(format: "%-28s median %.2f ms, p95 %.2f ms, min %.2f ms",
                     (label as NSString).utf8String!, times[times.count / 2],
                     times[Int(Double(times.count) * 0.95)], times[0]))
    }
    // Hello's reply spans two packets; GetEventLog with nothing buffered fits in one.
    // If they differ, the cost is per packet rather than per request.
    try bench("Hello (83-byte reply)", .hello(version: MinderVersion.current))
    try bench("GetEventLog (small reply)", .getEventLog(maxBytes: 3200))
    try bench("Hash (37-byte reply)", .hash(offset: 0x1030_0000, size: 16))
    print("PASS")
} catch {
    print("FAILED: \(error)")
    exit(1)
}
