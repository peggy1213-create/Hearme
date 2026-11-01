import CoreAudio
import Foundation

/// Tap/aggregate-device hygiene. Private aggregate devices are torn down by the system
/// when the creating process dies, so a crash should not leave anything in Audio MIDI
/// Setup. This sweep is belt-and-braces for any non-private leftovers from an older
/// build or an unclean shutdown.
public enum Cleanup {
    /// Destroy any aggregate devices whose UID carries this helper's prefix.
    @discardableResult
    public static func reapOrphanAggregates() -> Int {
        let devices = HAL.objectIDs(AudioObjectID(kAudioObjectSystemObject),
                                    kAudioHardwarePropertyDevices)
        var destroyed = 0
        for device in devices {
            guard let uid = HAL.string(device, kAudioDevicePropertyDeviceUID),
                  uid.hasPrefix(hearmeDevicePrefix)
            else { continue }
            if AudioHardwareDestroyAggregateDevice(device) == noErr {
                destroyed += 1
            }
        }
        return destroyed
    }
}

/// Installs SIGINT/SIGTERM handlers that run `handler` (tear down capture) then exit.
/// Uses `DispatchSource` so the handler runs on a normal queue rather than in the
/// restricted async-signal context. Retain the returned sources for the process
/// lifetime.
public func installTerminationHandlers(
    queue: DispatchQueue = .main,
    handler: @escaping () -> Void
) -> [DispatchSourceSignal] {
    [SIGINT, SIGTERM].map { sig -> DispatchSourceSignal in
        signal(sig, SIG_IGN)  // disable default handling so the source receives it
        let source = DispatchSource.makeSignalSource(signal: sig, queue: queue)
        source.setEventHandler {
            handler()
            exit(0)
        }
        source.resume()
        return source
    }
}
