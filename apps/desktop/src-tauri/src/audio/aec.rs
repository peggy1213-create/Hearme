//! Acoustic echo cancellation + noise suppression for the mic track.
//!
//! When the user has no headphones the microphone hears the meeting audio coming
//! back out of the speakers, so the same words appear on both tracks. We run the
//! mic (near-end / capture) through an echo canceller with the meeting track
//! (far-end / render) as the reference, then subtract the estimated echo.
//!
//! The backend is swappable behind [`AecProcessor`]:
//! - [`SonoraAec`] — pure-Rust WebRTC AudioProcessing (AEC3 + noise suppression),
//!   the production default (`sonora-aec` feature). Operates on 10 ms frames.
//! - [`NlmsAec`] — a dependency-free normalized-LMS adaptive filter. The fallback,
//!   and the deterministic backend the echo test measures against.
//! - [`NoAec`] — passthrough, for when the user turns echo cancellation off
//!   (`set_echo_cancellation`, required to be switchable for testing).

/// One swappable echo-cancellation backend. Render (far/meeting) and capture
/// (near/mic) are fed as equal-length, time-aligned mono blocks at a fixed rate.
pub trait AecProcessor: Send {
    /// Tell the canceller what was played out of the speakers (the far end).
    fn process_render(&mut self, far: &[f32]);
    /// Echo-cancel a block of microphone samples, returning the cleaned block.
    fn process_capture(&mut self, near: &[f32]) -> Vec<f32>;
    /// Human-readable backend name, for logs and the recording status event.
    fn name(&self) -> &'static str;
}

/// Passthrough: echo cancellation disabled.
pub struct NoAec;

impl AecProcessor for NoAec {
    fn process_render(&mut self, _far: &[f32]) {}
    fn process_capture(&mut self, near: &[f32]) -> Vec<f32> {
        near.to_vec()
    }
    fn name(&self) -> &'static str {
        "off"
    }
}

/// Normalized Least-Mean-Squares adaptive FIR echo canceller.
///
/// Models the echo path as an FIR filter applied to the far-end signal and
/// adaptively subtracts it from the near-end. Dependency-free and deterministic,
/// which makes it both a safe fallback and the backend the echo test asserts on.
pub struct NlmsAec {
    /// Adaptive filter taps (index 0 = newest far sample).
    weights: Vec<f32>,
    /// Sliding window of the most recent far-end samples (index 0 = newest).
    far_hist: Vec<f32>,
    /// Far-end samples pushed by `process_render`, consumed in lockstep by
    /// `process_capture` so each near sample aligns with its own far sample.
    far_queue: std::collections::VecDeque<f32>,
    mu: f32,
    eps: f32,
}

impl NlmsAec {
    /// `taps` is the modeled echo-path length in samples (e.g. 256 ≈ 16 ms at
    /// 16 kHz, enough for the speaker→mic acoustic path on a laptop).
    pub fn new(taps: usize) -> Self {
        Self {
            weights: vec![0.0; taps],
            far_hist: vec![0.0; taps],
            far_queue: std::collections::VecDeque::new(),
            mu: 0.5,
            eps: 1e-6,
        }
    }

    fn slide_in(&mut self, x: f32) {
        // Newest sample at index 0.
        self.far_hist.rotate_right(1);
        self.far_hist[0] = x;
    }
}

impl AecProcessor for NlmsAec {
    fn process_render(&mut self, far: &[f32]) {
        self.far_queue.extend(far.iter().copied());
    }

    fn process_capture(&mut self, near: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(near.len());
        for &d in near {
            // Advance the far window by one sample, aligned with this near sample.
            let x = self.far_queue.pop_front().unwrap_or(0.0);
            self.slide_in(x);

            // Estimated echo = weights · far_hist.
            let mut echo = 0.0f32;
            for (w, xi) in self.weights.iter().zip(self.far_hist.iter()) {
                echo += w * xi;
            }
            let e = d - echo;

            // NLMS weight update.
            let norm: f32 = self.far_hist.iter().map(|xi| xi * xi).sum::<f32>() + self.eps;
            let step = self.mu * e / norm;
            for (w, xi) in self.weights.iter_mut().zip(self.far_hist.iter()) {
                *w += step * xi;
            }
            out.push(e);
        }
        out
    }

    fn name(&self) -> &'static str {
        "nlms"
    }
}

#[cfg(feature = "sonora-aec")]
pub use sonora_backend::SonoraAec;

#[cfg(feature = "sonora-aec")]
mod sonora_backend {
    use super::AecProcessor;
    use sonora::config::{EchoCanceller, NoiseSuppression, NoiseSuppressionLevel};
    use sonora::{AudioProcessing, Config, StreamConfig};

    /// WebRTC AudioProcessing (AEC3 + noise suppression) via the pure-Rust `sonora`
    /// crate. Processes fixed 10 ms frames; the pipeline feeds mono audio at
    /// `rate_hz`, and this buffers into 10 ms frames.
    pub struct SonoraAec {
        apm: AudioProcessing,
        frame: usize,
        far_buf: Vec<f32>,
        near_buf: Vec<f32>,
        out_buf: Vec<f32>,
    }

    impl SonoraAec {
        pub fn new(rate_hz: u32) -> Self {
            let stream = StreamConfig::new(rate_hz, 1);
            let frame = stream.num_frames();
            let config = Config {
                echo_canceller: Some(EchoCanceller::default()),
                noise_suppression: Some(NoiseSuppression {
                    level: NoiseSuppressionLevel::High,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let apm = AudioProcessing::builder()
                .config(config)
                .capture_config(stream)
                .render_config(stream)
                .build();
            Self {
                apm,
                frame,
                far_buf: Vec::new(),
                near_buf: Vec::new(),
                out_buf: Vec::new(),
            }
        }
    }

    impl AecProcessor for SonoraAec {
        fn process_render(&mut self, far: &[f32]) {
            self.far_buf.extend_from_slice(far);
            while self.far_buf.len() >= self.frame {
                let chunk: Vec<f32> = self.far_buf.drain(..self.frame).collect();
                let mut out = vec![0.0f32; self.frame];
                let _ = self
                    .apm
                    .process_render_f32(&[&chunk], &mut [out.as_mut_slice()]);
            }
        }

        fn process_capture(&mut self, near: &[f32]) -> Vec<f32> {
            self.near_buf.extend_from_slice(near);
            while self.near_buf.len() >= self.frame {
                let chunk: Vec<f32> = self.near_buf.drain(..self.frame).collect();
                let mut out = vec![0.0f32; self.frame];
                if self
                    .apm
                    .process_capture_f32(&[&chunk], &mut [out.as_mut_slice()])
                    .is_ok()
                {
                    self.out_buf.extend_from_slice(&out);
                } else {
                    self.out_buf.extend_from_slice(&chunk);
                }
            }
            std::mem::take(&mut self.out_buf)
        }

        fn name(&self) -> &'static str {
            "sonora"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        if x.is_empty() {
            return 0.0;
        }
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Pseudo-speech far-end: a few detuned sinusoids.
    fn pseudo_speech(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32;
                0.4 * (t * 0.05).sin() + 0.3 * (t * 0.11).sin() + 0.2 * (t * 0.23).sin()
            })
            .collect()
    }

    #[test]
    fn nlms_strongly_reduces_a_minus_12db_echo() {
        // The meeting track leaks into the mic at -12 dB (≈0.25 linear) with a small
        // acoustic delay. See Prompt 5: must be strongly reduced after AEC, measured.
        const DELAY: usize = 50;
        const ECHO_GAIN: f32 = 0.251; // -12 dB
        let far = pseudo_speech(48_000);
        let mut mic = vec![0.0f32; far.len()];
        for i in DELAY..far.len() {
            mic[i] = ECHO_GAIN * far[i - DELAY];
        }

        let mut aec = NlmsAec::new(256);
        aec.process_render(&far);
        let out = aec.process_capture(&mic);

        // Measure on the converged tail.
        let tail = out.len() - 8_000..out.len();
        let before = rms(&mic[tail.clone()]);
        let after = rms(&out[tail]);
        let erle_db = 20.0 * (before / after.max(1e-9)).log10();
        assert!(
            erle_db > 10.0,
            "echo reduction only {erle_db:.1} dB (before={before:.5}, after={after:.5})"
        );
    }

    #[test]
    fn nlms_preserves_near_speech_when_no_echo() {
        // No far-end: the canceller must not eat the user's own voice.
        let near = pseudo_speech(16_000);
        let mut aec = NlmsAec::new(256);
        aec.process_render(&vec![0.0; near.len()]);
        let out = aec.process_capture(&near);
        let kept = 20.0 * (rms(&out) / rms(&near)).log10();
        assert!(kept > -1.0, "near speech attenuated by {kept:.2} dB");
    }

    #[test]
    fn no_aec_is_passthrough() {
        let mut aec = NoAec;
        let near = pseudo_speech(100);
        aec.process_render(&near);
        assert_eq!(aec.process_capture(&near), near);
    }
}
