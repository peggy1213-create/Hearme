import XCTest
@testable import tapcapture

final class FrameTests: XCTestCase {
    func testHeaderSizeIsNineteen() {
        XCTAssertEqual(frameHeaderSize, 19)
    }

    func testAudioFrameRoundTrip() throws {
        let samples: [Float] = [0.0, 0.5, -0.5, 1.0, -1.0, 0.25]
        let encoded = encodeAudioFrame(
            track: .meeting, hostTimeNs: 123_456_789, sampleRate: 48_000, channels: 1, samples: samples)

        let (frame, consumed) = try decodeFrame(from: encoded)
        XCTAssertEqual(consumed, encoded.count)
        XCTAssertEqual(frame.header.type, .audio)
        XCTAssertEqual(frame.header.track, .meeting)
        XCTAssertEqual(frame.header.hostTimeNs, 123_456_789)
        XCTAssertEqual(frame.header.sampleRate, 48_000)
        XCTAssertEqual(frame.header.channels, 1)
        XCTAssertEqual(frame.header.frameCount, UInt32(samples.count))

        let decodedSamples = frame.payload.withUnsafeBytes { raw -> [Float] in
            Array(raw.bindMemory(to: Float.self))
        }
        XCTAssertEqual(decodedSamples, samples)
    }

    func testEventFrameRoundTrip() throws {
        let json = Event.pong()
        let encoded = encodeEventFrame(json: json, hostTimeNs: 42)
        let (frame, consumed) = try decodeFrame(from: encoded)
        XCTAssertEqual(consumed, encoded.count)
        XCTAssertEqual(frame.header.type, .event)
        XCTAssertEqual(frame.header.sampleRate, 0)
        XCTAssertEqual(frame.header.channels, 0)
        XCTAssertEqual(frame.header.frameCount, UInt32(json.count))
        XCTAssertEqual(frame.payload, json)
    }

    func testLengthPrefixIsLittleEndian() {
        // A tiny event: the first four bytes are the LE length of header + payload.
        let json = Data("{}".utf8)
        let encoded = encodeEventFrame(json: json, hostTimeNs: 0)
        let bytes = [UInt8](encoded)
        let length = UInt32(bytes[0]) | (UInt32(bytes[1]) << 8) | (UInt32(bytes[2]) << 16) | (UInt32(bytes[3]) << 24)
        XCTAssertEqual(Int(length), frameHeaderSize + json.count)
        XCTAssertEqual(encoded.count, 4 + Int(length))
    }

    func testDecodeTruncatedThrows() {
        let encoded = encodeEventFrame(json: Data("{}".utf8), hostTimeNs: 0)
        let truncated = encoded.prefix(encoded.count - 1)
        XCTAssertThrowsError(try decodeFrame(from: Data(truncated)))
    }

    func testTwoFramesDecodeSequentially() throws {
        var stream = Data()
        stream.append(encodeAudioFrame(track: .mic, hostTimeNs: 1, sampleRate: 16_000, channels: 1, samples: [0.1]))
        stream.append(encodeEventFrame(json: Event.pong(), hostTimeNs: 2))

        let (first, consumed1) = try decodeFrame(from: stream)
        XCTAssertEqual(first.header.track, .mic)
        let (second, consumed2) = try decodeFrame(from: stream.suffix(from: stream.startIndex + consumed1))
        XCTAssertEqual(second.header.type, .event)
        XCTAssertEqual(consumed1 + consumed2, stream.count)
    }
}
