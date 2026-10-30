// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "macos-capture",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "macos-capture",
            path: "Sources/macos-capture"
        ),
        .testTarget(
            name: "macos-captureTests",
            dependencies: ["macos-capture"],
            path: "Tests/macos-captureTests"
        ),
    ]
)
