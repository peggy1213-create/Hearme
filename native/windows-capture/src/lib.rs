//! Hearme Windows audio capture helper (stub).
//!
//! This helper will capture a specific process's output audio using WASAPI
//! process loopback (Windows 10/11) plus the microphone as a separate track, and
//! write Opus/Ogg to disk. It never joins the meeting. Not implemented yet. See
//! ../../CLAUDE.md and docs/plan.md.

/// Version of the Windows capture helper.
pub const CAPTURE_VERSION: &str = "0.0.0";

/// Usage text printed by the CLI. Kept in a function so tests can assert on it.
pub fn usage_text() -> String {
    format!(
        "hearme-windows-capture {CAPTURE_VERSION} — stub\n\
         \n\
         Captures per-process output audio (WASAPI process loopback, Windows 10/11)\n\
         and the microphone as a separate track, without joining the meeting.\n\
         Not implemented yet.\n\
         \n\
         USAGE:\n  \
         hearme-windows-capture --help"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_mentions_helper() {
        let text = usage_text();
        assert!(text.contains("hearme-windows-capture"));
        assert!(text.contains(CAPTURE_VERSION));
    }
}
