// swift-tools-version: 5.10
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "steno",
    platforms: [.macOS(.v10_15)],
    products: [
        // Products define the executables and libraries a package produces, making them visible to other packages.
        .library(
            name: "Steno",
            targets: ["Steno"]),
        .executable(
            name: "typey",
            targets: ["typey"]),
    ],
    dependencies: [
        // .package(url: "https://github.com/apple/swift-testing", from: "0.10.0"),
        // .package(url: "https://github.com/pointfreeco/swift-parsing", from: "0.10.0"),
    ],
    targets: [
        // Targets are the basic building blocks of a package, defining a module or a test suite.
        // Targets can depend on other targets in this package and products from dependencies.
        .target(
            name: "Steno"),
        .testTarget(
            name: "StenoTests",
            dependencies: [
                "Steno",
                // "Testing",
                // .product(name: "Testing", package: "swift-testing"),
                // .product(name: "Parsing", package: "swift-parsing"),
            ]),
        .executableTarget(
            name: "typey",
            dependencies: [
                "Steno",
                // .product(name: "Parsing", package: "swift-parsing"),
            ]),
    ]
)
