//! Payloads and names for the events the core emits to the UI (Prompt 5 §5).

use serde::Serialize;

use crate::engine::ConnectionStatus;
use crate::frame::Source;
use crate::meeting::TranscriptSegment;

/// Event names (the `event` string the UI listens on).
pub mod names {
    pub const SOURCES: &str = "sources";
    pub const SOURCE_STARTED: &str = "source_started";
    pub const SOURCE_STOPPED: &str = "source_stopped";
    pub const LEVELS: &str = "levels";
    pub const SEGMENT: &str = "segment";
    pub const CONNECTION_STATUS: &str = "connection_status";
    pub const RECORDING_STATUS: &str = "recording_status";
    pub const WARNING: &str = "warning";
}

/// Audio input levels, linear RMS 0..1, ~10/s.
#[derive(Debug, Clone, Serialize)]
pub struct Levels {
    pub mic: f32,
    pub meeting: f32,
}

/// A meeting app starting/stopping (from the helper's `source_started/stopped`).
#[derive(Debug, Clone, Serialize)]
pub struct SourceChange {
    #[serde(rename = "sourceId")]
    pub source_id: String,
    pub name: String,
    #[serde(rename = "bundleId")]
    pub bundle_id: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    Starting,
    Recording,
    Restarting,
    Stopped,
}

/// Recording lifecycle for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct RecordingStatus {
    pub state: RecordingState,
    #[serde(rename = "meetingId", skip_serializing_if = "Option::is_none")]
    pub meeting_id: Option<String>,
    #[serde(rename = "folderPath", skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Echo-cancellation backend in use (`sonora`, `nlms`, or `off`).
    #[serde(rename = "echoCancellation")]
    pub echo_cancellation: String,
}

/// Connection state of the live transcription stream.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatusPayload {
    pub status: ConnectionStatus,
}

/// A non-fatal warning the user should see.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    /// A capture permission is likely missing (macOS system audio, Windows mic).
    Permission,
    /// Low free disk space before/while recording.
    DiskSpace,
    /// Capture started but no audio is arriving.
    NoAudio,
    /// The capture helper crashed (and is being restarted).
    HelperCrashed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    pub kind: WarningKind,
    /// Human-readable; never contains audio or transcript content.
    pub message: String,
}

/// A live transcript segment for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct SegmentPayload {
    pub segment: TranscriptSegment,
}

/// Reply payload for `list_sources`.
#[derive(Debug, Clone, Serialize)]
pub struct SourcesPayload {
    pub sources: Vec<Source>,
}
