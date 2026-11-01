import Foundation

/// The one shared monotonic clock for both tracks, in nanoseconds.
///
/// Values are arbitrary-zeroed (not wall time); only differences matter. Both the mic
/// and meeting capture paths stamp frames with this clock so the core can align them.
/// See `packages/shared/CAPTURE_HELPER.md`.
public enum Clock {
    private static let timebase: mach_timebase_info_data_t = {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        return info
    }()

    /// Current host time in nanoseconds.
    public static func nowNs() -> UInt64 {
        hostTimeToNs(mach_absolute_time())
    }

    /// Convert a raw `mach_absolute_time` value (e.g. `AudioTimeStamp.mHostTime` or
    /// `AVAudioTime.hostTime`) to nanoseconds using the Mach timebase.
    public static func hostTimeToNs(_ hostTime: UInt64) -> UInt64 {
        // nanos = hostTime * numer / denom, done in 128-bit-safe steps to avoid
        // overflow on long sessions.
        let numer = UInt64(timebase.numer)
        let denom = UInt64(timebase.denom)
        if numer == denom { return hostTime }
        let high = hostTime / denom
        let low = hostTime % denom
        return high * numer + (low * numer) / denom
    }
}
