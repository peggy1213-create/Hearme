import Foundation

/// Version of the macOS capture helper.
public let captureVersion = "0.0.0"

/// Usage text printed by the CLI. Kept in a function so tests can assert on it.
///
/// This helper will capture a specific app's output audio using Core Audio
/// process taps (macOS 14.2+) plus the microphone as a separate track, and write
/// Opus/Ogg to disk. It never joins the meeting. Not implemented yet. See
/// ../../CLAUDE.md and docs/plan.md.
public func usageText() -> String {
    """
    hearme-macos-capture \(captureVersion) — stub

    Captures per-app output audio (Core Audio process taps, macOS 14.2+) and the
    microphone as a separate track, without joining the meeting. Not implemented yet.

    USAGE:
      hearme-macos-capture --help
    """
}
