import XCTest
@testable import tapcapture

final class SilenceGuardTests: XCTestCase {
    func testFiresAfterThresholdOfZerosWhilePlaying() {
        var fired = 0
        let guardImpl = SilenceGuard(track: .meeting, thresholdSeconds: 1.0) { _ in fired += 1 }
        let rate: UInt32 = 48_000
        // 0.5 s of zeros: not yet.
        guardImpl.feed(samples: [Float](repeating: 0, count: 24_000), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 0)
        // Another 0.6 s crosses the 1 s threshold.
        guardImpl.feed(samples: [Float](repeating: 0, count: 28_800), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 1)
        // Stays fired once, not repeatedly.
        guardImpl.feed(samples: [Float](repeating: 0, count: 48_000), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 1)
    }

    func testDoesNotFireWhenSourceNotPlaying() {
        var fired = 0
        let guardImpl = SilenceGuard(track: .meeting, thresholdSeconds: 0.5) { _ in fired += 1 }
        guardImpl.feed(samples: [Float](repeating: 0, count: 96_000), sampleRate: 48_000, sourcePlaying: false)
        XCTAssertEqual(fired, 0)
    }

    func testNonZeroSampleResetsAndReArms() {
        var fired = 0
        let guardImpl = SilenceGuard(track: .meeting, thresholdSeconds: 1.0) { _ in fired += 1 }
        let rate: UInt32 = 48_000
        guardImpl.feed(samples: [Float](repeating: 0, count: 47_000), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 0)
        // Real audio arrives -> reset.
        guardImpl.feed(samples: [0.0, 0.0, 0.2], sampleRate: rate, sourcePlaying: true)
        // Zeros start over; need a full second again.
        guardImpl.feed(samples: [Float](repeating: 0, count: 47_000), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 0)
        guardImpl.feed(samples: [Float](repeating: 0, count: 2_000), sampleRate: rate, sourcePlaying: true)
        XCTAssertEqual(fired, 1)
    }
}
