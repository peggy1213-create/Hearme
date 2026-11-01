import Foundation

/// Serializes writes of binary frames to stdout. Audio callbacks run on real-time
/// Core Audio / AVAudioEngine threads, so writes are guarded by a lock and flushed in
/// full. stdout is the only channel for audio and events; stderr is for logs.
public final class StdoutWriter {
    private let lock = NSLock()
    private let fd: Int32

    public init(fd: Int32 = STDOUT_FILENO) {
        self.fd = fd
    }

    /// Write a fully-encoded frame, retrying on partial writes and `EINTR`.
    public func write(_ frame: Data) {
        lock.lock()
        defer { lock.unlock() }
        frame.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            guard var ptr = raw.baseAddress else { return }
            var remaining = raw.count
            while remaining > 0 {
                let n = Foundation.write(fd, ptr, remaining)
                if n > 0 {
                    ptr = ptr.advanced(by: n)
                    remaining -= n
                } else if n < 0 && (errno == EINTR || errno == EAGAIN) {
                    continue
                } else {
                    // Core has gone away (closed pipe). Nothing useful to do from a
                    // capture thread; drop and let the next command/read surface it.
                    return
                }
            }
        }
    }
}

/// Human-readable logging to stderr. Never pass audio samples or transcript text.
public func logStderr(_ message: String) {
    FileHandle.standardError.write(Data((message + "\n").utf8))
}
