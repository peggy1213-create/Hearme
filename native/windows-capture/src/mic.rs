//! Microphone track: ordinary WASAPI **shared-mode**, event-driven capture of the
//! selected or default input endpoint, emitted at its native channel count (the core
//! downmixes if it wants). A privacy-setting denial is surfaced as `MicError::Blocked`
//! so the session can emit `mic_blocked`.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use windows::core::HSTRING;
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

use crate::clock;
use crate::wasapi::{
    close_event, create_event, run_capture_loop, Capture, ComApartment, FormatInfo,
};

/// Callback invoked with each captured buffer: interleaved `f32`, host clock ns,
/// sample rate, channel count.
pub type FrameCallback = Box<dyn FnMut(&[f32], u64, u32, u8) + Send>;

#[derive(Debug)]
pub enum MicError {
    /// Windows microphone privacy setting (or an app-level consent denial) blocks
    /// capture — the UI should ask the user to allow microphone access.
    Blocked,
    Failed(String),
}

/// Start microphone capture on its own thread. Returns once capture has started (or an
/// error occurred during setup), with the running `Capture` and the negotiated format.
pub fn start(
    mic_device_id: Option<String>,
    mut on_frame: FrameCallback,
) -> Result<(Capture, FormatInfo), MicError> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let (tx, rx) = mpsc::channel::<Result<FormatInfo, MicError>>();

    let join = std::thread::Builder::new()
        .name("tapcapture-mic".into())
        .spawn(move || {
            let _com = ComApartment::new();

            let (audio_client, capture_client, event, format) =
                match unsafe { setup(mic_device_id.as_deref()) } {
                    Ok(parts) => parts,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
            let _ = tx.send(Ok(format));

            let channels = format.channels.max(1) as u8;
            let rate = format.sample_rate;
            let mut emit = move |samples: &[f32]| {
                on_frame(samples, clock::now_ns(), rate, channels);
            };

            let _ = unsafe {
                run_capture_loop(
                    &audio_client,
                    &capture_client,
                    event,
                    format,
                    &thread_stop,
                    &mut emit,
                )
            };
            unsafe { close_event(event) };
        })
        .map_err(|e| MicError::Failed(e.to_string()))?;

    match rx.recv() {
        Ok(Ok(format)) => Ok((Capture::new(stop, join), format)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(MicError::Failed("mic thread exited during setup".into())),
    }
}

/// Build and initialize the mic capture client. Separated so the thread body stays
/// readable.
///
/// # Safety
/// Calls COM/WASAPI; must run on a COM-initialized thread.
unsafe fn setup(
    mic_device_id: Option<&str>,
) -> Result<
    (
        IAudioClient,
        IAudioCaptureClient,
        windows::Win32::Foundation::HANDLE,
        FormatInfo,
    ),
    MicError,
> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(map_err)?;

    let device = match mic_device_id {
        Some(id) => enumerator.GetDevice(&HSTRING::from(id)).map_err(map_err)?,
        None => enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .map_err(map_err)?,
    };

    let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(map_err)?;
    let mix = audio_client.GetMixFormat().map_err(map_err)?;
    let format = FormatInfo::from_ptr(mix);

    let init = audio_client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        0,
        0,
        mix,
        None,
    );
    // Free the mix format now that Initialize has copied it.
    CoTaskMemFree(Some(mix as *const _));
    init.map_err(map_err)?;

    let event = create_event().map_err(map_err)?;
    audio_client.SetEventHandle(event).map_err(map_err)?;
    let capture_client: IAudioCaptureClient = audio_client.GetService().map_err(map_err)?;

    Ok((audio_client, capture_client, event, format))
}

fn map_err(e: windows::core::Error) -> MicError {
    if e.code() == E_ACCESSDENIED {
        MicError::Blocked
    } else {
        MicError::Failed(format!("0x{:08X}", e.code().0 as u32))
    }
}
