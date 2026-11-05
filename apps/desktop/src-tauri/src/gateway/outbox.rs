//! On-disk buffer of sent-but-unacknowledged audio frames, per track.
//!
//! PROTOCOL.md resume: the client keeps every frame it has sent until the gateway
//! acks it; on reconnect it resends everything after the gateway's `lastSeq`. Keeping
//! the buffer on disk means a brief disconnect (or a flush) never loses queued audio.
//! The file for a track is rewritten to hold only the still-unacked frames whenever an
//! ack advances the cursor, so it stays small.

use std::io::Write;
use std::path::{Path, PathBuf};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::frame::Track;

/// One queued frame: its per-track sequence number, meeting timestamp, and Opus bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub opus: Vec<u8>,
}

/// Per-track unacked buffer backed by a file.
struct TrackBox {
    path: PathBuf,
    frames: Vec<Pending>,
}

impl TrackBox {
    fn open(path: PathBuf) -> std::io::Result<Self> {
        let frames = if path.exists() {
            read_frames(&path)?
        } else {
            Vec::new()
        };
        Ok(Self { path, frames })
    }

    fn append(&mut self, p: Pending) -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        write_frame(&mut f, &p)?;
        f.flush()?;
        self.frames.push(p);
        Ok(())
    }

    fn ack(&mut self, last_seq: i64) -> std::io::Result<()> {
        let before = self.frames.len();
        self.frames.retain(|p| (p.seq as i64) > last_seq);
        if self.frames.len() != before {
            self.rewrite()?;
        }
        Ok(())
    }

    fn rewrite(&mut self) -> std::io::Result<()> {
        let tmp = self.path.with_extension("tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            for p in &self.frames {
                write_frame(&mut f, p)?;
            }
            f.flush()?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)
    }

    fn resend_after(&self, last_seq: i64) -> Vec<Pending> {
        self.frames
            .iter()
            .filter(|p| (p.seq as i64) > last_seq)
            .cloned()
            .collect()
    }
}

/// The two-track outbox for one meeting.
pub struct Outbox {
    mic: TrackBox,
    meeting: TrackBox,
}

impl Outbox {
    /// Open (or recover) the outbox under `dir`, creating `dir` if needed.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            mic: TrackBox::open(dir.join("outbox-mic.bin"))?,
            meeting: TrackBox::open(dir.join("outbox-meeting.bin"))?,
        })
    }

    fn track_mut(&mut self, track: Track) -> &mut TrackBox {
        match track {
            Track::Mic => &mut self.mic,
            Track::Meeting => &mut self.meeting,
        }
    }

    fn track(&self, track: Track) -> &TrackBox {
        match track {
            Track::Mic => &self.mic,
            Track::Meeting => &self.meeting,
        }
    }

    /// Record a frame as sent-but-unacked.
    pub fn record(&mut self, track: Track, p: Pending) -> std::io::Result<()> {
        self.track_mut(track).append(p)
    }

    /// Drop everything up to and including `last_seq` for `track`.
    pub fn ack(&mut self, track: Track, last_seq: i64) -> std::io::Result<()> {
        self.track_mut(track).ack(last_seq)
    }

    /// Frames to resend after a reconnect: everything with `seq > last_seq`.
    pub fn resend_after(&self, track: Track, last_seq: i64) -> Vec<Pending> {
        self.track(track).resend_after(last_seq)
    }

    /// Total unacked frames across both tracks (for status/metrics).
    pub fn pending_count(&self) -> usize {
        self.mic.frames.len() + self.meeting.frames.len()
    }
}

fn write_frame<W: Write>(w: &mut W, p: &Pending) -> std::io::Result<()> {
    w.write_u64::<LittleEndian>(p.seq)?;
    w.write_u64::<LittleEndian>(p.timestamp_ms)?;
    w.write_u32::<LittleEndian>(p.opus.len() as u32)?;
    w.write_all(&p.opus)
}

fn read_frames(path: &Path) -> std::io::Result<Vec<Pending>> {
    let bytes = std::fs::read(path)?;
    let mut cur = std::io::Cursor::new(&bytes[..]);
    let mut out = Vec::new();
    // Each iteration reads one record; a short read (clean EOF or a torn trailing
    // record from a crash) ends the loop.
    while let Ok(seq) = cur.read_u64::<LittleEndian>() {
        let Ok(timestamp_ms) = cur.read_u64::<LittleEndian>() else {
            break;
        };
        let Ok(len) = cur.read_u32::<LittleEndian>() else {
            break;
        };
        let mut opus = vec![0u8; len as usize];
        use std::io::Read as _;
        if cur.read_exact(&mut opus).is_err() {
            break; // torn trailing record from a crash — ignore it.
        }
        out.push(Pending {
            seq,
            timestamp_ms,
            opus,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u64) -> Pending {
        Pending {
            seq,
            timestamp_ms: seq * 20,
            opus: vec![seq as u8; 4],
        }
    }

    #[test]
    fn resend_returns_tail_after_ack() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = Outbox::open(dir.path()).unwrap();
        for i in 0..5 {
            ob.record(Track::Mic, frame(i)).unwrap();
        }
        // Gateway acked up to seq 2; resume should resend 3 and 4.
        let resend = ob.resend_after(Track::Mic, 2);
        assert_eq!(resend.iter().map(|p| p.seq).collect::<Vec<_>>(), vec![3, 4]);
    }

    #[test]
    fn ack_prunes_disk_and_memory() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = Outbox::open(dir.path()).unwrap();
        for i in 0..5 {
            ob.record(Track::Meeting, frame(i)).unwrap();
        }
        ob.ack(Track::Meeting, 2).unwrap();
        assert_eq!(ob.pending_count(), 2);

        // Reopen from disk: only the unacked frames remain.
        let ob2 = Outbox::open(dir.path()).unwrap();
        let remaining = ob2.resend_after(Track::Meeting, -1);
        assert_eq!(
            remaining.iter().map(|p| p.seq).collect::<Vec<_>>(),
            vec![3, 4]
        );
    }

    #[test]
    fn survives_a_torn_trailing_record() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut ob = Outbox::open(dir.path()).unwrap();
            ob.record(Track::Mic, frame(0)).unwrap();
            ob.record(Track::Mic, frame(1)).unwrap();
        }
        // Append a half-written record, as a crash mid-write would leave.
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(dir.path().join("outbox-mic.bin"))
                .unwrap();
            f.write_all(&[0xAA, 0xBB, 0xCC]).unwrap(); // < one record
        }
        let ob = Outbox::open(dir.path()).unwrap();
        assert_eq!(ob.resend_after(Track::Mic, -1).len(), 2);
    }

    #[test]
    fn tracks_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let mut ob = Outbox::open(dir.path()).unwrap();
        ob.record(Track::Mic, frame(0)).unwrap();
        ob.record(Track::Meeting, frame(0)).unwrap();
        ob.record(Track::Meeting, frame(1)).unwrap();
        ob.ack(Track::Mic, 0).unwrap();
        assert_eq!(ob.resend_after(Track::Mic, -1).len(), 0);
        assert_eq!(ob.resend_after(Track::Meeting, -1).len(), 2);
    }
}
