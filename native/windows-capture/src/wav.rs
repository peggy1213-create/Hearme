//! Minimal incremental 16-bit PCM WAV writer for the `record` debug subcommand, so a
//! human can listen to `mic.wav` / `meeting.wav`. Converts the capture path's `f32`
//! samples to little-endian `i16`. Not used on the live capture path. Mirrors the macOS
//! helper's `WavWriter`.

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter {
    file: File,
    sample_rate: u32,
    channels: u16,
    data_bytes: u32,
    closed: bool,
}

impl WavWriter {
    pub fn create(path: &Path, sample_rate: u32, channels: u16) -> std::io::Result<WavWriter> {
        let mut file = File::create(path)?;
        let sample_rate = sample_rate.max(1);
        let channels = channels.max(1);
        file.write_all(&header(sample_rate, channels, 0))?;
        Ok(WavWriter {
            file,
            sample_rate,
            channels,
            data_bytes: 0,
            closed: false,
        })
    }

    pub fn append(&mut self, samples: &[f32]) {
        if self.closed {
            return;
        }
        let mut pcm = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let v = (clamped * 32767.0) as i16;
            pcm.extend_from_slice(&v.to_le_bytes());
        }
        if self.file.write_all(&pcm).is_ok() {
            self.data_bytes = self.data_bytes.wrapping_add(pcm.len() as u32);
        }
    }

    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        let _ = self.file.seek(SeekFrom::Start(0));
        let _ = self
            .file
            .write_all(&header(self.sample_rate, self.channels, self.data_bytes));
        let _ = self.file.flush();
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        self.close();
    }
}

fn header(sample_rate: u32, channels: u16, data_bytes: u32) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let block_align = channels * bits_per_sample / 8;
    let byte_rate = sample_rate * block_align as u32;

    let mut d = Vec::with_capacity(44);
    d.extend_from_slice(b"RIFF");
    d.extend_from_slice(&(36u32.wrapping_add(data_bytes)).to_le_bytes());
    d.extend_from_slice(b"WAVE");
    d.extend_from_slice(b"fmt ");
    d.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk size
    d.extend_from_slice(&1u16.to_le_bytes()); // audioFormat = PCM
    d.extend_from_slice(&channels.to_le_bytes());
    d.extend_from_slice(&sample_rate.to_le_bytes());
    d.extend_from_slice(&byte_rate.to_le_bytes());
    d.extend_from_slice(&block_align.to_le_bytes());
    d.extend_from_slice(&bits_per_sample.to_le_bytes());
    d.extend_from_slice(b"data");
    d.extend_from_slice(&data_bytes.to_le_bytes());
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_readable_header() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("tapcapture-wavtest-{}.wav", std::process::id()));
        {
            let mut w = WavWriter::create(&path, 48_000, 1).unwrap();
            w.append(&[0.0, 0.5, -0.5, 1.0]);
            w.close();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // 4 samples * 2 bytes = 8 data bytes.
        let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(data_len, 8);
        let _ = std::fs::remove_file(&path);
    }
}
