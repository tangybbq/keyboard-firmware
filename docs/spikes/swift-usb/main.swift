import Foundation
import IOKit
import IOUSBHost

// The Jolt/Mesa vendor interface: class 0xff, one bulk IN and one bulk OUT.
let VID = 0xc0de
let PID = 0xcafe

func hex(_ d: Data) -> String { d.map { String(format: "%02x", $0) }.joined(separator: " ") }

// Request::Hello { version: "2024-11-01a" } exactly as minicbor's derive frames it:
// [1, [null, "2024-11-01a"]] -- the null is field index 0, unused because the
// field is #[n(1)].
let hello: [UInt8] = [0x82, 0x01, 0x82, 0xf6, 0x6b,
                      0x32, 0x30, 0x32, 0x34, 0x2d, 0x31, 0x31, 0x2d, 0x30, 0x31, 0x61]

// NOTE: IOUSBHostInterface.__createMatchingDictionary(...) puts idVendor and
// friends at the TOP LEVEL of the dictionary, and IOServiceGetMatchingServices
// does not match those for this class -- it silently finds nothing.  The
// properties have to be nested under IOPropertyMatch instead.
let matching = IOServiceMatching("IOUSBHostInterface") as NSMutableDictionary
matching[kIOPropertyMatchKey] = [
    "idVendor": VID,
    "idProduct": PID,
    "bInterfaceClass": 0xff,
]

let service = IOServiceGetMatchingService(kIOMainPortDefault, matching as CFDictionary)
guard service != 0 else {
    print("FAIL: no matching IOUSBHostInterface (keyboard unplugged?)")
    exit(1)
}
print("ok: matched IOUSBHostInterface service")

let intf: IOUSBHostInterface
do {
    intf = try IOUSBHostInterface(__ioService: service, options: [], queue: nil, interestHandler: nil)
} catch {
    print("FAIL: could not open the interface: \(error)")
    exit(1)
}
print("ok: opened the interface (claimed it)")

// Find the two bulk pipes by probing the plausible endpoint addresses.
var inPipe: IOUSBHostPipe?
var outPipe: IOUSBHostPipe?
for n in 1...15 {
    if outPipe == nil, let p = try? intf.copyPipe(withAddress: n) { outPipe = p; print("ok: bulk OUT at 0x\(String(n, radix: 16))") }
    if inPipe == nil, let p = try? intf.copyPipe(withAddress: 0x80 | n) { inPipe = p; print("ok: bulk IN  at 0x\(String(0x80 | n, radix: 16))") }
}
guard let outPipe, let inPipe else {
    print("FAIL: could not find both bulk pipes")
    exit(1)
}

// Send Hello.
let out = NSMutableData(bytes: hello, length: hello.count)
var sent = 0
do {
    try outPipe.__sendIORequest(with: out, bytesTransferred: &sent, completionTimeout: 2.0)
} catch {
    print("FAIL: write: \(error)")
    exit(1)
}
print("ok: wrote \(sent) bytes: \(hex(Data(hello)))")

// Read the reply.  One 64-byte bulk packet is enough for Hello.
let inBuf = NSMutableData(length: 64)!
var got = 0
do {
    try inPipe.__sendIORequest(with: inBuf, bytesTransferred: &got, completionTimeout: 2.0)
} catch {
    print("FAIL: read: \(error)")
    exit(1)
}
let reply = Data(bytes: inBuf.bytes, count: got)
print("ok: read \(got) bytes: \(hex(reply))")

// Reply::Hello is [1, [null, version, info]] -- check the framing and pull the
// two strings out, without pretending this is a real CBOR decoder.
guard reply.count > 5, reply[0] == 0x82, reply[1] == 0x01, reply[2] == 0x83, reply[3] == 0xf6 else {
    print("FAIL: not the Reply::Hello framing we expected")
    exit(1)
}
var i = 4
var strings: [String] = []
while i < reply.count, reply[i] & 0xe0 == 0x60 {
    let len = Int(reply[i] & 0x1f); i += 1
    guard i + len <= reply.count else { break }
    strings.append(String(decoding: reply[i..<(i+len)], as: UTF8.self)); i += len
}
print("ok: decoded \(strings)")
// How fast is a round trip?  The trainer's feedback latency is bounded by this.
var times: [Double] = []
for _ in 0..<200 {
    let t0 = DispatchTime.now().uptimeNanoseconds
    let o = NSMutableData(bytes: hello, length: hello.count)
    var s = 0, g = 0
    try! outPipe.__sendIORequest(with: o, bytesTransferred: &s, completionTimeout: 2.0)
    let b = NSMutableData(length: 64)!
    try! inPipe.__sendIORequest(with: b, bytesTransferred: &g, completionTimeout: 2.0)
    times.append(Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6)
}
times.sort()
print(String(format: "round trip over %d calls: min %.2f ms, median %.2f ms, p95 %.2f ms, max %.2f ms",
             times.count, times.first!, times[times.count/2], times[Int(Double(times.count) * 0.95)], times.last!))
print("PASS: Swift completed a minder Hello round trip over the vendor bulk interface")
