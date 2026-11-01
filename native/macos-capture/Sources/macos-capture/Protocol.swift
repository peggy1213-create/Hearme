import Foundation

/// A command received from the core on stdin (one JSON object per line).
public struct Command: Decodable, Equatable {
    public let cmd: String
    public let sourceId: String?
    public let micDeviceId: String?
}

public enum CommandParseError: Error, Equatable {
    case notJSON
    case missingCmd
}

/// Parse one stdin line into a `Command`. Blank lines return nil.
public func parseCommand(line: String) throws -> Command? {
    let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.isEmpty { return nil }
    guard let data = trimmed.data(using: .utf8) else { throw CommandParseError.notJSON }
    do {
        return try JSONDecoder().decode(Command.self, from: data)
    } catch {
        // Distinguish "valid JSON but no cmd" from "not JSON" for a clearer error.
        if (try? JSONSerialization.jsonObject(with: data)) != nil {
            throw CommandParseError.missingCmd
        }
        throw CommandParseError.notJSON
    }
}

/// Serialize an event payload (a JSON object) to compact UTF-8 bytes. Keys are sorted
/// for stable output in tests.
public func eventJSON(_ object: [String: Any]) -> Data {
    guard JSONSerialization.isValidJSONObject(object),
          let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    else {
        return Data(#"{"event":"error","code":"internal","message":"event encode failed"}"#.utf8)
    }
    return data
}

/// Convenience builders for the events in CAPTURE_HELPER.md.
public enum Event {
    public static func sources(_ sources: [[String: Any]]) -> Data {
        eventJSON(["event": "sources", "sources": sources])
    }

    public static func sourceStarted(sourceId: String, bundleId: String, name: String) -> Data {
        eventJSON(["event": "source_started", "sourceId": sourceId, "bundleId": bundleId, "name": name])
    }

    public static func sourceStopped(sourceId: String, bundleId: String, name: String) -> Data {
        eventJSON(["event": "source_stopped", "sourceId": sourceId, "bundleId": bundleId, "name": name])
    }

    public static func captureStarted(sourceId: String, sampleRate: UInt32, channels: UInt8) -> Data {
        eventJSON([
            "event": "capture_started",
            "sourceId": sourceId,
            "sampleRate": Int(sampleRate),
            "channels": Int(channels),
        ])
    }

    public static func captureStopped(sourceId: String?) -> Data {
        var obj: [String: Any] = ["event": "capture_stopped"]
        if let sourceId { obj["sourceId"] = sourceId }
        return eventJSON(obj)
    }

    public static func permissionSuspectedMissing(track: Track) -> Data {
        eventJSON(["event": "permission_suspected_missing", "track": track == .mic ? "mic" : "meeting"])
    }

    public static func deviceChanged(kind: String) -> Data {
        eventJSON(["event": "device_changed", "kind": kind])
    }

    public static func pong() -> Data {
        eventJSON(["event": "pong"])
    }

    public static func error(code: String, message: String) -> Data {
        eventJSON(["event": "error", "code": code, "message": message])
    }
}
