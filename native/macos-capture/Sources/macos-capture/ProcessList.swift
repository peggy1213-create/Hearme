import CoreAudio
import Foundation

/// A process known to Core Audio's HAL, used both to detect meeting apps and to feed
/// a `CATapDescription`'s `processes` list.
public struct AudioProcess: Equatable {
    public let objectID: AudioObjectID
    public let pid: pid_t
    public let bundleID: String
    public let isRunningOutput: Bool
}

/// Thin wrappers over the HAL property API. Kept here so the tap/device code reads
/// cleanly.
public enum HAL {
    public static func address(
        _ selector: AudioObjectPropertySelector,
        scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal,
        element: AudioObjectPropertyElement = kAudioObjectPropertyElementMain
    ) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: element)
    }

    /// Read an array of `AudioObjectID` from a property (e.g. the process list).
    public static func objectIDs(
        _ object: AudioObjectID,
        _ selector: AudioObjectPropertySelector
    ) -> [AudioObjectID] {
        var addr = address(selector)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(object, &addr, 0, nil, &size) == noErr, size > 0 else {
            return []
        }
        let count = Int(size) / MemoryLayout<AudioObjectID>.size
        var ids = [AudioObjectID](repeating: 0, count: count)
        let status = ids.withUnsafeMutableBytes {
            AudioObjectGetPropertyData(object, &addr, 0, nil, &size, $0.baseAddress!)
        }
        return status == noErr ? ids : []
    }

    public static func string(
        _ object: AudioObjectID,
        _ selector: AudioObjectPropertySelector
    ) -> String? {
        var addr = address(selector)
        var size = UInt32(MemoryLayout<CFString>.size)
        var value: CFString = "" as CFString
        let status = AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value)
        guard status == noErr else { return nil }
        let string = value as String
        return string.isEmpty ? nil : string
    }

    public static func pid(
        _ object: AudioObjectID,
        _ selector: AudioObjectPropertySelector
    ) -> pid_t? {
        var addr = address(selector)
        var size = UInt32(MemoryLayout<pid_t>.size)
        var value: pid_t = 0
        let status = AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value)
        return status == noErr ? value : nil
    }

    public static func uint32(
        _ object: AudioObjectID,
        _ selector: AudioObjectPropertySelector
    ) -> UInt32? {
        var addr = address(selector)
        var size = UInt32(MemoryLayout<UInt32>.size)
        var value: UInt32 = 0
        let status = AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value)
        return status == noErr ? value : nil
    }
}

public enum ProcessList {
    /// All process objects the HAL currently knows about.
    public static func current() -> [AudioProcess] {
        let ids = HAL.objectIDs(AudioObjectID(kAudioObjectSystemObject),
                                kAudioHardwarePropertyProcessObjectList)
        return ids.compactMap { id in
            guard let bundleID = HAL.string(id, kAudioProcessPropertyBundleID), !bundleID.isEmpty
            else { return nil }
            let pid = HAL.pid(id, kAudioProcessPropertyPID) ?? -1
            let output = (HAL.uint32(id, kAudioProcessPropertyIsRunningOutput) ?? 0) != 0
            return AudioProcess(objectID: id, pid: pid, bundleID: bundleID, isRunningOutput: output)
        }
    }

    /// The process objects that belong to `app`. These `objectID`s go straight into a
    /// `CATapDescription` so the tap covers the app's main process and its audio
    /// helpers (browser renderers, WebView helpers).
    public static func processes(for app: KnownApp, in all: [AudioProcess]) -> [AudioProcess] {
        all.filter { app.matches(bundleID: $0.bundleID) }
    }

    /// Resolve a process' HAL object ID from its BSD pid. Works even for processes
    /// with no bundle ID (e.g. the `afplay` CLI used in the capture test).
    public static func objectID(forPID pid: pid_t) -> AudioObjectID? {
        let ids = HAL.objectIDs(AudioObjectID(kAudioObjectSystemObject),
                                kAudioHardwarePropertyProcessObjectList)
        return ids.first { HAL.pid($0, kAudioProcessPropertyPID) == pid }
    }

    /// Build the `sources` payload: one entry per known app, with running / audio
    /// state resolved from the live process list.
    public static func sources(config: KnownAppsConfig, processes all: [AudioProcess]) -> [[String: Any]] {
        config.apps.map { app in
            let procs = processes(for: app, in: all)
            let running = !procs.isEmpty
            let audio = procs.contains { $0.isRunningOutput }
            let bundleID = procs.first?.bundleID ?? app.bundleIds.first ?? ""
            return [
                "sourceId": app.id,
                "name": app.name,
                "bundleId": bundleID,
                "running": running,
                "producingAudio": audio,
            ]
        }
    }
}
