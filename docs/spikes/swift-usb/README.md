# Swift USB spike

Answers the question `taipo-teacher.md` phase 1d puts first: **can a Swift app talk to
the keyboard's vendor bulk interface directly, or does the Mac app need a Rust core
underneath it after all?**

It can.  This program matches the vendor-specific interface, claims it, and completes a
minder `Hello` round trip, then times 200 more.

```
swiftc -O main.swift -o spike && ./spike
```

Needs the keyboard plugged in.  No root, no entitlement, no code signing — it was run as
an ordinary unsigned binary.  Claiming the vendor interface does not disturb typing; the
HID interfaces are separate and stay bound to their own drivers.

## What it found

- **`IOUSBHostInterface.createMatchingDictionary(...)` does not work with
  `IOServiceGetMatchingServices`.**  It puts `idVendor`/`idProduct`/`bInterfaceClass` at
  the top level of the matching dictionary, where they match nothing for this class — and
  the failure is silent, just zero results.  The properties have to be nested under
  `kIOPropertyMatchKey`.  This cost most of the time the spike took; see the comment in
  `main.swift`.
- **The IOUSBHost API reaches Swift only through its `NS_REFINED_FOR_SWIFT` spellings**,
  because there is no Swift overlay: `__createMatchingDictionary(withVendorID:...)`,
  `IOUSBHostInterface(__ioService:options:queue:interestHandler:)`,
  `__sendIORequest(with:bytesTransferred:completionTimeout:)`.  `copyPipe(withAddress:)`
  is *not* refined and keeps its plain name.  Expect to find these by compiler error.
- **The endpoints are 0x02 (OUT) and 0x82 (IN)** on the mesa1, discovered by probing.  A
  real client should walk the descriptors rather than probe, but the addresses are stable
  and probing is what kept this spike short.
- **Round trip: median 0.37 ms, p95 0.54 ms, max 0.59 ms** over 200 calls to an otherwise
  idle device.

## What it deliberately does not do

No CBOR library: the `Hello` request is the hard-coded bytes minicbor's derive produces,
and the reply is checked by framing rather than decoded.  That isolates the USB question
from the encoding question.  Those bytes are the first entry of the golden vector set 1d
calls for:

```
Request::Hello { version: "2024-11-01a" }
  82 01 82 f6 6b 32 30 32 34 2d 31 31 2d 30 31 61
Reply::Hello { version: "2024-11-01a", info: "mesa1-pnbgmlibdbijkcad" }
  82 01 83 f6 6b ...  76 ...
```

Note the `f6` (null) in both: minicbor frames a variant as `[index, [fields...]]` where
the inner array is positional and unused field numbers are filled with null.  `version`
is `#[n(1)]`, so position 0 is null.  A Swift encoder emitting `[1, ["2024-11-01a"]]` is
valid CBOR and will be rejected by the device.

`minder/tests/wire-vectors.txt` is now the full set, generated from the crate and checked
in, and it supersedes the two examples above as the thing to test a Swift decoder against.
`Reply::Hello` has since grown `boot_id`, `layout_fingerprint` and `capabilities`, so the
inner array may be 3 long (firmware predating them) or 6; this program no longer checks
its length, which is what a real decoder should do too.
