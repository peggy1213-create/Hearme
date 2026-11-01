//! stdin command parsing and event-JSON builders. The command and event shapes are
//! the same as the macOS helper (`Protocol.swift`) and `packages/shared/CAPTURE_HELPER.md`.
//!
//! JSON object keys serialize in sorted order (serde_json's default `Map` is a
//! `BTreeMap`), matching the macOS helper's `sortedKeys`, so event bytes are stable for
//! tests across both helpers.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::frame::Track;

/// A command received from the core on stdin (one JSON object per line).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Command {
    pub cmd: String,
    #[serde(rename = "sourceId", default)]
    pub source_id: Option<String>,
    #[serde(rename = "micDeviceId", default)]
    pub mic_device_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandParseError {
    NotJson,
    MissingCmd,
}

/// Parse one stdin line into a `Command`. Blank lines return `Ok(None)`.
pub fn parse_command(line: &str) -> Result<Option<Command>, CommandParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<Command>(trimmed) {
        Ok(cmd) => Ok(Some(cmd)),
        Err(_) => {
            // Distinguish "valid JSON but no cmd" from "not JSON" for a clearer error.
            if serde_json::from_str::<Value>(trimmed).is_ok() {
                Err(CommandParseError::MissingCmd)
            } else {
                Err(CommandParseError::NotJson)
            }
        }
    }
}

/// Serialize an event value to compact UTF-8 bytes.
pub fn event_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| {
        br#"{"code":"internal","event":"error","message":"event encode failed"}"#.to_vec()
    })
}

fn track_str(track: Track) -> &'static str {
    match track {
        Track::Mic => "mic",
        Track::Meeting => "meeting",
    }
}

/// Convenience builders for the events in CAPTURE_HELPER.md. Each returns the JSON
/// payload bytes; the session wraps them in an event frame.
pub mod event {
    use super::*;

    pub fn sources(sources: Vec<Value>) -> Vec<u8> {
        event_bytes(&json!({ "event": "sources", "sources": sources }))
    }

    pub fn source_started(source_id: &str, bundle_id: &str, name: &str) -> Vec<u8> {
        event_bytes(&json!({
            "event": "source_started",
            "sourceId": source_id,
            "bundleId": bundle_id,
            "name": name,
        }))
    }

    pub fn source_stopped(source_id: &str, bundle_id: &str, name: &str) -> Vec<u8> {
        event_bytes(&json!({
            "event": "source_stopped",
            "sourceId": source_id,
            "bundleId": bundle_id,
            "name": name,
        }))
    }

    pub fn capture_started(source_id: &str, sample_rate: u32, channels: u8) -> Vec<u8> {
        event_bytes(&json!({
            "event": "capture_started",
            "sourceId": source_id,
            "sampleRate": sample_rate,
            "channels": channels,
        }))
    }

    pub fn capture_stopped(source_id: Option<&str>) -> Vec<u8> {
        match source_id {
            Some(id) => event_bytes(&json!({ "event": "capture_stopped", "sourceId": id })),
            None => event_bytes(&json!({ "event": "capture_stopped" })),
        }
    }

    pub fn permission_suspected_missing(track: Track) -> Vec<u8> {
        event_bytes(&json!({
            "event": "permission_suspected_missing",
            "track": track_str(track),
        }))
    }

    pub fn device_changed(kind: &str) -> Vec<u8> {
        event_bytes(&json!({ "event": "device_changed", "kind": kind }))
    }

    /// Windows-specific: microphone privacy setting blocks capture. The message is
    /// short and never contains audio or transcript content.
    pub fn mic_blocked(message: &str) -> Vec<u8> {
        event_bytes(&json!({ "event": "mic_blocked", "message": message }))
    }

    pub fn pong() -> Vec<u8> {
        event_bytes(&json!({ "event": "pong" }))
    }

    pub fn error(code: &str, message: &str) -> Vec<u8> {
        event_bytes(&json!({ "event": "error", "code": code, "message": message }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_sources() {
        let cmd = parse_command(r#"{"cmd":"list_sources"}"#).unwrap().unwrap();
        assert_eq!(cmd.cmd, "list_sources");
        assert_eq!(cmd.source_id, None);
    }

    #[test]
    fn parses_start_with_fields() {
        let cmd = parse_command(r#"{"cmd":"start","sourceId":"zoom","micDeviceId":"mic-1"}"#)
            .unwrap()
            .unwrap();
        assert_eq!(cmd.cmd, "start");
        assert_eq!(cmd.source_id.as_deref(), Some("zoom"));
        assert_eq!(cmd.mic_device_id.as_deref(), Some("mic-1"));
    }

    #[test]
    fn blank_line_is_none() {
        assert_eq!(parse_command("   "), Ok(None));
    }

    #[test]
    fn valid_json_without_cmd_is_missing_cmd() {
        assert_eq!(
            parse_command(r#"{"foo":1}"#),
            Err(CommandParseError::MissingCmd)
        );
    }

    #[test]
    fn garbage_is_not_json() {
        assert_eq!(parse_command("not json"), Err(CommandParseError::NotJson));
    }

    #[test]
    fn event_keys_are_sorted() {
        // Stable, sorted-key output so both helpers emit byte-identical events.
        let bytes = event::source_started("zoom", "zoom.exe", "Zoom");
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(
            s,
            r#"{"bundleId":"zoom.exe","event":"source_started","name":"Zoom","sourceId":"zoom"}"#
        );
    }

    #[test]
    fn capture_stopped_omits_absent_source() {
        assert_eq!(
            String::from_utf8(event::capture_stopped(None)).unwrap(),
            r#"{"event":"capture_stopped"}"#
        );
    }

    #[test]
    fn permission_event_uses_track_name() {
        assert_eq!(
            String::from_utf8(event::permission_suspected_missing(Track::Meeting)).unwrap(),
            r#"{"event":"permission_suspected_missing","track":"meeting"}"#
        );
    }
}
