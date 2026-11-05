//! Writing mono 48 kHz audio as Opus in an Ogg container (`mic.ogg`, `meeting.ogg`,
//! `mixed.ogg`), and repairing a file that a crash left half-written.
//!
//! Audio is encoded in 20 ms Opus frames (960 samples at 48 kHz). The writer keeps
//! the newest packet un-flushed so that, at finalize, it can be the one flagged
//! end-of-stream; `flush()` forces every earlier packet's page to disk so a crash
//! loses at most the last frame plus whatever is still buffered (well under the 10 s
//! durability budget when the driver flushes every few seconds).

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use audiopus::coder::Encoder;
use audiopus::{Application, Channels, SampleRate};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};

/// Samples per Opus frame: 20 ms at 48 kHz, mono.
const FRAME_SAMPLES: usize = 960;
const OUT_CAP: usize = 4000;

pub struct OggOpusWriter {
    writer: PacketWriter<'static, BufWriter<File>>,
    encoder: Encoder,
    serial: u32,
    /// Cumulative 48 kHz samples encoded; the Ogg granule position.
    granule: u64,
    /// Input samples not yet forming a whole 20 ms frame.
    frame_buf: Vec<f32>,
    /// Encoded packets not yet written. The last is held back so finalize can flag
    /// end-of-stream on it.
    queue: std::collections::VecDeque<(Vec<u8>, u64)>,
    finished: bool,
}

impl OggOpusWriter {
    /// Create `path` and write the OpusHead + OpusTags headers.
    pub fn create(path: &Path) -> anyhow::Result<Self> {
        let file = File::create(path)?;
        let encoder = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip)?;
        let pre_skip = encoder.lookahead().unwrap_or(312).min(u16::MAX as u32) as u16;
        let serial = gen_serial();
        let mut writer = PacketWriter::new(BufWriter::new(file));

        writer.write_packet(
            opus_head(1, pre_skip, 48_000),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )?;
        writer.write_packet(opus_tags(), serial, PacketWriteEndInfo::EndPage, 0)?;

        Ok(Self {
            writer,
            encoder,
            serial,
            granule: 0,
            frame_buf: Vec::with_capacity(FRAME_SAMPLES * 2),
            queue: std::collections::VecDeque::new(),
            finished: false,
        })
    }

    /// Append mono 48 kHz samples. Encodes whole 20 ms frames; a tail shorter than a
    /// frame stays buffered.
    pub fn write_samples(&mut self, samples: &[f32]) -> anyhow::Result<()> {
        self.frame_buf.extend_from_slice(samples);
        while self.frame_buf.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.frame_buf.drain(..FRAME_SAMPLES).collect();
            self.encode_frame(&frame)?;
        }
        Ok(())
    }

    fn encode_frame(&mut self, frame: &[f32]) -> anyhow::Result<()> {
        let mut out = vec![0u8; OUT_CAP];
        let n = self.encoder.encode_float(frame, &mut out)?;
        out.truncate(n);
        self.granule += FRAME_SAMPLES as u64;
        self.queue.push_back((out, self.granule));
        Ok(())
    }

    /// Force all but the newest buffered packet to disk. Call at least every 10 s.
    pub fn flush(&mut self) -> anyhow::Result<()> {
        // Keep the last packet queued so finalize can mark it end-of-stream.
        while self.queue.len() > 1 {
            let (data, absgp) = self.queue.pop_front().unwrap();
            let end = if self.queue.len() == 1 {
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.writer.write_packet(data, self.serial, end, absgp)?;
        }
        let inner = self.writer.inner_mut();
        inner.flush()?;
        inner.get_ref().sync_all()?;
        Ok(())
    }

    /// Flush the rest, flag end-of-stream, and sync. Safe to call once.
    pub fn finalize(mut self) -> anyhow::Result<()> {
        self.finalize_mut()
    }

    fn finalize_mut(&mut self) -> anyhow::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;

        // Encode any partial trailing frame (pad with silence to a whole frame).
        if !self.frame_buf.is_empty() {
            let mut frame = std::mem::take(&mut self.frame_buf);
            frame.resize(FRAME_SAMPLES, 0.0);
            self.encode_frame(&frame)?;
        }
        // A meeting with literally no audio still needs a valid, closed stream.
        if self.queue.is_empty() {
            self.encode_frame(&vec![0.0f32; FRAME_SAMPLES])?;
        }

        while let Some((data, absgp)) = self.queue.pop_front() {
            let end = if self.queue.is_empty() {
                PacketWriteEndInfo::EndStream
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.writer.write_packet(data, self.serial, end, absgp)?;
        }
        let inner = self.writer.inner_mut();
        inner.flush()?;
        inner.get_ref().sync_all()?;
        Ok(())
    }
}

impl Drop for OggOpusWriter {
    fn drop(&mut self) {
        // Best-effort close if the caller forgot to finalize.
        let _ = self.finalize_mut();
    }
}

/// OpusHead identification header (RFC 7845 §5.1), mono, channel mapping family 0.
fn opus_head(channels: u8, pre_skip: u16, input_rate: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(19);
    v.extend_from_slice(b"OpusHead");
    v.push(1); // version
    v.push(channels);
    v.extend_from_slice(&pre_skip.to_le_bytes());
    v.extend_from_slice(&input_rate.to_le_bytes());
    v.extend_from_slice(&0i16.to_le_bytes()); // output gain
    v.push(0); // channel mapping family
    v
}

/// OpusTags comment header (RFC 7845 §5.2) with a vendor string and no comments.
fn opus_tags() -> Vec<u8> {
    let vendor = b"hearme";
    let mut v = Vec::new();
    v.extend_from_slice(b"OpusTags");
    v.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    v.extend_from_slice(vendor);
    v.extend_from_slice(&0u32.to_le_bytes()); // user comment list length
    v
}

fn gen_serial() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(1);
    nanos ^ 0x4865_6172 // "Hear"
}

/// Repair an Ogg file a crash left with a half-written final page: truncate it to the
/// end of the last **complete** page. Returns the number of bytes dropped. A file
/// that is already whole is left untouched (returns 0). Players tolerate the missing
/// end-of-stream flag; the final batch pass re-reads the audio anyway.
pub fn repair_ogg(path: &Path) -> anyhow::Result<u64> {
    let data = std::fs::read(path)?;
    let good = last_complete_page_end(&data);
    if good == data.len() {
        return Ok(0);
    }
    let file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.set_len(good as u64)?;
    file.sync_all()?;
    let mut f = file;
    f.seek(SeekFrom::End(0))?;
    Ok((data.len() - good) as u64)
}

/// Byte offset just past the last complete Ogg page in `data`. Walks page headers
/// (capture pattern `OggS`) and stops at the first page that runs off the end.
fn last_complete_page_end(data: &[u8]) -> usize {
    let mut pos = 0usize;
    let mut last_good = 0usize;
    while pos + 27 <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            // Not at a page boundary — stop; everything before is intact.
            break;
        }
        let page_segments = data[pos + 26] as usize;
        let seg_table_end = pos + 27 + page_segments;
        if seg_table_end > data.len() {
            break; // segment table itself is truncated
        }
        let body_len: usize = data[pos + 27..seg_table_end]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let page_end = seg_table_end + body_len;
        if page_end > data.len() {
            break; // body is truncated
        }
        last_good = page_end;
        pos = page_end;
    }
    last_good
}

/// Test-only accessor so sibling modules can assert a file is whole pages.
#[cfg(test)]
pub fn tests_last_page_end(data: &[u8]) -> usize {
    last_complete_page_end(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_playable_header_and_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mic.ogg");
        let mut w = OggOpusWriter::create(&path).unwrap();
        // 0.5 s of tone.
        let tone: Vec<f32> = (0..24_000).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        w.write_samples(&tone).unwrap();
        w.finalize().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.len() > 1000);
        assert_eq!(&bytes[0..4], b"OggS"); // first page
                                           // OpusHead lives in the first page body.
        assert!(
            bytes.windows(8).any(|w| w == b"OpusHead"),
            "missing OpusHead"
        );
        assert!(bytes.windows(8).any(|w| w == b"OpusTags"));
        // The whole file is a sequence of complete pages.
        assert_eq!(last_complete_page_end(&bytes), bytes.len());
    }

    #[test]
    fn repair_truncates_a_half_written_final_page() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meeting.ogg");
        let mut w = OggOpusWriter::create(&path).unwrap();
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.04).sin() * 0.3).collect();
        w.write_samples(&tone).unwrap();
        w.flush().unwrap(); // whole pages on disk
        drop(w); // finalize writes the last page too

        let whole = std::fs::read(&path).unwrap();
        let good = last_complete_page_end(&whole);
        assert_eq!(good, whole.len());

        // Simulate a crash mid-page: append garbage that looks like a new page start
        // but is truncated.
        {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(b"OggS\x00\x02\x00\x00").unwrap(); // partial header
        }
        let dropped = repair_ogg(&path).unwrap();
        assert!(dropped > 0, "repair should have dropped the partial page");
        let repaired = std::fs::read(&path).unwrap();
        assert_eq!(repaired.len(), good);
        assert_eq!(last_complete_page_end(&repaired), repaired.len());
    }

    #[test]
    fn repair_is_noop_on_a_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.ogg");
        let mut w = OggOpusWriter::create(&path).unwrap();
        w.write_samples(&vec![0.0f32; 9600]).unwrap();
        w.finalize().unwrap();
        assert_eq!(repair_ogg(&path).unwrap(), 0);
    }
}
