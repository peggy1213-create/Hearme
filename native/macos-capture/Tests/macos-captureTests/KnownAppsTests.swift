import XCTest
@testable import tapcapture

final class KnownAppsTests: XCTestCase {
    private let config = KnownApps.load()

    func testLoadHasAllFourPlatforms() {
        let ids = Set(config.apps.map { $0.id })
        XCTAssertTrue(ids.contains("zoom"))
        XCTAssertTrue(ids.contains("teams"))
        XCTAssertTrue(ids.contains("webex"))
        // At least one browser source for Google Meet.
        XCTAssertTrue(ids.contains { $0.hasPrefix("meet-") })
    }

    func testBundleMatchingResolvesMainAndHelperProcesses() {
        XCTAssertEqual(KnownApps.match(bundleID: "us.zoom.xos", in: config)?.id, "zoom")
        // Helper / renderer processes share a bundle-ID prefix with the browser.
        XCTAssertEqual(KnownApps.match(bundleID: "com.google.Chrome.helper", in: config)?.id, "meet-chrome")
        XCTAssertEqual(KnownApps.match(bundleID: "com.google.Chrome.helper.Renderer", in: config)?.id, "meet-chrome")
        XCTAssertEqual(KnownApps.match(bundleID: "com.microsoft.teams2", in: config)?.id, "teams")
        XCTAssertEqual(KnownApps.match(bundleID: "com.microsoft.edgemac.helper", in: config)?.id, "meet-edge")
    }

    func testBundleMatchingIsCaseInsensitive() {
        XCTAssertEqual(KnownApps.match(bundleID: "US.ZOOM.XOS", in: config)?.id, "zoom")
    }

    func testUnknownBundleReturnsNil() {
        XCTAssertNil(KnownApps.match(bundleID: "com.apple.Music", in: config))
        XCTAssertNil(KnownApps.match(bundleID: "", in: config))
    }

    func testDecodeFromJSONData() throws {
        let json = Data("""
        {"version":1,"apps":[{"id":"zoom","name":"Zoom","kind":"app","bundleIds":["us.zoom."]}]}
        """.utf8)
        let decoded = try KnownApps.decode(data: json)
        XCTAssertEqual(decoded.version, 1)
        XCTAssertEqual(decoded.apps.count, 1)
        XCTAssertTrue(decoded.apps[0].matches(bundleID: "us.zoom.xos"))
    }
}
