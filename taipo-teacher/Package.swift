// swift-tools-version: 5.9
import PackageDescription

// Taipo Teacher: the Mac half of taipo-teacher.md.
//
// No external dependencies, deliberately.  The one that was considered was a CBOR library,
// and it would not have earned its place: the protocol's framing is minicbor's positional
// field arrays, which a generic CBOR library gives no help with, so the mapping is
// hand-written either way and only the trivial byte-level encoding would have been saved.
// `minder/tests/wire-vectors.txt` is what proves the encoding right, not the provenance of
// the code.
let package = Package(
    name: "TaipoTeacher",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "MinderKit", targets: ["MinderKit"]),
        .library(name: "TaipoKit", targets: ["TaipoKit"]),
    ],
    targets: [
        .target(name: "MinderKit"),
        .target(name: "TaipoKit", resources: [.copy("layouts.json")]),
        .executableTarget(name: "minderctl", dependencies: ["MinderKit"]),
        .executableTarget(name: "secwatch"),
        .executableTarget(
            name: "TaipoTeacherApp",
            dependencies: ["MinderKit", "TaipoKit"]
        ),
        .testTarget(
            name: "TaipoKitTests",
            dependencies: ["TaipoKit"],
            resources: [.copy("golden")]
        ),
        // The app is an executable target, which SwiftPM will link into a test bundle so
        // long as it has no `main.swift`; `App.swift`'s `@main` is what allows this.
        .testTarget(
            name: "AppTests",
            dependencies: ["TaipoTeacherApp", "MinderKit", "TaipoKit"]
        ),
        .testTarget(
            name: "MinderKitTests",
            dependencies: ["MinderKit"],
            resources: [.copy("wire-vectors.txt")]
        ),
    ]
)
