# ADR 0004: Windows audio capture via WASAPI process loopback

- Status: accepted
- Date: 2026-11-01

## Context

Hearme must capture a meeting app's output audio plus the microphone on Windows
**without joining the meeting** and without installing a virtual audio driver or a
kernel-mode filter (CLAUDE.md). The desktop core (Tauri) runs a native helper as a
sidecar to do the capture, and it must behave identically to the macOS helper
(`native/macos-capture`, ADR 0003): same stdin commands, same length-prefixed binary
stdout frames, same events and the same `record` debug subcommand, so the core treats
both interchangeably (`packages/shared/CAPTURE_HELPER.md`).

Windows exposes **per-process loopback capture** through `ActivateAudioInterfaceAsync`
with an `AUDIOCLIENT_ACTIVATION_PARAMS` whose `ActivationType` is
`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`. The params carry a target process id
and a `PROCESS_LOOPBACK_MODE`:

- `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE` — capture the render audio of the
  target process **and all its children**. This is what we want: the new Microsoft
  Teams (`ms-teams.exe`) renders audio through WebView2 child processes, and browsers
  render Meet audio in child renderer processes, so capturing the whole tree covers
  them with a single activation on the app's root PID.
- `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE` — the inverse; not used here.

Reference: Microsoft's [Application Loopback API Capture Sample](https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/)
and the [`AUDIOCLIENT_ACTIVATION_TYPE`](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-audioclient_activation_type)
/ [`PROCESS_LOOPBACK_MODE`](https://learn.microsoft.com/en-us/windows/win32/api/audioclientactivationparams/ne-audioclientactivationparams-process_loopback_mode)
enumerations.

The activation is unusual compared with ordinary WASAPI: it uses an event-driven
shared-mode `IAudioClient` initialized with `AUDCLNT_STREAMFLAGS_LOOPBACK |
AUDCLNT_STREAMFLAGS_EVENTCALLBACK`, and the audio format must be set explicitly (the
loopback endpoint has no "mix format" to query); the sample uses 32-bit float PCM,
which is exactly the format the Hearme frame protocol carries.

### Minimum Windows build

Microsoft documents the minimum supported client for
`AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` (and the `PROCESS_LOOPBACK_MODE` enum
and `AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`) as **Windows 10 Build 20348**. Earlier
Windows 10 builds have `ActivateAudioInterfaceAsync` but not the process-loopback
activation type, and the activation fails on them. Practically this covers Windows 11
(all builds) and fully-updated Windows 10 21H2+; it does **not** cover older Windows 10
releases. The helper must detect this and report it clearly rather than crash. (This
development machine is build 26300, well above the minimum.)

## Decision

- **`tapcapture.exe`**, a Rust CLI in `native/windows-capture` using the
  [`windows`](https://crates.io/crates/windows) crate, is the helper. It only captures
  and reports audio; recording, encoding and streaming stay in the core. The binary is
  renamed from the `hearme-windows-capture` stub to `tapcapture` to match the macOS
  helper and the prompt; the Cargo package keeps its name.
- **Meeting-app detection:** enumerate active audio sessions via
  `IAudioSessionManager2::GetSessionEnumerator` on the default render endpoint to learn
  which processes are rendering audio, resolve each session's PID
  (`IAudioSessionControl2::GetProcessId`) to an executable name, and match against a
  shared `known-apps.json` (exe-name based on Windows, mirroring the macOS bundle-id
  config). A 1 s monitor emits `source_started` / `source_stopped` on
  producing-audio transitions, exactly like macOS. The helper never starts capture on
  its own.
- **Meeting track:** one process-loopback `IAudioClient` activated on the chosen app's
  **root PID** with `INCLUDE_TARGET_PROCESS_TREE`, event-driven, format 32-bit float,
  mixed down to **mono** before framing (`channels = 1`), matching the protocol.
- **Mic track:** an ordinary WASAPI **shared-mode** capture client on the selected or
  default capture endpoint, emitted at its native format as a separate track.
- **One shared clock:** both tracks stamp frames with `QueryPerformanceCounter` scaled
  to nanoseconds via `QueryPerformanceFrequency`, so the core can align them. Frame
  layout, commands and events are byte-for-byte the macOS protocol
  (`CAPTURE_HELPER_PROTOCOL_VERSION = 1`).
- **Device changes, sleep/wake, app restart:** watch for default-endpoint changes
  (`IMMNotificationClient::OnDefaultDeviceChanged`) and re-point the mic capture,
  emitting `device_changed`; the 1 s monitor re-targets the meeting app's new root PID
  if it restarts mid-call; capture is torn down cleanly on `stop`, on Ctrl-C /
  console-close, and on stdin EOF.
- **Permissions:** detect the Windows **microphone privacy** setting blocking capture
  (an `AUDCLNT_E_*` / `E_ACCESSDENIED` failure when initializing the mic client, or the
  registry consent value) and emit `{"event":"mic_blocked"}` with a short, content-free
  message. Process loopback itself needs no special permission. As on macOS, a track
  that delivers several seconds of exact-zero audio while its source is known to be
  playing fires `permission_suspected_missing` via a shared `SilenceGuard`.
- **Unsupported builds:** if the process-loopback activation fails on an older build,
  emit a clear `error` (not a panic) naming the build requirement.
- **`record` debug subcommand:** `tapcapture.exe record --process zoom.exe --seconds 30
--out .\out` writes `mic.wav` and `meeting.wav` so a human can confirm the two tracks
  are clean and aligned. (The macOS helper selects by `--bundle`; Windows selects by
  `--process` exe name, per the prompt.)

## Consequences

- No virtual audio device and no driver install; capture works per-app across Zoom,
  Teams, Webex and browsers for Meet with one mechanism, and
  `INCLUDE_TARGET_PROCESS_TREE` means we activate on one root PID instead of tracking
  every child renderer ourselves.
- Windows 10 builds older than 20348 are unsupported for meeting capture; the helper
  reports this rather than failing opaquely. This is an accepted limitation.
- The exe names in `known-apps.json` (notably `ms-teams.exe` for new Teams, and the
  browser exes) and the real set of audio-rendering child processes must be verified on
  real machines and kept current, the Windows analogue of the macOS bundle-id caveat.
- The `windows` crate pulls the Win32 audio, COM and session APIs; the crate's feature
  list is pinned to the interfaces used so build times stay reasonable.

## Alternatives considered

- **Full-system loopback** (`AUDCLNT_STREAMFLAGS_LOOPBACK` on the default render
  endpoint): captures everything playing — notifications, music, other apps — and
  cannot isolate the meeting. Kept only as a conceivable fallback where a specific
  app's audio cannot be resolved.
- **Virtual audio cable / driver:** needs an installer and a signed driver, is fragile
  across OS updates, and routes audio away from the user's speakers unless looped back.
  Heavier and more invasive than process loopback.
- **A meeting bot / Graph media APIs:** explicitly forbidden by CLAUDE.md — the app
  must never join the meeting.
