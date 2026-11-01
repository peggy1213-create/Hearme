import XCTest
@testable import tapcapture

final class ProtocolTests: XCTestCase {
    func testParseStartCommand() throws {
        let cmd = try parseCommand(line: #"{"cmd":"start","sourceId":"zoom","micDeviceId":"abc"}"#)
        XCTAssertEqual(cmd?.cmd, "start")
        XCTAssertEqual(cmd?.sourceId, "zoom")
        XCTAssertEqual(cmd?.micDeviceId, "abc")
    }

    func testParseSimpleCommand() throws {
        let cmd = try parseCommand(line: #"{"cmd":"list_sources"}"#)
        XCTAssertEqual(cmd?.cmd, "list_sources")
        XCTAssertNil(cmd?.sourceId)
    }

    func testBlankLineIsNil() throws {
        XCTAssertNil(try parseCommand(line: "   "))
        XCTAssertNil(try parseCommand(line: ""))
    }

    func testNonJSONThrows() {
        XCTAssertThrowsError(try parseCommand(line: "not json")) { error in
            XCTAssertEqual(error as? CommandParseError, .notJSON)
        }
    }

    func testValidJSONWithoutCmdThrows() {
        XCTAssertThrowsError(try parseCommand(line: #"{"foo":"bar"}"#)) { error in
            XCTAssertEqual(error as? CommandParseError, .missingCmd)
        }
    }

    func testEventJSONHasSortedKeysAndEventField() {
        let data = Event.sourceStarted(sourceId: "zoom", bundleId: "us.zoom.xos", name: "Zoom")
        let text = String(decoding: data, as: UTF8.self)
        XCTAssertTrue(text.contains(#""event":"source_started""#))
        XCTAssertTrue(text.contains(#""bundleId":"us.zoom.xos""#))
        // sortedKeys => "bundleId" appears before "event" before "name" before "sourceId".
        let keysInOrder = ["bundleId", "event", "name", "sourceId"]
        var searchStart = text.startIndex
        for key in keysInOrder {
            guard let range = text.range(of: "\"\(key)\"", range: searchStart..<text.endIndex) else {
                return XCTFail("missing key \(key) or out of order")
            }
            searchStart = range.upperBound
        }
    }

    func testPermissionEventUsesTrackName() {
        XCTAssertTrue(String(decoding: Event.permissionSuspectedMissing(track: .meeting), as: UTF8.self)
            .contains(#""track":"meeting""#))
        XCTAssertTrue(String(decoding: Event.permissionSuspectedMissing(track: .mic), as: UTF8.self)
            .contains(#""track":"mic""#))
    }

    func testProtocolVersionIsOne() {
        XCTAssertEqual(captureHelperProtocolVersion, 1)
    }
}
