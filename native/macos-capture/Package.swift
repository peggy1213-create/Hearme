// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "tapcapture",
    // The Core Audio process-tap APIs (AudioHardwareCreateProcessTap, CATapDescription
    // tap/aggregate keys) require macOS 14.2, so the deployment target is 14.2 rather
    // than the coarser `.v14` (14.0), which would make every tap call site an
    // availability error.
    platforms: [.macOS("14.2")],
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
