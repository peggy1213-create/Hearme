//! Recording a meeting to its folder: `mic.ogg`, `meeting.ogg` and a mixed
//! `mixed.ogg` for playback (MEETING_FOLDER.md). Also crash recovery: on launch,
//! unfinished meetings are repaired and marked `recovered`.

pub mod oggopus;

use std::collections::VecDeque;
use std::path::Path;

use crate::audio::{Recorder, REC_RATE};
use crate::frame::Track;
use crate::meeting::{self, MeetingPaths, MeetingStatus};
use oggopus::{repair_ogg, OggOpusWriter};

/// Cap on how far the two mix queues may diverge before the longer one is mixed
/// against silence, so a track that goes quiet for a long time cannot grow memory
/// without bound (~10 s).
const MAX_LAG: usize = REC_RATE as usize * 10;

/// Writes the three Opus/Ogg files and keeps a time-aligned mix for playback.
///
/// Each track is written contiguously to its own file. For the mix, both tracks are
/// placed on one absolute 48 kHz timeline (each queue is pre-padded by the track's
/// start offset), then summed sample-for-sample as far as both have arrived.
pub struct OggRecorder {
    mic: OggOpusWriter,
    meeting: OggOpusWriter,
    mixed: OggOpusWriter,
    mic_q: VecDeque<f32>,
    meeting_q: VecDeque<f32>,
    mic_started: bool,
    meeting_started: bool,
}

impl OggRecorder {
    /// Create the three files under `paths` and write their Opus headers.
    pub fn create(paths: &MeetingPaths) -> anyhow::Result<Self> {
        Ok(Self {
            mic: OggOpusWriter::create(&paths.mic())?,
            meeting: OggOpusWriter::create(&paths.meeting())?,
            mixed: OggOpusWriter::create(&paths.mixed())?,
            mic_q: VecDeque::new(),
            meeting_q: VecDeque::new(),
            mic_started: false,
            meeting_started: false,
        })
    }

    fn enqueue_mix(&mut self, track: Track, start_sample: u64, samples: &[f32]) {
        match track {
            Track::Mic => {
                if !self.mic_started {
                    self.mic_started = true;
                    self.mic_q
                        .extend(std::iter::repeat_n(0.0, start_sample as usize));
                }
                self.mic_q.extend(samples.iter().copied());
            }
            Track::Meeting => {
                if !self.meeting_started {
                    self.meeting_started = true;
                    self.meeting_q
                        .extend(std::iter::repeat_n(0.0, start_sample as usize));
                }
                self.meeting_q.extend(samples.iter().copied());
            }
        }
    }

    /// Mix and write everything both tracks have in common, plus any overflow beyond
    /// `MAX_LAG` (mixed against silence to bound memory).
    fn drain_mix(&mut self) -> anyhow::Result<()> {
        let common = self.mic_q.len().min(self.meeting_q.len());
        let mut out = Vec::with_capacity(common);
        for _ in 0..common {
            let a = self.mic_q.pop_front().unwrap();
            let b = self.meeting_q.pop_front().unwrap();
            out.push(mix(a, b));
        }
        // Bound memory if one track is far ahead of the other.
        for q in [true, false] {
            let (long, _short_empty) = if q {
                (&mut self.mic_q, self.meeting_q.is_empty())
            } else {
                (&mut self.meeting_q, self.mic_q.is_empty())
            };
            if long.len() > MAX_LAG {
                let overflow = long.len() - MAX_LAG;
                for _ in 0..overflow {
                    out.push(mix(long.pop_front().unwrap(), 0.0));
                }
            }
        }
        if !out.is_empty() {
            self.mixed.write_samples(&out)?;
        }
        Ok(())
    }

    /// Flush all three files to disk. The driver calls this at least every 10 s.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        self.mic.flush()?;
        self.meeting.flush()?;
        self.mixed.flush()?;
        Ok(())
    }

    /// Mix any remaining audio (padding the shorter track with silence) and close all
    /// three files.
    pub fn finalize(mut self) -> anyhow::Result<()> {
        // Flush the remaining common region, then pad the shorter queue so no captured
        // audio is dropped from the mix.
        self.drain_mix()?;
        let tail = self.mic_q.len().max(self.meeting_q.len());
        let mut out = Vec::with_capacity(tail);
        for _ in 0..tail {
            let a = self.mic_q.pop_front().unwrap_or(0.0);
            let b = self.meeting_q.pop_front().unwrap_or(0.0);
            out.push(mix(a, b));
        }
        if !out.is_empty() {
            self.mixed.write_samples(&out)?;
        }
        self.mic.finalize()?;
        self.meeting.finalize()?;
        self.mixed.finalize()?;
        Ok(())
    }
}

impl Recorder for OggRecorder {
    fn write(&mut self, track: Track, start_sample: u64, samples: &[f32]) {
        let writer = match track {
            Track::Mic => &mut self.mic,
            Track::Meeting => &mut self.meeting,
        };
        if let Err(e) = writer.write_samples(samples) {
            tracing::warn!("recording write failed for {:?}: {e}", track);
        }
        self.enqueue_mix(track, start_sample, samples);
        if let Err(e) = self.drain_mix() {
            tracing::warn!("mix write failed: {e}");
        }
    }
}

/// Sum two tracks for playback, clamped to avoid clipping.
fn mix(a: f32, b: f32) -> f32 {
    (a + b).clamp(-1.0, 1.0)
}

/// Result of recovering one crashed meeting.
#[derive(Debug, Clone)]
pub struct Recovered {
    pub dir: std::path::PathBuf,
    pub bytes_repaired: u64,
}

/// On launch, find meetings still marked `recording` (the process died mid-capture),
/// repair their Ogg files, and mark each `recovered` + `stopped` so the final pass
/// can still transcribe them. Returns what was recovered.
pub fn recover_unfinished(root: &Path) -> Vec<Recovered> {
    let mut out = Vec::new();
    for paths in meeting::scan_unfinished(root) {
        let mut repaired = 0u64;
        for f in [paths.mic(), paths.meeting(), paths.mixed()] {
            if f.exists() {
                repaired += repair_ogg(&f).unwrap_or(0);
            }
        }
        if let Ok(mut file) = meeting::read_meeting_json(&paths.meta()) {
            file.meeting.status = MeetingStatus::Stopped;
            file.meeting.recovered = Some(true);
            if file.meeting.ended_at.is_none() {
                file.meeting.ended_at = Some(meeting::now_iso());
            }
            let _ = meeting::write_meeting_json(&paths, &file);
        }
        out.push(Recovered {
            dir: paths.dir.clone(),
            bytes_repaired: repaired,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting::{create_meeting, write_meeting_json, Platform};

    #[test]
    fn records_three_files_and_mixes() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, file) = create_meeting(dir.path(), "m1", "Test", Platform::Zoom).unwrap();
        write_meeting_json(&paths, &file).unwrap();

        let mut rec = OggRecorder::create(&paths).unwrap();
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        rec.write(Track::Meeting, 0, &tone);
        rec.write(Track::Mic, 0, &tone);
        rec.flush().unwrap();
        rec.finalize().unwrap();

        for p in [paths.mic(), paths.meeting(), paths.mixed()] {
            let bytes = std::fs::read(&p).unwrap();
            assert!(bytes.len() > 1000, "{p:?} too small");
            assert_eq!(&bytes[0..4], b"OggS");
            assert_eq!(
                oggopus::tests_last_page_end(&bytes),
                bytes.len(),
                "{p:?} not a whole page sequence"
            );
        }
    }

    #[test]
    fn recovery_repairs_and_marks_recovered() {
        let dir = tempfile::tempdir().unwrap();
        let (paths, file) = create_meeting(dir.path(), "crash", "Boom", Platform::Meet).unwrap();
        write_meeting_json(&paths, &file).unwrap(); // status: recording

        // Write some audio and flush whole pages, then simulate a crash: files are
        // left un-finalized with a half-written trailing page.
        {
            let mut rec = OggRecorder::create(&paths).unwrap();
            let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.04).sin() * 0.3).collect();
            rec.write(Track::Meeting, 0, &tone);
            rec.write(Track::Mic, 0, &tone);
            rec.flush().unwrap();
            std::mem::forget(rec); // skip finalize/Drop → simulate crash
        }
        {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(paths.meeting())
                .unwrap();
            f.write_all(b"OggS\x00\x02trunc").unwrap();
        }

        let recovered = recover_unfinished(dir.path());
        assert_eq!(recovered.len(), 1);
        assert!(recovered[0].bytes_repaired > 0);

        let reloaded = meeting::read_meeting_json(&paths.meta()).unwrap();
        assert_eq!(reloaded.meeting.status, MeetingStatus::Stopped);
        assert_eq!(reloaded.meeting.recovered, Some(true));
        assert!(reloaded.meeting.ended_at.is_some());

        // The repaired file is a whole page sequence again.
        let bytes = std::fs::read(paths.meeting()).unwrap();
        assert_eq!(oggopus::tests_last_page_end(&bytes), bytes.len());
    }
}
