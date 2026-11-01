import Foundation

/// Minimal incremental 16-bit PCM WAV writer for the `record` debug subcommand, so a
/// human can listen to `mic.wav` / `meeting.wav`. Converts the capture path's
/// `float32` samples to little-endian Int16. Not used on the live capture path.
public final class WavWriter {
    private let handle: FileHandle
    private let sampleRate: UInt32
    private let channels: UInt16
    private var dataBytes: UInt32 = 0
    private let lock = NSLock()
    private var closed = false

    public init(url: URL, sampleRate: UInt32, channels: UInt16) throws {
        FileManager.default.createFile(atPath: url.path, contents: nil)
        handle = try FileHandle(forWritingTo: url)
        self.sampleRate = max(1, sampleRate)
        self.channels = max(1, channels)
        handle.write(Self.header(sampleRate: self.sampleRate, channels: self.channels, dataBytes: 0))
    }

    public func append(_ samples: [Float]) {
        lock.lock()
        defer { lock.unlock() }
        guard !closed else { return }
        var pcm = [Int16]()
        pcm.reserveCapacity(samples.count)
        for sample in samples {
            let clamped = max(-1.0, min(1.0, sample))
            pcm.append(Int16(clamped * 32767))
        }
        let data = pcm.withUnsafeBufferPointer { Data(buffer: $0) }
        handle.write(data)
        dataBytes &+= UInt32(truncatingIfNeeded: data.count)
    }

    public func close() {
        lock.lock()
        defer { lock.unlock() }
        guard !closed else { return }
        closed = true
        try? handle.seek(toOffset: 0)
        handle.write(Self.header(sampleRate: sampleRate, channels: channels, dataBytes: dataBytes))
        try? handle.close()
    }

    private static func header(sampleRate: UInt32, channels: UInt16, dataBytes: UInt32) -> Data {
        let bitsPerSample: UInt16 = 16
        let blockAlign = channels * bitsPerSample / 8
        let byteRate = sampleRate * UInt32(blockAlign)

        var data = Data()
        func le<T: FixedWidthInteger>(_ v: T) {
            var x = v.littleEndian
            withUnsafeBytes(of: &x) { data.append(contentsOf: $0) }
        }
        data.append(contentsOf: Array("RIFF".utf8))
        le(UInt32(36) &+ dataBytes)
        data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8))
        le(UInt32(16))          // PCM fmt chunk size
        le(UInt16(1))           // audioFormat = PCM
        le(channels)
        le(sampleRate)
        le(byteRate)
        le(blockAlign)
        le(bitsPerSample)
        data.append(contentsOf: Array("data".utf8))
        le(dataBytes)
        return data
    }
}
