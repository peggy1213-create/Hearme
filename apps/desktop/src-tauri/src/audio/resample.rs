//! Mono sample-rate conversion built on `rubato` (`FastFixedIn`, cubic polynomial —
//! low latency, no FFT, good enough for speech STT and recording).
//!
//! The capture helper emits audio at the device's native rate. The core needs two
//! rates downstream: **48 kHz for recording** and **16 kHz for streaming** (Prompt 5
//! / PROTOCOL.md). A `Resampler` here buffers an arbitrary-length input stream into
//! the fixed input chunks `rubato` wants and yields resampled samples as they are
//! produced. A 1:1 ratio is a zero-copy passthrough.

use rubato::{FastFixedIn, PolynomialDegree, Resampler as _};

const CHUNK: usize = 1024;

/// Streaming mono resampler from `in_rate` to `out_rate`.
pub struct Resampler {
    inner: Option<FastFixedIn<f32>>,
    in_buf: Vec<f32>,
    chunk_in: usize,
}

impl Resampler {
    /// Build a resampler. When the rates are equal, the resampler is a passthrough.
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        if in_rate == out_rate || in_rate == 0 {
            return Self {
                inner: None,
                in_buf: Vec::new(),
                chunk_in: 0,
            };
        }
        let ratio = out_rate as f64 / in_rate as f64;
        let inner = FastFixedIn::<f32>::new(ratio, 1.0, PolynomialDegree::Cubic, CHUNK, 1)
            .expect("resampler ratio must be valid");
        let chunk_in = inner.input_frames_next();
        Self {
            inner: Some(inner),
            in_buf: Vec::with_capacity(chunk_in * 2),
            chunk_in,
        }
    }

    /// Feed input samples; returns whatever resampled output is ready now. Any tail
    /// shorter than one chunk stays buffered for the next call.
    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        let Some(inner) = self.inner.as_mut() else {
            return input.to_vec(); // passthrough
        };
        self.in_buf.extend_from_slice(input);
        let mut out = Vec::new();
        while self.in_buf.len() >= self.chunk_in {
            let chunk: Vec<f32> = self.in_buf.drain(..self.chunk_in).collect();
            let produced = inner
                .process(&[chunk.as_slice()], None)
                .expect("resample chunk");
            out.extend_from_slice(&produced[0]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_rates_match() {
        let mut r = Resampler::new(48_000, 48_000);
        let input: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        assert_eq!(r.push(&input), input);
    }

    #[test]
    fn downsample_48k_to_16k_is_about_one_third() {
        let mut r = Resampler::new(48_000, 16_000);
        // One second of a 48 kHz tone.
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let out = r.push(&input);
        let expected = 16_000.0;
        // Within one chunk's worth of buffering latency.
        assert!(
            (out.len() as f32 - expected).abs() < CHUNK as f32,
            "got {} samples, expected ~{expected}",
            out.len()
        );
    }

    #[test]
    fn upsample_44100_to_48000_grows() {
        let mut r = Resampler::new(44_100, 48_000);
        let input: Vec<f32> = (0..44_100).map(|i| (i as f32 * 0.03).sin()).collect();
        let out = r.push(&input);
        assert!(out.len() > 44_000, "got {}", out.len());
    }

    #[test]
    fn streaming_in_small_pushes_matches_one_big_push() {
        let input: Vec<f32> = (0..20_000).map(|i| (i as f32 * 0.02).sin()).collect();
        let mut big = Resampler::new(48_000, 16_000);
        let out_big = big.push(&input);

        let mut small = Resampler::new(48_000, 16_000);
        let mut out_small = Vec::new();
        for chunk in input.chunks(97) {
            out_small.extend(small.push(chunk));
        }
        assert_eq!(out_big, out_small);
    }
}
