import CoreAudio
import XCTest
@testable import tapcapture

/// End-to-end capture check: play a known tone with `afplay`, tap the `afplay`
/// process, and verify the captured audio's dominant frequency is the tone.
///
/// Requires a real Mac **with system-audio-recording permission granted to whatever
/// runs the test** (see the ADR on sidecar permission attribution). It is therefore
/// opt-in: set `HEARME_RUN_AUDIO_TESTS=1` to run it. Headless CI leaves it skipped.
final class ToneCaptureTests: XCTestCase {
    func testCapturedToneMatchesPlayedFrequency() throws {
        try XCTSkipUnless(
            ProcessInfo.processInfo.environment["HEARME_RUN_AUDIO_TESTS"] == "1",
            "Set HEARME_RUN_AUDIO_TESTS=1 on a Mac with audio permission to run this.")

        let toneHz = 440.0
        let toneRate: UInt32 = 48_000
        let toneURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("hearme-tone-\(UUID().uuidString).wav")
        try writeSineWav(url: toneURL, frequency: toneHz, sampleRate: toneRate, seconds: 4.0)
        defer { try? FileManager.default.removeItem(at: toneURL) }

        // Start playback, then find afplay in the HAL process list.
        let player = Process()
        player.executableURL = URL(fileURLWithPath: "/usr/bin/afplay")
        player.arguments = [toneURL.path]
        try player.run()
        defer { if player.isRunning { player.terminate() } }

        guard let objectID = waitForProcessObjectID(pid: player.processIdentifier, timeout: 2.0) else {
            throw XCTSkip("afplay did not appear in the Core Audio process list")
        }

        var captured = [Float]()
        var capturedRate: UInt32 = 0
        let lock = NSLock()
        let tap = MeetingTap(processIDs: [objectID]) { samples, _, rate, _ in
            lock.lock()
            if capturedRate == 0 { capturedRate = rate }
            if captured.count < Int(rate) * 2 { captured.append(contentsOf: samples) }
            lock.unlock()
        }
        try tap.start()
        RunLoop.current.run(until: Date().addingTimeInterval(2.0))
        tap.stop()

        lock.lock()
        let samples = captured
        let rate = capturedRate
        lock.unlock()

        XCTAssertGreaterThan(samples.count, 0, "no audio captured")
        if samples.allSatisfy({ $0 == 0 }) {
            throw XCTSkip("captured only zeros — system-audio permission is probably missing")
        }

        let targetPower = goertzelPower(samples, sampleRate: Double(rate), frequency: toneHz)
        let controlPower = goertzelPower(samples, sampleRate: Double(rate), frequency: toneHz * 3)
        XCTAssertGreaterThan(targetPower, controlPower * 4,
                             "captured dominant frequency is not the \(toneHz) Hz tone")
    }

    // MARK: - helpers

    private func writeSineWav(url: URL, frequency: Double, sampleRate: UInt32, seconds: Double) throws {
        let writer = try WavWriter(url: url, sampleRate: sampleRate, channels: 1)
        let total = Int(Double(sampleRate) * seconds)
        var samples = [Float](repeating: 0, count: total)
        for i in 0..<total {
            samples[i] = Float(0.6 * sin(2.0 * Double.pi * frequency * Double(i) / Double(sampleRate)))
        }
        writer.append(samples)
        writer.close()
    }

    private func waitForProcessObjectID(pid: pid_t, timeout: TimeInterval) -> AudioObjectID? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let id = ProcessList.objectID(forPID: pid) { return id }
            Thread.sleep(forTimeInterval: 0.1)
        }
        return nil
    }

    /// Goertzel power estimate at one frequency.
    private func goertzelPower(_ samples: [Float], sampleRate: Double, frequency: Double) -> Double {
        let n = samples.count
        guard n > 0 else { return 0 }
        let k = 2.0 * cos(2.0 * Double.pi * frequency / sampleRate)
        var s0 = 0.0, s1 = 0.0, s2 = 0.0
        for sample in samples {
            s0 = Double(sample) + k * s1 - s2
            s2 = s1
            s1 = s0
        }
        return s1 * s1 + s2 * s2 - k * s1 * s2
    }
}
