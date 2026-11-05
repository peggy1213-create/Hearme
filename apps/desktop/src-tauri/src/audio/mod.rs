//! The per-meeting audio pipeline: align the two tracks, cancel echo on the mic,
//! detect voice, resample, and emit (a) continuous 48 kHz audio to the recorder and
//! (b) voiced 16 kHz frames to stream to the gateway.
//!
//! The pipeline is pure DSP and holds no I/O or async: it is driven one decoded
//! helper frame at a time and writes recording audio through a [`Recorder`] trait,
//! so it can be tested against WAV pairs with a mock recorder and no capture helper
//! (Prompt 5 tests). Opus encoding and the WebSocket live in the gateway/recording
//! modules, not here.

pub mod aec;
pub mod resample;
pub mod vad;

use crate::clock::MeetingClock;
use crate::frame::Track;
use aec::{AecProcessor, NlmsAec, NoAec};
use resample::Resampler;
use vad::{EnergyVad, Vad};

/// Recording sample rate (Opus in Ogg on disk).
pub const REC_RATE: u32 = 48_000;
/// Live-streaming sample rate.
pub const STREAM_RATE: u32 = 16_000;
/// Streaming frame length: 20 ms at 16 kHz.
pub const STREAM_FRAME: usize = (STREAM_RATE as usize / 1000) * 20; // 320

/// A voiced frame ready to be Opus-encoded and streamed.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamFrame {
    pub track: Track,
    /// Monotonic per-track sequence number from 0 (PROTOCOL.md).
    pub seq: u64,
    /// Milliseconds from meeting start of the first sample.
    pub timestamp_ms: u64,
    /// 16 kHz mono PCM, `STREAM_FRAME` samples.
    pub samples: Vec<f32>,
}

/// Where continuous 48 kHz recording audio goes. The real implementation
/// (`recording::OggRecorder`) encodes to Opus/Ogg and mixes; tests use a mock.
pub trait Recorder: Send {
    /// Append `samples` (48 kHz mono) for `track`. `start_sample` is that block's
    /// absolute position from meeting start, in 48 kHz samples, used to align the
    /// mixed track; it is meaningful on the first call per track and ignored after.
    fn write(&mut self, track: Track, start_sample: u64, samples: &[f32]);
}

/// Per-track streaming state after AEC: 48k→16k resample, 20 ms framing, VAD, seq.
struct StreamChain {
    down: Resampler,
    vad: Box<dyn Vad>,
    pending: Vec<f32>,
    seq: u64,
    /// Meeting-ms of the next sample waiting in `pending`.
    next_ts_ms: f64,
}

impl StreamChain {
    fn new() -> Self {
        Self {
            down: Resampler::new(REC_RATE, STREAM_RATE),
            vad: Box::new(EnergyVad::new()),
            pending: Vec::new(),
            seq: 0,
            next_ts_ms: 0.0,
        }
    }

    /// Feed a 48 kHz block that starts at `start_ms`; emit any whole voiced 20 ms
    /// frames at 16 kHz.
    fn push(&mut self, track: Track, start_ms: f64, block48: &[f32]) -> Vec<StreamFrame> {
        if self.pending.is_empty() {
            self.next_ts_ms = start_ms;
        }
        let down = self.down.push(block48);
        self.pending.extend_from_slice(&down);

        let mut frames = Vec::new();
        let ms_per_frame = 1000.0 * STREAM_FRAME as f64 / STREAM_RATE as f64; // 20.0
        while self.pending.len() >= STREAM_FRAME {
            let samples: Vec<f32> = self.pending.drain(..STREAM_FRAME).collect();
            let ts = self.next_ts_ms;
            self.next_ts_ms += ms_per_frame;
            if self.vad.is_voiced(&samples) {
                frames.push(StreamFrame {
                    track,
                    seq: self.seq,
                    timestamp_ms: ts.max(0.0) as u64,
                    samples,
                });
                self.seq += 1;
            }
        }
        frames
    }
}

/// Per-track recording state: native → 48 kHz resample and a contiguous write cursor.
struct RecordChain {
    up: Option<Resampler>,
    native_rate: u32,
    origin_sample: Option<u64>,
    written: u64,
}

impl RecordChain {
    fn new() -> Self {
        Self {
            up: None,
            native_rate: 0,
            origin_sample: None,
            written: 0,
        }
    }

    fn resample(&mut self, native_rate: u32, mono: &[f32]) -> Vec<f32> {
        if self.up.is_none() || self.native_rate != native_rate {
            self.up = Some(Resampler::new(native_rate, REC_RATE));
            self.native_rate = native_rate;
        }
        self.up.as_mut().unwrap().push(mono)
    }
}

/// Whole-meeting pipeline.
pub struct Pipeline {
    clock: MeetingClock,
    aec: Box<dyn AecProcessor>,
    aec_enabled: bool,
    mic_rec: RecordChain,
    meeting_rec: RecordChain,
    mic_stream: StreamChain,
    meeting_stream: StreamChain,
    /// Most recent input RMS per track (linear), for the `levels` event.
    mic_level: f32,
    meeting_level: f32,
}

impl Pipeline {
    /// Build a pipeline. `aec` is the echo-cancellation backend (see
    /// `make_aec`); `aec_enabled` starts it on or off (`set_echo_cancellation`).
    pub fn new(aec: Box<dyn AecProcessor>, aec_enabled: bool) -> Self {
        Self {
            clock: MeetingClock::new(),
            aec,
            aec_enabled,
            mic_rec: RecordChain::new(),
            meeting_rec: RecordChain::new(),
            mic_stream: StreamChain::new(),
            meeting_stream: StreamChain::new(),
            mic_level: 0.0,
            meeting_level: 0.0,
        }
    }

    /// Turn echo cancellation on or off mid-meeting.
    pub fn set_echo_cancellation(&mut self, enabled: bool) {
        self.aec_enabled = enabled;
    }

    pub fn aec_name(&self) -> &'static str {
        self.aec.name()
    }

    /// Current input levels (linear RMS 0..1) as (mic, meeting).
    pub fn levels(&self) -> (f32, f32) {
        (self.mic_level, self.meeting_level)
    }

    /// Process one decoded audio frame from the helper. `samples` are interleaved
    /// `channels`-channel `f32` at `rate` Hz. Returns voiced stream frames to send.
    pub fn process_audio(
        &mut self,
        track: Track,
        host_time_ns: u64,
        rate: u32,
        channels: u8,
        samples: &[f32],
        recorder: &mut dyn Recorder,
    ) -> Vec<StreamFrame> {
        let start_ms = self.clock.to_meeting_ms(host_time_ns) as f64;
        let start_sample = (start_ms / 1000.0 * REC_RATE as f64) as u64;
        let mono = downmix(samples, channels);
        let level = rms(&mono);
        match track {
            Track::Mic => self.mic_level = level,
            Track::Meeting => self.meeting_level = level,
        }

        let block48 = match track {
            Track::Mic => self.mic_rec.resample(rate, &mono),
            Track::Meeting => self.meeting_rec.resample(rate, &mono),
        };
        if block48.is_empty() {
            return Vec::new();
        }

        match track {
            Track::Meeting => {
                // Far-end reference for the echo canceller, then record + stream.
                self.aec.process_render(&block48);
                let origin = *self.meeting_rec.origin_sample.get_or_insert(start_sample);
                let cursor = origin + self.meeting_rec.written;
                recorder.write(Track::Meeting, cursor, &block48);
                self.meeting_rec.written += block48.len() as u64;
                self.meeting_stream.push(
                    Track::Meeting,
                    cursor as f64 / REC_RATE as f64 * 1000.0,
                    &block48,
                )
            }
            Track::Mic => {
                let cleaned = if self.aec_enabled {
                    self.aec.process_capture(&block48)
                } else {
                    block48.clone()
                };
                // AEC backends may buffer; fall back to the raw block if it withheld
                // output this tick so the recording never stalls.
                let rec_block = if cleaned.len() == block48.len() {
                    cleaned.clone()
                } else if cleaned.is_empty() {
                    block48.clone()
                } else {
                    cleaned.clone()
                };
                let origin = *self.mic_rec.origin_sample.get_or_insert(start_sample);
                let cursor = origin + self.mic_rec.written;
                recorder.write(Track::Mic, cursor, &rec_block);
                self.mic_rec.written += rec_block.len() as u64;
                self.mic_stream.push(
                    Track::Mic,
                    cursor as f64 / REC_RATE as f64 * 1000.0,
                    &rec_block,
                )
            }
        }
    }

    pub fn started(&self) -> bool {
        self.clock.started()
    }
}

/// Build the default echo-cancellation backend. Uses sonora when the feature is on,
/// otherwise the dependency-free NLMS canceller.
pub fn make_aec() -> Box<dyn AecProcessor> {
    #[cfg(feature = "sonora-aec")]
    {
        Box::new(aec::SonoraAec::new(REC_RATE))
    }
    #[cfg(not(feature = "sonora-aec"))]
    {
        // ~21 ms echo path at 48 kHz.
        Box::new(NlmsAec::new(1024))
    }
}

/// Build the dependency-free NLMS backend explicitly (used by tests).
pub fn nlms_aec() -> Box<dyn AecProcessor> {
    Box::new(NlmsAec::new(1024))
}

/// Build the passthrough backend (echo cancellation off).
pub fn no_aec() -> Box<dyn AecProcessor> {
    Box::new(NoAec)
}

/// Downmix interleaved multi-channel `f32` to mono by averaging.
pub fn downmix(samples: &[f32], channels: u8) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MockRecorder {
        mic: Vec<f32>,
        meeting: Vec<f32>,
        mic_origin: Option<u64>,
        meeting_origin: Option<u64>,
    }

    impl Recorder for MockRecorder {
        fn write(&mut self, track: Track, start_sample: u64, samples: &[f32]) {
            match track {
                Track::Mic => {
                    self.mic_origin.get_or_insert(start_sample);
                    self.mic.extend_from_slice(samples);
                }
                Track::Meeting => {
                    self.meeting_origin.get_or_insert(start_sample);
                    self.meeting.extend_from_slice(samples);
                }
            }
        }
    }

    fn speechy(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32;
                amp * (0.4 * (t * 0.05).sin() + 0.3 * (t * 0.11).sin())
            })
            .collect()
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, 3.0, 2.0, 4.0]; // L,R,L,R
        assert_eq!(downmix(&stereo, 2), vec![2.0, 3.0]);
    }

    #[test]
    fn records_both_tracks_and_emits_monotonic_seq() {
        let mut p = Pipeline::new(no_aec(), false);
        let mut rec = MockRecorder::default();
        // 0.5 s of 48 kHz audio per track, meeting then mic, aligned at t=0.
        let meeting = speechy(24_000, 0.3);
        let mic = speechy(24_000, 0.3);
        let mut frames = Vec::new();
        frames.extend(p.process_audio(Track::Meeting, 0, 48_000, 1, &meeting, &mut rec));
        frames.extend(p.process_audio(Track::Mic, 0, 48_000, 1, &mic, &mut rec));

        // Recording got ~0.5 s per track at 48 kHz.
        assert!((rec.meeting.len() as i64 - 24_000).abs() < 1200);
        assert!((rec.mic.len() as i64 - 24_000).abs() < 1200);

        // Per-track seq starts at 0 and is strictly increasing.
        for track in [Track::Mic, Track::Meeting] {
            let seqs: Vec<u64> = frames
                .iter()
                .filter(|f| f.track == track)
                .map(|f| f.seq)
                .collect();
            assert!(!seqs.is_empty(), "no frames for {track:?}");
            assert_eq!(seqs[0], 0);
            for w in seqs.windows(2) {
                assert_eq!(w[1], w[0] + 1);
            }
        }
    }

    #[test]
    fn stream_frames_are_16k_20ms() {
        let mut p = Pipeline::new(no_aec(), false);
        let mut rec = MockRecorder::default();
        let mic = speechy(48_000, 0.3);
        let frames = p.process_audio(Track::Mic, 0, 48_000, 1, &mic, &mut rec);
        assert!(!frames.is_empty());
        for f in &frames {
            assert_eq!(f.samples.len(), STREAM_FRAME);
        }
        // Timestamps advance by ~20 ms.
        let mic_ts: Vec<u64> = frames.iter().map(|f| f.timestamp_ms).collect();
        assert_eq!(mic_ts[0], 0);
        assert!(mic_ts.last().unwrap() > &0);
    }

    #[test]
    fn silence_is_recorded_but_not_streamed() {
        let mut p = Pipeline::new(no_aec(), false);
        let mut rec = MockRecorder::default();
        let silence = vec![0.0f32; 24_000];
        let frames = p.process_audio(Track::Mic, 0, 48_000, 1, &silence, &mut rec);
        // Recorded (so timestamps line up with audio) …
        assert!(rec.mic.len() > 20_000);
        // … but nothing voiced was streamed.
        assert!(frames.is_empty());
    }

    #[test]
    fn disabling_aec_is_passthrough_for_recording() {
        let mut p = Pipeline::new(nlms_aec(), false); // AEC off
        let mut rec = MockRecorder::default();
        let mic = speechy(4_800, 0.3);
        p.process_audio(Track::Mic, 0, 48_000, 1, &mic, &mut rec);
        // With AEC off and 1:1 rate, the recorded mic equals the input.
        assert_eq!(rec.mic.len(), mic.len());
        assert_eq!(rec.mic, mic);
    }
}
