import XCTest
@testable import tapcapture

final class CaptureTests: XCTestCase {
    func testUsageMentionsHelper() {
        let text = usageText()
        XCTAssertTrue(text.contains("tapcapture"))
        XCTAssertTrue(text.contains(captureVersion))
    }
}
