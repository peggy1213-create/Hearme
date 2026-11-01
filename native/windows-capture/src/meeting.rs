//! Meeting track: per-process loopback capture via `ActivateAudioInterfaceAsync` with
//! `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` and
//! `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`, so the target app's root process
//! **and its children** (Teams' WebView2 processes, browser renderers) are captured.
//! Mixed down to mono before framing. Requires Windows 10 Build 20348+ (see
//! `docs/decisions/0004-windows-audio-capture.md`); older builds surface as
//! `MeetingError::Unsupported`.

use std::mem::size_of;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;

use windows::core::{implement, w, IUnknown, Interface, HRESULT, PCWSTR, PROPVARIANT};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use crate::clock;
use crate::mic::FrameCallback;
use crate::wasapi::{
    close_event, create_event, downmix_to_mono, float_format, run_capture_loop, Capture,
    ComApartment, FormatInfo, SampleKind,
};

/// The virtual device path for process-loopback activation (`VAD\Process_Loopback`).
const VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK: PCWSTR = w!("VAD\\Process_Loopback");

/// We request 32-bit float; rate/channels are what the loopback client is asked to
/// produce (it has no mix format of its own).
const LOOPBACK_SAMPLE_RATE: u32 = 48_000;
const LOOPBACK_CHANNELS: u16 = 2;

#[derive(Debug)]
pub enum MeetingError {
    /// Process loopback is unavailable — most likely a Windows build older than 20348.
    Unsupported(String),
    Failed(String),
}

/// Start process-loopback capture of `root_pid`'s process tree on its own thread.
pub fn start(
    root_pid: u32,
    mut on_frame: FrameCallback,
) -> Result<(Capture, FormatInfo), MeetingError> {
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let (tx, rx) = mpsc::channel::<Result<FormatInfo, MeetingError>>();

    let join = std::thread::Builder::new()
        .name("tapcapture-meeting".into())
        .spawn(move || {
            let _com = ComApartment::new();

            let (audio_client, capture_client, event, format) = match unsafe { setup(root_pid) } {
                Ok(parts) => parts,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            let _ = tx.send(Ok(format));

            let channels = format.channels;
            let rate = format.sample_rate;
            let mut emit = move |samples: &[f32]| {
                let mono = downmix_to_mono(samples, channels);
                on_frame(&mono, clock::now_ns(), rate, 1);
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
        .map_err(|e| MeetingError::Failed(e.to_string()))?;

    match rx.recv() {
        Ok(Ok(format)) => Ok((Capture::new(stop, join), format)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(MeetingError::Failed(
            "meeting thread exited during setup".into(),
        )),
    }
}

/// Completion handler for `ActivateAudioInterfaceAsync`: signals an event when the async
/// activation finishes, so the setup thread can read the result.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct CompletionHandler {
    done: HANDLE,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for CompletionHandler_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Option<&IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        unsafe {
            let _ = SetEvent(self.done);
        }
        Ok(())
    }
}

/// VT_BLOB `PROPVARIANT` laid out by hand (the `windows_core` `PROPVARIANT` is opaque).
/// On all supported targets this matches the OS `PROPVARIANT` for a blob: a 2-byte
/// `vt`, three reserved `u16`s, then the 8-byte-aligned `BLOB` union member.
#[repr(C)]
struct PropVariantBlob {
    vt: u16,
    r1: u16,
    r2: u16,
    r3: u16,
    blob: BLOB,
}

/// Activate the process-loopback client and initialize it. Separated for readability.
///
/// # Safety
/// Calls COM/WASAPI; must run on a COM-initialized thread.
unsafe fn setup(
    root_pid: u32,
) -> Result<(IAudioClient, IAudioCaptureClient, HANDLE, FormatInfo), MeetingError> {
    let audio_client = activate_loopback_client(root_pid)?;

    let format = float_format(LOOPBACK_SAMPLE_RATE, LOOPBACK_CHANNELS);
    // Process loopback uses shared mode with the LOOPBACK + event-callback flags.
    audio_client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            2_000_000, // 200 ms buffer (REFERENCE_TIME, 100 ns units)
            0,
            &*format,
            None,
        )
        .map_err(|e| MeetingError::Failed(format!("Initialize 0x{:08X}", e.code().0 as u32)))?;

    let event = create_event().map_err(|e| MeetingError::Failed(e.to_string()))?;
    audio_client
        .SetEventHandle(event)
        .map_err(|e| MeetingError::Failed(e.to_string()))?;
    let capture_client: IAudioCaptureClient = audio_client
        .GetService()
        .map_err(|e| MeetingError::Failed(e.to_string()))?;

    let info = FormatInfo {
        channels: LOOPBACK_CHANNELS,
        sample_rate: LOOPBACK_SAMPLE_RATE,
        bits: 32,
        kind: SampleKind::Float32,
    };
    Ok((audio_client, capture_client, event, info))
}

/// Drive `ActivateAudioInterfaceAsync` to completion and return the `IAudioClient`.
///
/// # Safety
/// Calls COM/WASAPI on a COM-initialized thread.
unsafe fn activate_loopback_client(root_pid: u32) -> Result<IAudioClient, MeetingError> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: root_pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };

    // Wrap the params struct in a VT_BLOB PROPVARIANT. Both `params` and `prop` must
    // outlive the activation call below, which they do (same scope).
    let prop = PropVariantBlob {
        vt: VT_BLOB.0,
        r1: 0,
        r2: 0,
        r3: 0,
        blob: BLOB {
            cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: &params as *const _ as *mut u8,
        },
    };
    let prop_ptr = &prop as *const PropVariantBlob as *const PROPVARIANT;

    let done =
        CreateEventW(None, false, false, None).map_err(|e| MeetingError::Failed(e.to_string()))?;
    let handler: IActivateAudioInterfaceCompletionHandler = CompletionHandler { done }.into();

    let operation: IActivateAudioInterfaceAsyncOperation = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(prop_ptr),
        &handler,
    )
    .map_err(unsupported_or_failed)?;

    // Wait for the completion handler to signal (5 s is generous).
    WaitForSingleObject(done, 5_000);
    let _ = CloseHandle(done);

    let mut activate_hr = HRESULT(0);
    let mut unknown: Option<IUnknown> = None;
    operation
        .GetActivateResult(&mut activate_hr, &mut unknown)
        .map_err(unsupported_or_failed)?;
    activate_hr.ok().map_err(unsupported_or_failed)?;

    let unknown = unknown.ok_or_else(|| MeetingError::Failed("no activated interface".into()))?;
    unknown
        .cast::<IAudioClient>()
        .map_err(|e| MeetingError::Failed(e.to_string()))
}

/// Activation failures on builds without process loopback show up as E_NOTFOUND /
/// E_NOINTERFACE-style errors; treat an activation failure as "unsupported build" with
/// the build requirement in the message, which the session relays.
fn unsupported_or_failed(e: windows::core::Error) -> MeetingError {
    MeetingError::Unsupported(format!(
        "process loopback unavailable (0x{:08X}); requires Windows 10 build 20348 or newer",
        e.code().0 as u32
    ))
}
