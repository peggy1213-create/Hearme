//! Shared WASAPI / COM building blocks: apartment init, an owned float `WAVEFORMATEX`,
//! parsing a device mix format, reading capture packets to interleaved `f32`, and a
//! mono downmix. Kept separate so `mic`, `meeting` and `sessions` read cleanly.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use windows::core::Result;
use windows::Win32::Foundation::{CloseHandle, HANDLE, S_FALSE, S_OK};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

// `wFormatTag` values (mmreg.h). Defined locally to avoid pulling extra features.
const TAG_PCM: u16 = 1;
const TAG_IEEE_FLOAT: u16 = 3;
const TAG_EXTENSIBLE: u16 = 0xFFFE;

/// RAII COM apartment for the current thread. Each capture thread and the session
/// enumeration init COM; `Drop` uninitializes. `RPC_E_CHANGED_MODE` / `S_FALSE` (already
/// initialized) are treated as success.
pub struct ComApartment {
    owned: bool,
}

impl ComApartment {
    pub fn new() -> ComApartment {
        // Safe: CoInitializeEx is idempotent per thread; we only CoUninitialize what we
        // successfully initialized.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let owned = hr == S_OK || hr == S_FALSE;
        ComApartment { owned }
    }
}

impl Default for ComApartment {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owned {
            unsafe { CoUninitialize() };
        }
    }
}

/// The sample format a capture stream delivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleKind {
    Float32,
    Int16,
    Unsupported,
}

/// A parsed, flattened view of a `WAVEFORMATEX(TENSIBLE)`.
#[derive(Debug, Clone, Copy)]
pub struct FormatInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
    pub kind: SampleKind,
}

impl FormatInfo {
    /// Parse a `WAVEFORMATEX` pointer (optionally a `WAVEFORMATEXTENSIBLE`). The
    /// effective tag of an EXTENSIBLE format is the low 16 bits of its SubFormat GUID
    /// `Data1` (1 = PCM, 3 = IEEE float).
    ///
    /// # Safety
    /// `ptr` must point to a valid `WAVEFORMATEX`; if `wFormatTag` is EXTENSIBLE it must
    /// actually be a `WAVEFORMATEXTENSIBLE`.
    pub unsafe fn from_ptr(ptr: *const WAVEFORMATEX) -> FormatInfo {
        let wfx = &*ptr;
        let mut tag = wfx.wFormatTag;
        if tag == TAG_EXTENSIBLE {
            let ext = &*(ptr as *const WAVEFORMATEXTENSIBLE);
            tag = (ext.SubFormat.data1 & 0xFFFF) as u16;
        }
        let kind = match tag {
            TAG_IEEE_FLOAT if wfx.wBitsPerSample == 32 => SampleKind::Float32,
            TAG_PCM if wfx.wBitsPerSample == 16 => SampleKind::Int16,
            _ => SampleKind::Unsupported,
        };
        FormatInfo {
            channels: wfx.nChannels,
            sample_rate: wfx.nSamplesPerSec,
            bits: wfx.wBitsPerSample,
            kind,
        }
    }
}

/// Build an owned 32-bit-float `WAVEFORMATEX` for the given rate/channels. Used to
/// request float output from the process-loopback client (which has no mix format of
/// its own).
pub fn float_format(sample_rate: u32, channels: u16) -> Box<WAVEFORMATEX> {
    let block_align = channels * 4;
    Box::new(WAVEFORMATEX {
        wFormatTag: TAG_IEEE_FLOAT,
        nChannels: channels,
        nSamplesPerSec: sample_rate,
        nAvgBytesPerSec: sample_rate * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: 32,
        cbSize: 0,
    })
}

/// Downmix interleaved `channels`-channel `f32` audio to mono by averaging.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    let frames = interleaved.len() / ch;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let base = f * ch;
        let sum: f32 = interleaved[base..base + ch].iter().sum();
        out.push(sum / ch as f32);
    }
    out
}

/// Create an auto-reset, initially-unsignaled event for event-driven WASAPI capture.
pub fn create_event() -> Result<HANDLE> {
    unsafe { CreateEventW(None, false, false, None) }
}

/// A running capture on its own thread. Dropping or calling `stop()` signals the thread
/// to finish and joins it.
pub struct Capture {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Capture {
    pub fn new(stop: Arc<AtomicBool>, join: JoinHandle<()>) -> Capture {
        Capture {
            stop,
            join: Some(join),
        }
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Event-driven capture loop: start the client, then wake on the event (with a 100 ms
/// timeout so `stop` is observed promptly), drain packets and hand interleaved `f32` to
/// `on_samples`, until `stop` is set. Stops the client on exit.
///
/// # Safety
/// `audio_client` / `capture` must be an initialized, matching event-driven capture
/// pair, and `event` the handle passed to `SetEventHandle`.
pub unsafe fn run_capture_loop(
    audio_client: &IAudioClient,
    capture: &IAudioCaptureClient,
    event: HANDLE,
    format: FormatInfo,
    stop: &AtomicBool,
    on_samples: &mut dyn FnMut(&[f32]),
) -> Result<()> {
    audio_client.Start()?;
    while !stop.load(Ordering::Relaxed) {
        WaitForSingleObject(event, 100);
        let samples = drain_packets(capture, format)?;
        if !samples.is_empty() {
            on_samples(&samples);
        }
    }
    let _ = audio_client.Stop();
    Ok(())
}

/// Close an event handle created with `create_event`.
///
/// # Safety
/// `event` must be a handle not used after this call.
pub unsafe fn close_event(event: HANDLE) {
    let _ = CloseHandle(event);
}

/// Drain all currently available packets from a capture client into interleaved `f32`.
/// A `SILENT` packet contributes exact zeros (the macOS/Windows missing-permission
/// symptom the `SilenceGuard` watches for). Returns the interleaved samples.
///
/// # Safety
/// `client` must be an initialized capture client whose stream format matches `format`.
pub unsafe fn drain_packets(client: &IAudioCaptureClient, format: FormatInfo) -> Result<Vec<f32>> {
    let channels = format.channels.max(1) as usize;
    let mut out: Vec<f32> = Vec::new();

    loop {
        let packet = client.GetNextPacketSize()?;
        if packet == 0 {
            break;
        }
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut num_frames: u32 = 0;
        let mut flags: u32 = 0;
        client.GetBuffer(&mut data, &mut num_frames, &mut flags, None, None)?;

        let frames = num_frames as usize;
        let total = frames * channels;
        let silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;

        if silent || data.is_null() {
            out.extend(std::iter::repeat(0.0f32).take(total));
        } else {
            match format.kind {
                SampleKind::Float32 => {
                    let src = std::slice::from_raw_parts(data as *const f32, total);
                    out.extend_from_slice(src);
                }
                SampleKind::Int16 => {
                    let src = std::slice::from_raw_parts(data as *const i16, total);
                    out.extend(src.iter().map(|&s| s as f32 / 32768.0));
                }
                SampleKind::Unsupported => {
                    out.extend(std::iter::repeat(0.0f32).take(total));
                }
            }
        }

        client.ReleaseBuffer(num_frames)?;
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_stereo() {
        let stereo = [1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        assert_eq!(downmix_to_mono(&stereo, 2), vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn downmix_mono_passthrough() {
        let mono = [0.1, 0.2, 0.3];
        assert_eq!(downmix_to_mono(&mono, 1), mono.to_vec());
    }

    #[test]
    fn float_format_fields() {
        // WAVEFORMATEX is a packed struct; copy fields into locals before asserting so
        // we never take a reference to an unaligned field.
        let f = float_format(48_000, 2);
        let channels = f.nChannels;
        let rate = f.nSamplesPerSec;
        let bits = f.wBitsPerSample;
        let block_align = f.nBlockAlign;
        let avg_bytes = f.nAvgBytesPerSec;
        assert_eq!(channels, 2);
        assert_eq!(rate, 48_000);
        assert_eq!(bits, 32);
        assert_eq!(block_align, 8);
        assert_eq!(avg_bytes, 48_000 * 8);
    }
}
