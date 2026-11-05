//! The transcription-engine abstraction.
//!
//! [`TranscriptionEngine`] is where voiced audio goes to become transcript
//! segments. The only implementation today is the cloud `GatewayEngine`
//! (`gateway::GatewayEngine`), which streams to the Hearme gateway. The trait exists
//! so an on-device engine (whisper.cpp) can be added later for local-only mode
//! (plan.md, v1.1) without touching the pipeline or the UI layer — that engine is
//! **not** built now.

use crate::audio::StreamFrame;
use crate::meeting::{Platform, TranscriptSegment};

/// Connection state reported to the UI (`connection_status` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connecting,
    Connected,
    /// Disconnected; recording continues and the engine is retrying with backoff.
    Reconnecting,
    /// Offline past the live-buffering cutoff; the final pass will transcribe from
    /// the recording instead (Prompt 5).
    Offline,
    Stopped,
}

/// Things the engine emits back to the driver.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Connection(ConnectionStatus),
    /// A live transcript segment (Traditional-Chinese-safe; the gateway guarantees).
    Segment(TranscriptSegment),
    /// The session ended cleanly; `segment_count` is what the gateway reported.
    Ended {
        segment_count: u64,
    },
    /// A gateway error. `fatal` means the session is over.
    Error {
        code: String,
        message: String,
        fatal: bool,
    },
}

/// Configuration for a live session.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// WebSocket base URL, e.g. `ws://127.0.0.1:8787/v1/live`.
    pub gateway_url: String,
    pub meeting_id: String,
    pub platform: Platform,
    /// STT language bias, e.g. `["en","zh-TW"]`.
    pub languages: Vec<String>,
    pub vocabulary: Option<Vec<String>>,
    /// Directory for the on-disk outbox (inside the meeting folder).
    pub outbox_dir: std::path::PathBuf,
    /// Gateway auth token (from the OS keychain). Sent as a Bearer header, never in
    /// the URL (privacy).
    pub token: Option<String>,
    /// Stop buffering for the live stream after this long offline (default 30 min).
    pub offline_cutoff: std::time::Duration,
}

impl EngineConfig {
    pub fn new(gateway_url: String, meeting_id: String, platform: Platform) -> Self {
        Self {
            gateway_url,
            meeting_id,
            platform,
            languages: vec!["en".into(), "zh-TW".into()],
            vocabulary: None,
            outbox_dir: std::env::temp_dir(),
            token: None,
            offline_cutoff: std::time::Duration::from_secs(30 * 60),
        }
    }
}

/// Where voiced audio goes to be transcribed. Implementations run their own
/// background work; `submit`/`finish` are cheap and non-blocking so they can be
/// called from the capture thread.
pub trait TranscriptionEngine: Send + Sync {
    /// Queue one voiced 16 kHz mono frame for transcription.
    fn submit(&self, frame: StreamFrame);
    /// Signal end of meeting; the engine flushes and emits `Ended`.
    fn finish(&self);
    fn name(&self) -> &'static str;
}

/// Opus encoder for the live stream: 16 kHz mono, 20 ms frames.
pub struct StreamEncoder {
    enc: audiopus::coder::Encoder,
    out: Vec<u8>,
}

impl StreamEncoder {
    pub fn new() -> anyhow::Result<Self> {
        use audiopus::{coder::Encoder, Application, Channels, SampleRate};
        Ok(Self {
            enc: Encoder::new(SampleRate::Hz16000, Channels::Mono, Application::Voip)?,
            out: vec![0u8; 4000],
        })
    }

    /// Encode one 20 ms frame (`STREAM_FRAME` samples) to an Opus packet.
    pub fn encode(&mut self, samples: &[f32]) -> anyhow::Result<Vec<u8>> {
        let n = self.enc.encode_float(samples, &mut self.out)?;
        Ok(self.out[..n].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::STREAM_FRAME;

    #[test]
    fn encodes_a_20ms_frame_to_opus() {
        let mut enc = StreamEncoder::new().unwrap();
        let frame: Vec<f32> = (0..STREAM_FRAME)
            .map(|i| (i as f32 * 0.1).sin() * 0.3)
            .collect();
        let packet = enc.encode(&frame).unwrap();
        assert!(!packet.is_empty());
        assert!(packet.len() < 4000);
    }
}
