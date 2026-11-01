//! The one shared monotonic clock for both tracks, in nanoseconds.
//!
//! Values are arbitrary-zeroed (not wall time); only differences matter. Both the mic
//! and meeting capture paths stamp frames with this clock so the core can align them.
//! Windows: `QueryPerformanceCounter` scaled to ns via `QueryPerformanceFrequency`
//! (see `packages/shared/CAPTURE_HELPER.md`).

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

/// Convert a raw performance counter value to nanoseconds given the counter frequency
/// (ticks per second). Done in 128-bit so a long session can't overflow. Pure, so it
/// is unit-tested without touching the OS.
pub fn counter_to_ns(counter: i64, frequency: i64) -> u64 {
    if frequency <= 0 {
        return 0;
    }
    let counter = counter.max(0) as u128;
    let frequency = frequency as u128;
    ((counter * 1_000_000_000u128) / frequency) as u64
}

/// The counter frequency (ticks per second). Fixed at boot on all supported Windows.
pub fn frequency() -> i64 {
    let mut freq: i64 = 0;
    // QueryPerformanceFrequency never fails on Windows XP or later.
    unsafe {
        let _ = QueryPerformanceFrequency(&mut freq);
    }
    freq
}

/// Current host time in nanoseconds.
pub fn now_ns() -> u64 {
    let mut counter: i64 = 0;
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
    }
    counter_to_ns(counter, frequency())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_conversion_basic() {
        // 10 MHz counter: 1 tick = 100 ns.
        assert_eq!(counter_to_ns(1, 10_000_000), 100);
        assert_eq!(counter_to_ns(10_000_000, 10_000_000), 1_000_000_000);
    }

    #[test]
    fn counter_conversion_no_overflow_on_long_session() {
        // ~2 hours at a 10 MHz counter is far beyond u64 if done naively in ticks*1e9.
        let two_hours_ticks = 10_000_000i64 * 3600 * 2;
        let ns = counter_to_ns(two_hours_ticks, 10_000_000);
        assert_eq!(ns, 7_200 * 1_000_000_000);
    }

    #[test]
    fn zero_frequency_is_safe() {
        assert_eq!(counter_to_ns(123, 0), 0);
    }

    #[test]
    fn live_clock_is_monotonic() {
        let a = now_ns();
        let b = now_ns();
        assert!(b >= a);
    }
}
