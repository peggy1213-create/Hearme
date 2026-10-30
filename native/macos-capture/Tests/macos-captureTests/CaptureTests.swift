import XCTest
@testable import macos_capture

final class CaptureTests: XCTestCase {
    func testUsageMentionsHelper() {
        let text = usageText()
        XCTAssertTrue(text.contains("hearme-macos-capture"))
        XCTAssertTrue(text.contains(captureVersion))
    }
}
