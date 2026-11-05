//! Voice activity detection, per track, so only speech is streamed to the gateway
//! (voiced audio only, PROTOCOL.md) — this cuts bandwidth and STT cost.
//!
//! Recording is unaffected: the recorder writes everything, including silence, so
//! transcript timestamps always line up with the audio (Prompt 5). VAD gates only
//! the live stream.
//!
//! Backend is swappable behind [`Vad`]:
//! - [`EnergyVad`] — dependency-free adaptive energy gate with hangover; the default
//!   and the backend used in tests.
//! - `SileroVad` — Silero ONNX model via `ort` (`silero-vad` feature). Opt-in; needs
//!   the onnxruntime native library and the bundled model at runtime.

/// One swappable VAD backend. Fed fixed-size frames (20 ms at the streaming rate).
pub trait Vad: Send {
    /// Whether this frame should be treated as speech (and therefore streamed).
    fn is_voiced(&mut self, frame: &[f32]) -> bool;
    fn name(&self) -> &'static str;
}

/// Adaptive energy-based VAD.
///
/// Tracks a slow noise floor that only rises during non-speech, flags a frame as
/// speech when its RMS sits a margin above the floor, and holds the speech state for
/// a short hangover so word-internal pauses are not clipped.
pub struct EnergyVad {
    noise_floor: f32,
    /// Linear margin above the noise floor to count as speech (≈ +9 dB).
    margin: f32,
    /// Frames of speech to hold after energy drops (hangover).
    hangover_frames: u32,
    hangover: u32,
    /// Absolute floor so a dead-silent start still has a sane threshold.
    min_floor: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergyVad {
    pub fn new() -> Self {
        Self {
            noise_floor: 1e-4,
            margin: 2.8,         // ~ +9 dB
            hangover_frames: 12, // ~240 ms at 20 ms frames
            hangover: 0,
            min_floor: 1e-4,
        }
    }

    fn rms(frame: &[f32]) -> f32 {
        if frame.is_empty() {
            return 0.0;
        }
        (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
    }
}

impl Vad for EnergyVad {
    fn is_voiced(&mut self, frame: &[f32]) -> bool {
        let rms = Self::rms(frame);
        let threshold = (self.noise_floor * self.margin).max(self.min_floor);
        let loud = rms > threshold;

        if loud {
            self.hangover = self.hangover_frames;
        } else {
            // Adapt the noise floor only when it is quiet (slow attack, slow release).
            self.noise_floor = 0.95 * self.noise_floor + 0.05 * rms;
            self.hangover = self.hangover.saturating_sub(1);
        }
        loud || self.hangover > 0
    }

    fn name(&self) -> &'static str {
        "energy"
    }
}

#[cfg(feature = "silero-vad")]
pub use silero::SileroVad;

#[cfg(feature = "silero-vad")]
mod silero {
    //! Silero VAD via onnxruntime (`ort`). UNVERIFIED in this repo: it requires the
    //! onnxruntime native library and the bundled `silero_vad.onnx` model at runtime,
    //! neither of which is exercised by the default build or tests. Kept behind the
    //! `silero-vad` feature so the production app can opt in once the model and
    //! runtime are provisioned.
    use super::Vad;
    use ort::session::Session;
    use ort::value::Tensor;
    use std::path::Path;

    pub struct SileroVad {
        session: Session,
        /// Silero's recurrent state tensor (2, 1, 128), carried between frames.
        state: Vec<f32>,
        sample_rate: i64,
        threshold: f32,
    }

    impl SileroVad {
        /// Load the model from `model_path`. The 16 kHz streaming rate is assumed.
        pub fn new(model_path: &Path, sample_rate: u32) -> anyhow::Result<Self> {
            let session = Session::builder()?.commit_from_file(model_path)?;
            Ok(Self {
                session,
                state: vec![0.0; 2 * 1 * 128],
                sample_rate: sample_rate as i64,
                threshold: 0.5,
            })
        }
    }

    impl Vad for SileroVad {
        fn is_voiced(&mut self, frame: &[f32]) -> bool {
            // One inference step: inputs are the audio frame, the recurrent state and
            // the sample rate; outputs are the speech probability and the next state.
            let run = || -> anyhow::Result<f32> {
                let input = Tensor::from_array(([1usize, frame.len()], frame.to_vec()))?;
                let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()))?;
                let sr = Tensor::from_array(([1usize], vec![self.sample_rate]))?;
                let outputs = self.session.run(ort::inputs![
                    "input" => input,
                    "state" => state,
                    "sr" => sr,
                ]?)?;
                let (_, next_state) = outputs["stateN"].try_extract_raw_tensor::<f32>()?;
                self.state = next_state.to_vec();
                let (_, prob) = outputs["output"].try_extract_raw_tensor::<f32>()?;
                Ok(prob.first().copied().unwrap_or(0.0))
            };
            run().map(|p| p >= self.threshold).unwrap_or(true)
        }

        fn name(&self) -> &'static str {
            "silero"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| (i as f32 * 0.3).sin() * amp).collect()
    }

    #[test]
    fn silence_is_not_voiced() {
        let mut vad = EnergyVad::new();
        let silence = vec![0.0f32; 320];
        // Prime the floor, then check.
        for _ in 0..10 {
            vad.is_voiced(&silence);
        }
        assert!(!vad.is_voiced(&silence));
    }

    #[test]
    fn loud_speech_is_voiced() {
        let mut vad = EnergyVad::new();
        let silence = vec![0.0f32; 320];
        for _ in 0..10 {
            vad.is_voiced(&silence);
        }
        assert!(vad.is_voiced(&tone(320, 0.3)));
    }

    #[test]
    fn hangover_bridges_short_gaps() {
        let mut vad = EnergyVad::new();
        let silence = vec![0.0f32; 320];
        for _ in 0..10 {
            vad.is_voiced(&silence);
        }
        assert!(vad.is_voiced(&tone(320, 0.3)));
        // A couple of quiet frames right after speech stay "voiced" (hangover).
        assert!(vad.is_voiced(&silence));
        assert!(vad.is_voiced(&silence));
    }

    #[test]
    fn long_silence_eventually_drops() {
        let mut vad = EnergyVad::new();
        let silence = vec![0.0f32; 320];
        vad.is_voiced(&tone(320, 0.3));
        let mut voiced = true;
        for _ in 0..40 {
            voiced = vad.is_voiced(&silence);
        }
        assert!(!voiced, "VAD should drop to silence after the hangover");
    }
}
