import Foundation

/// Detects the "missing permission" symptom: macOS can deliver exact-zero buffers
/// instead of an error when system-audio permission is absent. If a track delivers
/// only zeros for `thresholdSeconds` while its source is known to be playing, fire
/// once. Any non-zero sample resets the guard, so it also re-arms if permission is
/// granted and later revoked.
public final class SilenceGuard {
    public let track: Track
    private let thresholdSeconds: Double
    private let onSuspected: (Track) -> Void

    private var zeroSampleRun = 0
    private var fired = false

    public init(track: Track, thresholdSeconds: Double = 3.0, onSuspected: @escaping (Track) -> Void) {
        self.track = track
        self.thresholdSeconds = thresholdSeconds
        self.onSuspected = onSuspected
    }

    /// Feed one audio buffer. `sourcePlaying` must be true only when the source is
    /// known to be producing audio, so real silence (nobody talking) doesn't trip it.
    public func feed(samples: [Float], sampleRate: UInt32, sourcePlaying: Bool) {
        guard sourcePlaying, sampleRate > 0, !samples.isEmpty else {
            reset()
            return
        }
        if samples.allSatisfy({ $0 == 0 }) {
            zeroSampleRun += samples.count
            let threshold = Int(thresholdSeconds * Double(sampleRate))
            if zeroSampleRun >= threshold, !fired {
                fired = true
                onSuspected(track)
            }
        } else {
            reset()
        }
    }

    public func reset() {
        zeroSampleRun = 0
        fired = false
    }
}
