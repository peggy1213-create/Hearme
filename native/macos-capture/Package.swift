// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "tapcapture",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "tapcapture",
            path: "Sources/macos-capture",
            resources: [
                .process("known-apps.json"),
            ]
        ),
        .testTarget(
            name: "tapcaptureTests",
            dependencies: ["tapcapture"],
            path: "Tests/macos-captureTests"
        ),
    ]
)
