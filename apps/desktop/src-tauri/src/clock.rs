//! Mapping the helper's shared host clock to meeting-relative milliseconds.
//!
//! `hostTimeNs` on both tracks comes from one monotonic host clock that is
//! arbitrarily zeroed (see CAPTURE_HELPER.md). The core picks the first sample it
//! sees on *either* track as meeting time zero, so mic and meeting samples captured
//! at the same instant get the same `timestampMs` for the gateway (PROTOCOL.md).

/// Converts monotonic host nanoseconds to meeting-relative milliseconds, anchored on
/// the first frame seen.
#[derive(Debug, Default)]
pub struct MeetingClock {
    origin_ns: Option<u64>,
}

impl MeetingClock {
    pub fn new() -> Self {
        Self { origin_ns: None }
    }

    /// Milliseconds from meeting start for a sample stamped `host_time_ns`. The first
    /// call fixes the origin and returns 0. Clamped at 0 so a slightly earlier frame
    /// on the other track can never go negative.
    pub fn to_meeting_ms(&mut self, host_time_ns: u64) -> u64 {
        let origin = *self.origin_ns.get_or_insert(host_time_ns);
        host_time_ns.saturating_sub(origin) / 1_000_000
    }

    /// Whether the origin has been anchored yet.
    pub fn started(&self) -> bool {
        self.origin_ns.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_is_zero() {
        let mut clock = MeetingClock::new();
        assert_eq!(clock.to_meeting_ms(5_000_000_000), 0);
        assert!(clock.started());
    }

    #[test]
    fn later_frames_are_relative_ms() {
        let mut clock = MeetingClock::new();
        clock.to_meeting_ms(1_000_000_000); // origin at 1 s
        assert_eq!(clock.to_meeting_ms(1_020_000_000), 20); // +20 ms
        assert_eq!(clock.to_meeting_ms(2_000_000_000), 1_000); // +1 s
    }

    #[test]
    fn tracks_share_one_origin() {
        // Meeting frame arrives first and anchors zero; a mic frame 10 ms later lines
        // up against the same origin.
        let mut clock = MeetingClock::new();
        assert_eq!(clock.to_meeting_ms(500_000_000), 0);
        assert_eq!(clock.to_meeting_ms(510_000_000), 10);
    }

    #[test]
    fn earlier_frame_clamps_to_zero() {
        let mut clock = MeetingClock::new();
        clock.to_meeting_ms(1_000_000_000);
        assert_eq!(clock.to_meeting_ms(999_000_000), 0);
    }
}
