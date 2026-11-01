import Foundation

/// A meeting app the helper can detect and tap. `bundleIds` are matched as
/// **prefixes** so helper/renderer processes (e.g. `com.google.Chrome.helper`) are
/// caught along with the main process. Edit `known-apps.json`, not this file, to add
/// apps; the JSON ships as a resource and this constant is the fallback.
public struct KnownApp: Codable, Equatable {
    public let id: String
    public let name: String
    public let kind: String  // "app" | "browser"
    public let bundleIds: [String]

    /// True if `bundleID` belongs to this app (prefix match, case-insensitive on the
    /// reverse-DNS portion as macOS treats bundle IDs case-insensitively).
    public func matches(bundleID: String) -> Bool {
        let lower = bundleID.lowercased()
        return bundleIds.contains { lower.hasPrefix($0.lowercased()) }
    }
}

public struct KnownAppsConfig: Codable, Equatable {
    public let version: Int
    public let apps: [KnownApp]
}

public enum KnownApps {
    /// Load the config, preferring `known-apps.json` next to the binary or in the
    /// SwiftPM resource bundle, falling back to the embedded default.
    public static func load() -> KnownAppsConfig {
        if let url = resourceURL(), let cfg = try? decode(contentsOf: url) {
            return cfg
        }
        return embedded
    }

    public static func decode(contentsOf url: URL) throws -> KnownAppsConfig {
        let data = try Data(contentsOf: url)
        return try JSONDecoder().decode(KnownAppsConfig.self, from: data)
    }

    public static func decode(data: Data) throws -> KnownAppsConfig {
        try JSONDecoder().decode(KnownAppsConfig.self, from: data)
    }

    private static func resourceURL() -> URL? {
        // SwiftPM resource bundle (preferred when built normally).
        if let url = Bundle.module.url(forResource: "known-apps", withExtension: "json") {
            return url
        }
        // A JSON placed next to the executable, for the bare-sidecar case.
        let exe = URL(fileURLWithPath: CommandLine.arguments.first ?? "")
            .deletingLastPathComponent()
            .appendingPathComponent("known-apps.json")
        return FileManager.default.fileExists(atPath: exe.path) ? exe : nil
    }

    private static let embedded = KnownAppsConfig(
        version: 1,
        apps: [
            KnownApp(id: "zoom", name: "Zoom", kind: "app",
                     bundleIds: ["us.zoom.xos", "us.zoom."]),
            KnownApp(id: "teams", name: "Microsoft Teams", kind: "app",
                     bundleIds: ["com.microsoft.teams2", "com.microsoft.teams"]),
            KnownApp(id: "webex", name: "Webex", kind: "app",
                     bundleIds: ["Cisco-Systems.Spark", "com.cisco.webexmeetingsapp", "com.webex."]),
            KnownApp(id: "meet-chrome", name: "Google Chrome", kind: "browser",
                     bundleIds: ["com.google.Chrome"]),
            KnownApp(id: "meet-edge", name: "Microsoft Edge", kind: "browser",
                     bundleIds: ["com.microsoft.edgemac"]),
            KnownApp(id: "meet-safari", name: "Safari", kind: "browser",
                     bundleIds: ["com.apple.Safari"]),
            KnownApp(id: "meet-arc", name: "Arc", kind: "browser",
                     bundleIds: ["company.thebrowser.Browser"]),
        ]
    )

    /// Find the known app a bundle ID belongs to, if any.
    public static func match(bundleID: String, in config: KnownAppsConfig) -> KnownApp? {
        config.apps.first { $0.matches(bundleID: bundleID) }
    }
}
