//! Length-prefixed binary frame encode/decode, shared byte-for-byte with the macOS
//! helper (`native/macos-capture/Sources/macos-capture/Frame.swift`) and
//! `packages/shared/CAPTURE_HELPER.md`. The codec test vectors mirror the Swift ones.

/// Frame kind. See `packages/shared/CAPTURE_HELPER.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    Audio,
    Event,
}

impl FrameType {
    pub fn raw(self) -> u8 {
        match self {
            FrameType::Audio => 1,
            FrameType::Event => 2,
        }
    }

    pub fn from_raw(value: u8) -> Option<FrameType> {
        match value {
            1 => Some(FrameType::Audio),
            2 => Some(FrameType::Event),
            _ => None,
        }
    }
}

/// Which captured track a frame belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Track {
    Mic,
    Meeting,
}

impl Track {
    pub fn raw(self) -> u8 {
        match self {
            Track::Mic => 0,
            Track::Meeting => 1,
        }
    }

    pub fn from_raw(value: u8) -> Option<Track> {
        match value {
            0 => Some(Track::Mic),
            1 => Some(Track::Meeting),
            _ => None,
        }
    }
}

/// Header size in bytes: type 1 + track 1 + hostTimeNs 8 + sampleRate 4 + channels 1
/// + frameCount 4.
pub const FRAME_HEADER_SIZE: usize = 19;

/// Fixed-size part of a frame, excluding the 4-byte length prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_type: FrameType,
    pub track: Track,
    /// Shared host clock, nanoseconds (see `clock`).
    pub host_time_ns: u64,
    pub sample_rate: u32,
    pub channels: u8,
    /// Audio: PCM sample-frame count. Event: JSON payload byte length.
    pub frame_count: u32,
}

/// A decoded frame, used by tests and by the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub header: FrameHeader,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDecodeError {
    Truncated,
    UnknownType(u8),
    UnknownTrack(u8),
    LengthMismatch,
}

/// Encode one frame: `length (u32 LE)` then header then payload. Audio samples in the
/// payload are expected to already be little-endian `f32`.
pub fn encode_frame(header: &FrameHeader, payload: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(FRAME_HEADER_SIZE + payload.len());
    body.push(header.frame_type.raw());
    body.push(header.track.raw());
    body.extend_from_slice(&header.host_time_ns.to_le_bytes());
    body.extend_from_slice(&header.sample_rate.to_le_bytes());
    body.push(header.channels);
    body.extend_from_slice(&header.frame_count.to_le_bytes());
    body.extend_from_slice(payload);

    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Encode a mono/multi-channel audio frame from interleaved `f32` samples.
pub fn encode_audio_frame(
    track: Track,
    host_time_ns: u64,
    sample_rate: u32,
    channels: u8,
    samples: &[f32],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        payload.extend_from_slice(&s.to_le_bytes());
    }
    let frame_count = if channels == 0 {
        0
    } else {
        samples.len() as u32 / channels as u32
    };
    let header = FrameHeader {
        frame_type: FrameType::Audio,
        track,
        host_time_ns,
        sample_rate,
        channels,
        frame_count,
    };
    encode_frame(&header, &payload)
}

/// Encode an event frame. `track` is informational and defaults (by convention) to
/// meeting at the call site.
pub fn encode_event_frame(json: &[u8], host_time_ns: u64, track: Track) -> Vec<u8> {
    let header = FrameHeader {
        frame_type: FrameType::Event,
        track,
        host_time_ns,
        sample_rate: 0,
        channels: 0,
        frame_count: json.len() as u32,
    };
    encode_frame(&header, json)
}

/// Decode a single frame from the front of `data`. Returns the frame and the number of
/// bytes consumed, so a caller can decode a stream.
pub fn decode_frame(data: &[u8]) -> Result<(DecodedFrame, usize), FrameDecodeError> {
    if data.len() < 4 {
        return Err(FrameDecodeError::Truncated);
    }
    let length = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if length < FRAME_HEADER_SIZE {
        return Err(FrameDecodeError::LengthMismatch);
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
    let header = FrameHeader {
        frame_type,
        track,
        host_time_ns,
        sample_rate,
        channels,
        frame_count,
    };
    Ok((DecodedFrame { header, payload }, total))
}

/// Interpret an audio payload as little-endian `f32` samples (used by tests and the
/// `record` subcommand).
pub fn payload_to_f32(payload: &[u8]) -> Vec<f32> {
    payload
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_nineteen() {
        assert_eq!(FRAME_HEADER_SIZE, 19);
    }

    #[test]
    fn audio_frame_round_trip() {
        let samples: [f32; 6] = [0.0, 0.5, -0.5, 1.0, -1.0, 0.25];
        let encoded = encode_audio_frame(Track::Meeting, 123_456_789, 48_000, 1, &samples);

        let (frame, consumed) = decode_frame(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(frame.header.frame_type, FrameType::Audio);
        assert_eq!(frame.header.track, Track::Meeting);
        assert_eq!(frame.header.host_time_ns, 123_456_789);
        assert_eq!(frame.header.sample_rate, 48_000);
        assert_eq!(frame.header.channels, 1);
        assert_eq!(frame.header.frame_count, samples.len() as u32);
        assert_eq!(payload_to_f32(&frame.payload), samples);
    }

    #[test]
    fn event_frame_round_trip() {
        let json = br#"{"event":"pong"}"#;
        let encoded = encode_event_frame(json, 42, Track::Meeting);
        let (frame, consumed) = decode_frame(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(frame.header.frame_type, FrameType::Event);
        assert_eq!(frame.header.sample_rate, 0);
        assert_eq!(frame.header.channels, 0);
        assert_eq!(frame.header.frame_count, json.len() as u32);
        assert_eq!(frame.payload, json);
    }

    #[test]
    fn length_prefix_is_little_endian() {
        let json = b"{}";
        let encoded = encode_event_frame(json, 0, Track::Meeting);
        let length = u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]) as usize;
        assert_eq!(length, FRAME_HEADER_SIZE + json.len());
        assert_eq!(encoded.len(), 4 + length);
    }

    #[test]
    fn decode_truncated_errors() {
        let encoded = encode_event_frame(b"{}", 0, Track::Meeting);
        let truncated = &encoded[..encoded.len() - 1];
        assert!(decode_frame(truncated).is_err());
    }

    #[test]
    fn two_frames_decode_sequentially() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&encode_audio_frame(Track::Mic, 1, 16_000, 1, &[0.1]));
        stream.extend_from_slice(&encode_event_frame(
            br#"{"event":"pong"}"#,
            2,
            Track::Meeting,
        ));

        let (first, consumed1) = decode_frame(&stream).unwrap();
        assert_eq!(first.header.track, Track::Mic);
        let (second, consumed2) = decode_frame(&stream[consumed1..]).unwrap();
        assert_eq!(second.header.frame_type, FrameType::Event);
        assert_eq!(consumed1 + consumed2, stream.len());
    }
}
