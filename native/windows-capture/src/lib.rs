//! Hearme Windows audio capture helper (`tapcapture`).
//!
//! Captures a meeting app's output audio via WASAPI **process loopback** (Windows 10
//! Build 20348+) plus the microphone as a separate, time-aligned track, **without
//! joining the meeting**. It only captures and reports audio; recording, encoding and
//! streaming live in the Tauri core. It speaks the same stdin/stdout protocol as the
//! macOS helper — see `packages/shared/CAPTURE_HELPER.md` and
//! `docs/decisions/0004-windows-audio-capture.md`.

pub mod clock;
pub mod frame;
pub mod known_apps;
pub mod protocol;
pub mod silence_guard;

#[cfg(windows)]
pub mod wasapi;

#[cfg(windows)]
pub mod sessions;

#[cfg(windows)]
pub mod mic;

#[cfg(windows)]
pub mod meeting;

#[cfg(windows)]
pub mod device_watcher;

#[cfg(windows)]
pub mod session;

#[cfg(windows)]
pub mod wav;

#[cfg(windows)]
pub mod recorder;

/// Version of the Windows capture helper binary.
pub const CAPTURE_VERSION: &str = "0.1.0";

/// Protocol version shared with `packages/shared/CAPTURE_HELPER.md` and the macOS
/// helper. Bump both together when any frame, command or event shape changes.
pub const CAPTURE_HELPER_PROTOCOL_VERSION: u32 = 1;

/// Usage text printed by `--help`. Kept in a function so tests can assert on it.
pub fn usage_text() -> String {
    format!(
        "tapcapture {CAPTURE_VERSION} — Hearme Windows audio capture helper\n\
         \n\
         Captures a meeting app's output audio (WASAPI process loopback) plus the\n\
         microphone as a separate track, without joining the meeting. With no\n\
         arguments it runs in protocol mode: JSON commands on stdin, length-prefixed\n\
         binary frames on stdout (see packages/shared/CAPTURE_HELPER.md).\n\
         \n\
         USAGE:\n  \
         tapcapture                 run in protocol mode (the core's sidecar)\n  \
         tapcapture record --process <exe> [--seconds <n>] [--out <dir>]\n  \
         tapcapture --help\n\
         \n\
         record writes mic.wav and meeting.wav so you can confirm the two tracks are\n\
         clean and aligned, e.g.:\n  \
         tapcapture record --process chrome.exe --seconds 30 --out .\\out"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_mentions_helper_and_version() {
        let text = usage_text();
        assert!(text.contains("tapcapture"));
        assert!(text.contains(CAPTURE_VERSION));
        assert!(text.contains("record"));
    }

    #[test]
    fn protocol_version_is_one() {
        assert_eq!(CAPTURE_HELPER_PROTOCOL_VERSION, 1);
    }
}
