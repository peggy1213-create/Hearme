import Foundation

/// Version of the macOS capture helper.
public let captureVersion = "0.1.0"

/// Usage text printed by `--help`. Kept in a function so tests can assert on it.
public func usageText() -> String {
    """
    tapcapture \(captureVersion) — Hearme macOS audio capture helper

    Captures a meeting app's output audio (Core Audio process taps, macOS 14.2+) and
    the microphone as a separate, aligned track, without joining the meeting. It only
    captures and reports audio; recording, encoding and streaming happen in the
    desktop core. Protocol: packages/shared/CAPTURE_HELPER.md.

    USAGE:
      tapcapture                      Protocol mode (sidecar): JSON commands on stdin,
                                      binary audio/event frames on stdout.
      tapcapture record \\
        --bundle <bundle-id> \\
        --seconds <n> \\
        --out <dir>                   Debug: capture a bundle's output + mic for <n>
                                      seconds into <dir>/meeting.wav and <dir>/mic.wav.
      tapcapture --help               This text.

    EXAMPLE:
      tapcapture record --bundle us.zoom.xos --seconds 30 --out ./out
    """
}
