//! Capture-helper wire protocol, core side.
//!
//! The native helper (`native/macos-capture`, `native/windows-capture`) writes
//! length-prefixed binary frames to its stdout and reads JSON command lines on its
//! stdin. This module is the byte-for-byte counterpart of the helpers' `frame.rs` /
//! `Frame.swift` and `protocol.rs` / `Protocol.swift`, and of
//! `packages/shared/CAPTURE_HELPER.md` (protocol version 1).
//!
//! The core only ever *decodes* frames and *encodes* commands, so this file carries
//! the decode path plus a small typed view of the helper events.

use serde::Deserialize;

/// Must match `CAPTURE_HELPER_PROTOCOL_VERSION` in the helpers.
pub const CAPTURE_HELPER_PROTOCOL_VERSION: u32 = 1;

/// Header size in bytes: type 1 + track 1 + hostTimeNs 8 + sampleRate 4 + channels 1
/// + frameCount 4.
pub const FRAME_HEADER_SIZE: usize = 19;

/// `type` byte values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Audio,
    Event,
}

impl FrameType {
    fn from_raw(value: u8) -> Option<FrameType> {
        match value {
            1 => Some(FrameType::Audio),
            2 => Some(FrameType::Event),
            _ => None,
        }
    }
}

/// `track` byte values. Ignored for event frames (sent as `0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Track {
    Mic,
    Meeting,
}

impl Track {
    fn from_raw(value: u8) -> Option<Track> {
        match value {
            0 => Some(Track::Mic),
            1 => Some(Track::Meeting),
            _ => None,
        }
    }

    /// Lowercase name used in the gateway protocol and meeting JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Track::Mic => "mic",
            Track::Meeting => "meeting",
        }
    }
}

/// The fixed-size part of a frame, excluding the 4-byte length prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_type: FrameType,
    pub track: Track,
    /// Shared monotonic host clock, nanoseconds (see CAPTURE_HELPER.md).
    pub host_time_ns: u64,
    pub sample_rate: u32,
    pub channels: u8,
    /// Audio: PCM sample-frame count. Event: JSON payload byte length.
    pub frame_count: u32,
}

/// A fully decoded frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedFrame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecodeError {
    /// Not enough bytes yet for a whole frame; feed more and retry.
    Truncated,
    UnknownType(u8),
    UnknownTrack(u8),
    LengthTooSmall,
}

/// Decode a single frame from the front of `data`. Returns the frame and the number
/// of bytes consumed so a caller can decode a stream. `Truncated` means "need more
/// bytes", not a hard error.
pub fn decode_frame(data: &[u8]) -> Result<(DecodedFrame, usize), FrameDecodeError> {
    if data.len() < 4 {
        return Err(FrameDecodeError::Truncated);
    }
    let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if length < FRAME_HEADER_SIZE {
        return Err(FrameDecodeError::LengthTooSmall);
    }
    let total = 4 + length;
    if data.len() < total {
        return Err(FrameDecodeError::Truncated);
    }

    let mut offset = 4;
    let frame_type =
        FrameType::from_raw(data[offset]).ok_or(FrameDecodeError::UnknownType(data[offset]))?;
    offset += 1;
    let track =
        Track::from_raw(data[offset]).ok_or(FrameDecodeError::UnknownTrack(data[offset]))?;
    offset += 1;
    let host_time_ns = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
    offset += 8;
    let sample_rate = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    offset += 4;
    let channels = data[offset];
    offset += 1;
    let frame_count = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    offset += 4;

    let payload = data[offset..total].to_vec();
    Ok((
        DecodedFrame {
            header: FrameHeader {
                frame_type,
                track,
                host_time_ns,
                sample_rate,
                channels,
                frame_count,
            },
            payload,
        },
        total,
    ))
}

/// Interpret an audio payload as little-endian `f32` samples (interleaved).
pub fn payload_to_f32(payload: &[u8]) -> Vec<f32> {
    payload
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// A streaming decoder: feed bytes, pull out whole frames. The helper's stdout is a
/// byte stream with no guaranteed chunk boundaries, so the core buffers partial
/// frames here.
#[derive(Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append bytes read from the helper's stdout.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pull the next complete frame, if one is buffered.
    pub fn next_frame(&mut self) -> Option<DecodedFrame> {
        match decode_frame(&self.buf) {
            Ok((frame, consumed)) => {
                self.buf.drain(..consumed);
                Some(frame)
            }
            Err(FrameDecodeError::Truncated) => None,
            Err(_) => {
                // A corrupt length/type should not wedge the stream forever. Drop one
                // byte and resync on the next length prefix.
                if !self.buf.is_empty() {
                    self.buf.drain(..1);
                }
                None
            }
        }
    }
}

/// Events the helper emits (payload of an event frame). Unknown events decode to
/// `Other` so a newer helper never crashes the core.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HelperEvent {
    Sources {
        sources: Vec<Source>,
    },
    SourceStarted {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "bundleId", default)]
        bundle_id: String,
        #[serde(default)]
        name: String,
    },
    SourceStopped {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "bundleId", default)]
        bundle_id: String,
        #[serde(default)]
        name: String,
    },
    CaptureStarted {
        #[serde(rename = "sourceId")]
        source_id: String,
        #[serde(rename = "sampleRate")]
        sample_rate: u32,
        channels: u8,
    },
    CaptureStopped {
        #[serde(rename = "sourceId", default)]
        source_id: Option<String>,
    },
    PermissionSuspectedMissing {
        track: String,
    },
    DeviceChanged {
        kind: String,
    },
    MicBlocked {
        #[serde(default)]
        message: String,
    },
    Pong,
    Error {
        code: String,
        #[serde(default)]
        message: String,
    },
    #[serde(other)]
    Other,
}

/// A detectable meeting app, as reported by the `sources` event.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct Source {
    #[serde(rename = "sourceId")]
    pub source_id: String,
    pub name: String,
    #[serde(rename = "bundleId", default)]
    pub bundle_id: String,
    #[serde(default)]
    pub running: bool,
    #[serde(rename = "producingAudio", default)]
    pub producing_audio: bool,
}

/// Parse an event-frame payload into a typed event.
pub fn parse_event(payload: &[u8]) -> Result<HelperEvent, serde_json::Error> {
    serde_json::from_slice(payload)
}

/// Build a command line (one JSON object, newline-terminated) for the helper's stdin.
pub fn command_line(cmd: &str, source_id: Option<&str>, mic_device_id: Option<&str>) -> String {
    let mut obj = serde_json::Map::new();
    obj.insert("cmd".into(), serde_json::Value::String(cmd.into()));
    if let Some(s) = source_id {
        obj.insert("sourceId".into(), serde_json::Value::String(s.into()));
    }
    if let Some(m) = mic_device_id {
        obj.insert("micDeviceId".into(), serde_json::Value::String(m.into()));
    }
    let mut line = serde_json::to_string(&serde_json::Value::Object(obj)).unwrap();
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a frame the same way the helpers do, for round-trip tests.
    fn encode(header: FrameHeader, payload: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.push(match header.frame_type {
            FrameType::Audio => 1u8,
            FrameType::Event => 2u8,
        });
        body.push(match header.track {
            Track::Mic => 0u8,
            Track::Meeting => 1u8,
        });
        body.extend_from_slice(&header.host_time_ns.to_le_bytes());
        body.extend_from_slice(&header.sample_rate.to_le_bytes());
        body.push(header.channels);
        body.extend_from_slice(&header.frame_count.to_le_bytes());
        body.extend_from_slice(payload);
        let mut out = (body.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    fn audio_frame(track: Track, host_time_ns: u64, rate: u32, ch: u8, samples: &[f32]) -> Vec<u8> {
        let mut payload = Vec::new();
        for s in samples {
            payload.extend_from_slice(&s.to_le_bytes());
        }
        encode(
            FrameHeader {
                frame_type: FrameType::Audio,
                track,
                host_time_ns,
                sample_rate: rate,
                channels: ch,
                frame_count: samples.len() as u32 / ch.max(1) as u32,
            },
            &payload,
        )
    }

    #[test]
    fn header_size_matches_spec() {
        assert_eq!(FRAME_HEADER_SIZE, 19);
    }

    #[test]
    fn decodes_audio_frame() {
        let samples = [0.0f32, 0.5, -0.5, 1.0];
        let bytes = audio_frame(Track::Meeting, 123, 48_000, 1, &samples);
        let (frame, consumed) = decode_frame(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(frame.header.frame_type, FrameType::Audio);
        assert_eq!(frame.header.track, Track::Meeting);
        assert_eq!(frame.header.host_time_ns, 123);
        assert_eq!(frame.header.sample_rate, 48_000);
        assert_eq!(payload_to_f32(&frame.payload), samples);
    }

    #[test]
    fn reader_handles_split_and_joined_frames() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&audio_frame(Track::Mic, 1, 16_000, 1, &[0.1, 0.2]));
        let event = br#"{"event":"pong"}"#;
        stream.extend_from_slice(&encode(
            FrameHeader {
                frame_type: FrameType::Event,
                track: Track::Mic,
                host_time_ns: 2,
                sample_rate: 0,
                channels: 0,
                frame_count: event.len() as u32,
            },
            event,
        ));

        // Feed one byte at a time; frames must still come out whole and in order.
        let mut reader = FrameReader::new();
        let mut frames = Vec::new();
        for b in &stream {
            reader.push(&[*b]);
            while let Some(f) = reader.next_frame() {
                frames.push(f);
            }
        }
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].header.track, Track::Mic);
        assert_eq!(frames[1].header.frame_type, FrameType::Event);
    }

    #[test]
    fn parses_known_events() {
        let sources = br#"{"event":"sources","sources":[{"sourceId":"zoom","name":"Zoom","bundleId":"us.zoom.xos","running":true,"producingAudio":false}]}"#;
        match parse_event(sources).unwrap() {
            HelperEvent::Sources { sources } => {
                assert_eq!(sources.len(), 1);
                assert_eq!(sources[0].source_id, "zoom");
                assert!(sources[0].running);
            }
            other => panic!("expected sources, got {other:?}"),
        }

        let perm = br#"{"event":"permission_suspected_missing","track":"meeting"}"#;
        assert_eq!(
            parse_event(perm).unwrap(),
            HelperEvent::PermissionSuspectedMissing {
                track: "meeting".into()
            }
        );
    }

    #[test]
    fn unknown_event_is_other_not_error() {
        let unknown = br#"{"event":"brand_new_thing","foo":1}"#;
        assert_eq!(parse_event(unknown).unwrap(), HelperEvent::Other);
    }

    #[test]
    fn command_line_is_json_newline() {
        let line = command_line("start", Some("zoom"), Some("mic-1"));
        assert!(line.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["cmd"], "start");
        assert_eq!(v["sourceId"], "zoom");
        assert_eq!(v["micDeviceId"], "mic-1");

        let list = command_line("list_sources", None, None);
        let v: serde_json::Value = serde_json::from_str(list.trim()).unwrap();
        assert_eq!(v["cmd"], "list_sources");
        assert!(v.get("sourceId").is_none());
    }
}
