import Foundation

/// Protocol version shared with `packages/shared/CAPTURE_HELPER.md` and the Windows
/// helper. Bump both together when any frame, command or event shape changes.
public let captureHelperProtocolVersion = 1

/// Frame kind. See `packages/shared/CAPTURE_HELPER.md`.
public enum FrameType: UInt8 {
    case audio = 1
    case event = 2
}

/// Which captured track a frame belongs to.
public enum Track: UInt8 {
    case mic = 0
    case meeting = 1
}

/// Fixed-size part of a frame, excluding the 4-byte length prefix. 19 bytes.
public struct FrameHeader: Equatable {
    public var type: FrameType
    public var track: Track
    /// Shared host clock, nanoseconds (see `Clock`).
    public var hostTimeNs: UInt64
    public var sampleRate: UInt32
    public var channels: UInt8
    /// Audio: PCM sample-frame count. Event: JSON payload byte length.
    public var frameCount: UInt32

    public init(
        type: FrameType,
        track: Track,
        hostTimeNs: UInt64,
        sampleRate: UInt32,
        channels: UInt8,
        frameCount: UInt32
    ) {
        self.type = type
        self.track = track
        self.hostTimeNs = hostTimeNs
        self.sampleRate = sampleRate
        self.channels = channels
        self.frameCount = frameCount
    }
}

/// Header size in bytes (type 1 + track 1 + hostTimeNs 8 + sampleRate 4 + channels 1
/// + frameCount 4).
public let frameHeaderSize = 19

/// A decoded frame, used by tests and by the (future) Windows core; the capture path
/// itself only encodes.
public struct DecodedFrame: Equatable {
    public var header: FrameHeader
    public var payload: Data
}

public enum FrameDecodeError: Error, Equatable {
    case truncated
    case unknownType(UInt8)
    case unknownTrack(UInt8)
    case lengthMismatch
}

@inline(__always)
private func appendLE<T: FixedWidthInteger>(_ value: T, to data: inout Data) {
    var little = value.littleEndian
    withUnsafeBytes(of: &little) { data.append(contentsOf: $0) }
}

/// Encode one frame: `length (u32 LE)` then header then payload. Audio samples in the
/// payload are expected to already be little-endian `float32`.
public func encodeFrame(_ header: FrameHeader, payload: Data) -> Data {
    var body = Data(capacity: frameHeaderSize + payload.count)
    body.append(header.type.rawValue)
    body.append(header.track.rawValue)
    appendLE(header.hostTimeNs, to: &body)
    appendLE(header.sampleRate, to: &body)
    body.append(header.channels)
    appendLE(header.frameCount, to: &body)
    body.append(payload)

    var out = Data(capacity: 4 + body.count)
    appendLE(UInt32(truncatingIfNeeded: body.count), to: &out)
    out.append(body)
    return out
}

/// Encode a mono/multi-channel audio frame from interleaved `float32` samples.
public func encodeAudioFrame(
    track: Track,
    hostTimeNs: UInt64,
    sampleRate: UInt32,
    channels: UInt8,
    samples: [Float]
) -> Data {
    let payload = samples.withUnsafeBufferPointer { Data(buffer: $0) }
    let frameCount = channels == 0 ? 0 : UInt32(samples.count) / UInt32(channels)
    let header = FrameHeader(
        type: .audio,
        track: track,
        hostTimeNs: hostTimeNs,
        sampleRate: sampleRate,
        channels: channels,
        frameCount: frameCount
    )
    return encodeFrame(header, payload: payload)
}

/// Encode an event frame. `track` is informational and defaults to meeting.
public func encodeEventFrame(json: Data, hostTimeNs: UInt64, track: Track = .meeting) -> Data {
    let header = FrameHeader(
        type: .event,
        track: track,
        hostTimeNs: hostTimeNs,
        sampleRate: 0,
        channels: 0,
        frameCount: UInt32(truncatingIfNeeded: json.count)
    )
    return encodeFrame(header, payload: json)
}

@inline(__always)
private func readLE<T: FixedWidthInteger>(_ type: T.Type, from bytes: [UInt8], at offset: Int) -> T {
    var value: T = 0
    for i in 0..<MemoryLayout<T>.size {
        value |= T(bytes[offset + i]) << (8 * i)
    }
    return value
}

/// Decode a single frame from the front of `data`. Returns the frame and the number
/// of bytes consumed, so a caller can decode a stream. Used by tests and the core.
public func decodeFrame(from data: Data) throws -> (frame: DecodedFrame, consumed: Int) {
    let bytes = [UInt8](data)
    guard bytes.count >= 4 else { throw FrameDecodeError.truncated }
    let length = Int(readLE(UInt32.self, from: bytes, at: 0))
    guard length >= frameHeaderSize else { throw FrameDecodeError.lengthMismatch }
    let total = 4 + length
    guard bytes.count >= total else { throw FrameDecodeError.truncated }

    var offset = 4
    guard let type = FrameType(rawValue: bytes[offset]) else {
        throw FrameDecodeError.unknownType(bytes[offset])
    }
    offset += 1
    guard let track = Track(rawValue: bytes[offset]) else {
        throw FrameDecodeError.unknownTrack(bytes[offset])
    }
    offset += 1
    let hostTimeNs = readLE(UInt64.self, from: bytes, at: offset); offset += 8
    let sampleRate = readLE(UInt32.self, from: bytes, at: offset); offset += 4
    let channels = bytes[offset]; offset += 1
    let frameCount = readLE(UInt32.self, from: bytes, at: offset); offset += 4

    let payload = Data(bytes[offset..<total])
    let header = FrameHeader(
        type: type,
        track: track,
        hostTimeNs: hostTimeNs,
        sampleRate: sampleRate,
        channels: channels,
        frameCount: frameCount
    )
    return (DecodedFrame(header: header, payload: payload), total)
}
