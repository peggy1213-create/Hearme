//! The gateway WebSocket messages, core side. JSON text frames, mirroring the zod
//! schemas in `packages/shared/src/protocol.ts` (PROTOCOL.md). Chinese text in
//! incoming segments is already Traditional (the gateway guarantees it).

use serde::{Deserialize, Serialize};

use crate::meeting::{Platform, TranscriptSegment};

/// Must match `PROTOCOL_VERSION` in `packages/shared`.
pub const PROTOCOL_VERSION: u32 = 2;

// --- client -> server ---

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Start {
        #[serde(rename = "protocolVersion")]
        protocol_version: u32,
        #[serde(rename = "meetingId")]
        meeting_id: String,
        platform: Platform,
        languages: Vec<String>,
        #[serde(rename = "sampleRate")]
        sample_rate: u32,
        codec: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        vocabulary: Option<Vec<String>>,
    },
    Audio {
        track: String,
        seq: u64,
        #[serde(rename = "timestampMs")]
        timestamp_ms: u64,
        /// base64 Opus.
        data: String,
    },
    Stop {
        #[serde(rename = "meetingId")]
        meeting_id: String,
    },
}

// --- server -> client ---

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    Segment {
        segment: TranscriptSegment,
    },
    Ack {
        track: String,
        #[serde(rename = "lastSeq")]
        last_seq: i64,
    },
    Error {
        code: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        fatal: bool,
    },
    Ended {
        #[serde(rename = "meetingId")]
        meeting_id: String,
        #[serde(rename = "segmentCount")]
        segment_count: u64,
    },
}

impl ClientMessage {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("client message serializes")
    }
}

/// Parse a server text frame.
pub fn parse_server(text: &str) -> Result<ServerMessage, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_serializes_to_protocol_shape() {
        let msg = ClientMessage::Start {
            protocol_version: PROTOCOL_VERSION,
            meeting_id: "m1".into(),
            platform: Platform::Meet,
            languages: vec!["en".into(), "zh-TW".into()],
            sample_rate: 16_000,
            codec: "opus".into(),
            vocabulary: None,
        };
        let v: serde_json::Value = serde_json::from_str(&msg.to_json()).unwrap();
        assert_eq!(v["type"], "start");
        assert_eq!(v["protocolVersion"], 2);
        assert_eq!(v["platform"], "meet");
        assert_eq!(v["sampleRate"], 16_000);
        assert_eq!(v["codec"], "opus");
        assert!(v.get("vocabulary").is_none());
    }

    #[test]
    fn audio_serializes_with_camel_case() {
        let msg = ClientMessage::Audio {
            track: "mic".into(),
            seq: 7,
            timestamp_ms: 140,
            data: "AAAA".into(),
        };
        let v: serde_json::Value = serde_json::from_str(&msg.to_json()).unwrap();
        assert_eq!(v["type"], "audio");
        assert_eq!(v["track"], "mic");
        assert_eq!(v["seq"], 7);
        assert_eq!(v["timestampMs"], 140);
        assert_eq!(v["data"], "AAAA");
    }

    #[test]
    fn parses_ack_and_segment_and_ended() {
        match parse_server(r#"{"type":"ack","track":"meeting","lastSeq":-1}"#).unwrap() {
            ServerMessage::Ack { track, last_seq } => {
                assert_eq!(track, "meeting");
                assert_eq!(last_seq, -1);
            }
            _ => panic!("expected ack"),
        }
        match parse_server(r#"{"type":"ended","meetingId":"m1","segmentCount":5}"#).unwrap() {
            ServerMessage::Ended {
                meeting_id,
                segment_count,
            } => {
                assert_eq!(meeting_id, "m1");
                assert_eq!(segment_count, 5);
            }
            _ => panic!("expected ended"),
        }
        let seg = r#"{"type":"segment","segment":{"id":"s1","meetingId":"m1","track":"mic","speaker":"me","startMs":0,"endMs":10,"text":"hi","lang":"en","isFinal":true,"pass":"live"}}"#;
        match parse_server(seg).unwrap() {
            ServerMessage::Segment { segment } => assert_eq!(segment.text, "hi"),
            _ => panic!("expected segment"),
        }
    }

    #[test]
    fn parses_error_with_defaults() {
        match parse_server(r#"{"type":"error","code":"internal"}"#).unwrap() {
            ServerMessage::Error {
                code,
                message,
                fatal,
            } => {
                assert_eq!(code, "internal");
                assert_eq!(message, "");
                assert!(!fatal);
            }
            _ => panic!("expected error"),
        }
    }
}
