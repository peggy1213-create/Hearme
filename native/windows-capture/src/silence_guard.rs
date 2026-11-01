//! Detects the "missing permission" symptom: an OS can deliver exact-zero buffers
//! instead of an error when capture permission is absent. If a track delivers only
//! zeros for `threshold_seconds` while its source is known to be playing, fire once.
//! Any non-zero sample resets the guard, so it also re-arms if permission is granted
//! and later revoked. Mirrors the macOS helper's `SilenceGuard`.

use crate::frame::Track;

pub struct SilenceGuard {
    pub track: Track,
    threshold_seconds: f64,
    zero_sample_run: usize,
    fired: bool,
}

impl SilenceGuard {
    pub fn new(track: Track) -> SilenceGuard {
        Self::with_threshold(track, 3.0)
    }

    pub fn with_threshold(track: Track, threshold_seconds: f64) -> SilenceGuard {
        SilenceGuard {
            track,
            threshold_seconds,
            zero_sample_run: 0,
            fired: false,
        }
    }

    /// Feed one audio buffer. `source_playing` must be true only when the source is
    /// known to be producing audio, so real silence (nobody talking) doesn't trip it.
    /// Returns true exactly once, when the zero run first crosses the threshold.
    pub fn feed(&mut self, samples: &[f32], sample_rate: u32, source_playing: bool) -> bool {
        if !source_playing || sample_rate == 0 || samples.is_empty() {
            self.reset();
            return false;
        }
        if samples.iter().all(|&s| s == 0.0) {
            self.zero_sample_run += samples.len();
            let threshold = (self.threshold_seconds * sample_rate as f64) as usize;
            if self.zero_sample_run >= threshold && !self.fired {
                self.fired = true;
                return true;
            }
            false
        } else {
            self.reset();
            false
        }
    }

    pub fn reset(&mut self) {
        self.zero_sample_run = 0;
        self.fired = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_after_threshold_of_zeros() {
        let mut g = SilenceGuard::with_threshold(Track::Meeting, 1.0);
        let rate = 1000;
        let half = vec![0.0f32; 500];
        assert!(!g.feed(&half, rate, true)); // 500 < 1000
        assert!(g.feed(&half, rate, true)); // crosses 1000 -> fires
        assert!(!g.feed(&half, rate, true)); // already fired, no repeat
    }

    #[test]
    fn non_zero_resets() {
        let mut g = SilenceGuard::with_threshold(Track::Meeting, 1.0);
        let rate = 1000;
        assert!(!g.feed(&vec![0.0f32; 999], rate, true));
        assert!(!g.feed(&[0.1], rate, true)); // resets
        assert!(!g.feed(&vec![0.0f32; 999], rate, true)); // only 999 again
    }

    #[test]
    fn not_playing_does_not_trip() {
        let mut g = SilenceGuard::with_threshold(Track::Mic, 1.0);
        assert!(!g.feed(&vec![0.0f32; 5000], 1000, false));
    }

    #[test]
    fn rearms_after_reset_when_permission_revoked() {
        let mut g = SilenceGuard::with_threshold(Track::Meeting, 1.0);
        let rate = 1000;
        g.feed(&vec![0.0f32; 1000], rate, true); // fires
        g.feed(&[0.5], rate, true); // reset by real audio
        assert!(g.feed(&vec![0.0f32; 1000], rate, true)); // fires again
    }
}
