//! The local meeting folder: the single source of truth for a meeting (see
//! `packages/shared/MEETING_FOLDER.md`). Nothing about a meeting is saved anywhere
//! else. These structs mirror the zod schemas in
//! `packages/shared/src/meetingFolder.ts` / `meeting.ts` / `segment.ts`; keep them
//! in lockstep and bump `MEETING_FOLDER_VERSION` when a shape changes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::OffsetDateTime;

/// Must equal `MEETING_FOLDER_VERSION` in `packages/shared`.
pub const MEETING_FOLDER_VERSION: u32 = 1;

/// Warn the user when free disk space drops below this before starting (Prompt 5).
pub const LOW_DISK_BYTES: u64 = 1_000_000_000; // 1 GB

/// Standard file names inside a meeting folder.
pub mod files {
    pub const MIC: &str = "mic.ogg";
    pub const MEETING: &str = "meeting.ogg";
    pub const MIXED: &str = "mixed.ogg";
    pub const META: &str = "meeting.json";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Zoom,
    Meet,
    Teams,
    Webex,
    Other,
}

impl Platform {
    /// Map a capture-helper `sourceId` (e.g. `meet-chrome`) to a meeting platform.
    pub fn from_source_id(source_id: &str) -> Platform {
        if source_id.starts_with("meet") {
            Platform::Meet
        } else if source_id.starts_with("zoom") {
            Platform::Zoom
        } else if source_id.starts_with("teams") {
            Platform::Teams
        } else if source_id.starts_with("webex") {
            Platform::Webex
        } else {
            Platform::Other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingStatus {
    Recording,
    Stopped,
    Transcribing,
    Summarizing,
    Done,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriveSyncState {
    Off,
    Pending,
    Synced,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveSync {
    pub state: DriveSyncState,
    #[serde(rename = "driveFolderId", skip_serializing_if = "Option::is_none")]
    pub drive_folder_id: Option<String>,
    #[serde(rename = "lastSyncedAt", skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<String>,
}

impl Default for DriveSync {
    fn default() -> Self {
        Self {
            state: DriveSyncState::Off,
            drive_folder_id: None,
            last_synced_at: None,
        }
    }
}

/// `meeting.ts` `Meeting`. `recovered` is an additive, optional marker set when the
/// previous process died mid-recording and the files were repaired on next launch
/// (Prompt 5); readers that predate it ignore it, so the folder version is unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub platform: Platform,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "endedAt", skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(rename = "folderPath")]
    pub folder_path: String,
    pub status: MeetingStatus,
    #[serde(rename = "driveSync", skip_serializing_if = "Option::is_none")]
    pub drive_sync: Option<DriveSync>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovered: Option<bool>,
}

/// `segment.ts` `TranscriptSegment`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegment {
    pub id: String,
    #[serde(rename = "meetingId")]
    pub meeting_id: String,
    pub track: String,
    pub speaker: String,
    #[serde(rename = "speakerName", skip_serializing_if = "Option::is_none")]
    pub speaker_name: Option<String>,
    #[serde(rename = "startMs")]
    pub start_ms: u64,
    #[serde(rename = "endMs")]
    pub end_ms: u64,
    pub text: String,
    pub lang: String,
    #[serde(rename = "isFinal")]
    pub is_final: bool,
    pub pass: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// `meetingFolder.ts` `MeetingFile` — metadata plus the live-pass segments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingFile {
    pub version: u32,
    pub meeting: Meeting,
    #[serde(rename = "liveSegments")]
    pub live_segments: Vec<TranscriptSegment>,
}

impl MeetingFile {
    pub fn new(meeting: Meeting) -> Self {
        Self {
            version: MEETING_FOLDER_VERSION,
            meeting,
            live_segments: Vec::new(),
        }
    }
}

/// Absolute paths of the files inside one meeting folder.
#[derive(Debug, Clone)]
pub struct MeetingPaths {
    pub dir: PathBuf,
}

impl MeetingPaths {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    pub fn mic(&self) -> PathBuf {
        self.dir.join(files::MIC)
    }
    pub fn meeting(&self) -> PathBuf {
        self.dir.join(files::MEETING)
    }
    pub fn mixed(&self) -> PathBuf {
        self.dir.join(files::MIXED)
    }
    pub fn meta(&self) -> PathBuf {
        self.dir.join(files::META)
    }
}

/// Current time as an ISO-8601 UTC string with millisecond precision, matching JS
/// `Date.prototype.toISOString()`, for `startedAt` / `endedAt`.
pub fn now_iso() -> String {
    let fmt =
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z");
    OffsetDateTime::now_utc()
        .format(fmt)
        .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".into())
}

/// Folder name `<yyyy-mm-dd HHmm> <title>` in local time, with filesystem-unsafe
/// characters in the title replaced. Falls back to UTC if the local offset is
/// unavailable.
pub fn folder_name(title: &str) -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let fmt = format_description!("[year]-[month]-[day] [hour][minute]");
    let stamp = now.format(fmt).unwrap_or_else(|_| "0000-00-00 0000".into());
    let title = sanitize_title(title);
    if title.is_empty() {
        stamp
    } else {
        format!("{stamp} {title}")
    }
}

fn sanitize_title(title: &str) -> String {
    title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Free space in bytes on the volume holding `path` (or its nearest existing
/// ancestor), or `None` if it cannot be determined.
pub fn free_space_bytes(path: &Path) -> Option<u64> {
    let mut probe = path;
    loop {
        if probe.exists() {
            return fs4::available_space(probe).ok();
        }
        {
            let p = probe.parent()?;
            probe = p
        }
    }
}

/// Create a fresh meeting folder under `root` and return its paths and `MeetingFile`
/// (status `recording`). Does not write `meeting.json` yet — the caller writes it
/// once capture starts. Errors if the folder cannot be created.
pub fn create_meeting(
    root: &Path,
    id: &str,
    title: &str,
    platform: Platform,
) -> std::io::Result<(MeetingPaths, MeetingFile)> {
    let mut dir = root.join(folder_name(title));
    // Disambiguate same-minute, same-title meetings with a short id suffix.
    if dir.exists() {
        dir = root.join(format!("{} {}", folder_name(title), &id[..id.len().min(8)]));
    }
    std::fs::create_dir_all(&dir)?;
    let paths = MeetingPaths::new(dir.clone());
    let meeting = Meeting {
        id: id.to_string(),
        title: title.to_string(),
        platform,
        started_at: now_iso(),
        ended_at: None,
        folder_path: dir.to_string_lossy().to_string(),
        status: MeetingStatus::Recording,
        drive_sync: Some(DriveSync::default()),
        recovered: None,
    };
    Ok((paths, MeetingFile::new(meeting)))
}

/// Atomically write `meeting.json` (write to a temp file, then rename).
pub fn write_meeting_json(paths: &MeetingPaths, file: &MeetingFile) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(file).map_err(std::io::Error::other)?;
    let tmp = paths.dir.join("meeting.json.tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, paths.meta())
}

pub fn read_meeting_json(path: &Path) -> std::io::Result<MeetingFile> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(std::io::Error::other)
}

/// Scan `root` for meeting folders whose `meeting.json` still says `recording` — a
/// meeting whose process died before it could be finalized. The caller repairs each
/// one's audio files and marks it `recovered`.
pub fn scan_unfinished(root: &Path) -> Vec<MeetingPaths> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let paths = MeetingPaths::new(dir);
        if let Ok(file) = read_meeting_json(&paths.meta()) {
            if file.meeting.status == MeetingStatus::Recording {
                out.push(paths);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_name_has_date_and_title() {
        let name = folder_name("Weekly Sync");
        assert!(name.ends_with("Weekly Sync"), "got {name}");
        // yyyy-mm-dd HHmm = 15 chars before the space+title.
        assert!(name.len() >= 15 + 1 + "Weekly Sync".len());
    }

    #[test]
    fn title_is_sanitized() {
        let name = folder_name("Q3: plan/review?");
        assert!(!name.contains('/'));
        assert!(!name.contains(':'));
        assert!(!name.contains('?'));
    }

    #[test]
    fn meeting_json_round_trips_with_shared_shape() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, mut file) =
            create_meeting(dir.path(), "m-123", "Standup", Platform::Zoom).unwrap();
        file.live_segments.push(TranscriptSegment {
            id: "seg-0".into(),
            meeting_id: "m-123".into(),
            track: "mic".into(),
            speaker: "me".into(),
            speaker_name: None,
            start_ms: 0,
            end_ms: 500,
            text: "hello".into(),
            lang: "en".into(),
            is_final: true,
            pass: "live".into(),
            confidence: Some(0.9),
        });
        write_meeting_json(&paths, &file).unwrap();

        let raw = std::fs::read_to_string(paths.meta()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["meeting"]["platform"], "zoom");
        assert_eq!(v["meeting"]["status"], "recording");
        let started = v["meeting"]["startedAt"].as_str().unwrap();
        assert_eq!(started.len(), 24); // yyyy-mm-ddThh:mm:ss.sssZ, like toISOString()
        assert!(started.ends_with('Z') && started.contains('T'));
        assert_eq!(v["liveSegments"][0]["startMs"], 0);
        assert_eq!(v["liveSegments"][0]["isFinal"], true);
    }

    #[test]
    fn scan_finds_only_unfinalized_meetings() {
        let dir = tempfile::tempdir().unwrap();
        // One left "recording" (crashed), one finalized "done".
        let (p1, f1) = create_meeting(dir.path(), "crash-1", "Crashed", Platform::Meet).unwrap();
        write_meeting_json(&p1, &f1).unwrap();
        let (p2, mut f2) = create_meeting(dir.path(), "ok-2", "Fine", Platform::Meet).unwrap();
        f2.meeting.status = MeetingStatus::Done;
        write_meeting_json(&p2, &f2).unwrap();

        let found = scan_unfinished(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dir, p1.dir);
    }

    #[test]
    fn free_space_reports_for_existing_path() {
        let dir = tempfile::tempdir().unwrap();
        let free = free_space_bytes(dir.path());
        assert!(free.is_some());
        assert!(free.unwrap() > 0);
    }
}
