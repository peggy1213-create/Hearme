# tapcapture — Hearme macOS audio capture helper

A Swift command-line helper for **macOS 14.2+** that the desktop core runs as a
sidecar. It captures a meeting app's output audio (Core Audio process taps) plus the
microphone as a separate, time-aligned track, **without joining the meeting**. It only
captures and reports audio; recording, encoding and streaming live in the Tauri core.

- Protocol (commands, binary frames, events): [`../../packages/shared/CAPTURE_HELPER.md`](../../packages/shared/CAPTURE_HELPER.md)
- Design decision and risks: [`../../docs/decisions/0003-macos-audio-capture.md`](../../docs/decisions/0003-macos-audio-capture.md)

## Build and test (macOS only)

```bash
cd native/macos-capture
swift build
swift test
```

`swift test` runs the frame-codec, config-matching, silence-guard and protocol unit
tests headlessly. The end-to-end tone test is opt-in and needs audio permission:

```bash
HEARME_RUN_AUDIO_TESTS=1 swift test --filter ToneCaptureTests
```

## Run

Protocol mode (what the core launches) — JSON commands on stdin, binary frames on
stdout:

```bash
swift run tapcapture
```

Debug recording — capture a bundle's output and the mic for 30 s into `./out`:

```bash
swift run tapcapture record --bundle us.zoom.xos --seconds 30 --out ./out
# writes ./out/meeting.wav and ./out/mic.wav — listen with afplay
```

## Permissions (read this before wondering why audio is silent)

Capturing a tap requires `NSAudioCaptureUsageDescription`; the mic requires
`NSMicrophoneUsageDescription`. Both must be in the **app bundle's** Info.plist.

macOS attributes the permission to the **responsible process**. When `tapcapture` runs
as a sidecar inside the signed Hearme `.app`, the responsible process should resolve to
the app, so the user gets one prompt attributed to Hearme. **Running `tapcapture`
directly from a terminal during development is attributed to the terminal**, which has
no usage key — the request is denied and the tap delivers **all-zero** buffers instead
of an error. The helper detects this and emits `permission_suspected_missing`.

The system prompt appears on the **first IO start** of the aggregate device (i.e. at
`start`/`record`), not at launch.

> **To verify on a real Mac (not yet done — this repo was authored on Windows):**
> confirm the permission is attributed to the host app when `tapcapture` is a Tauri
> child process, and confirm the `known-apps.json` bundle IDs and the set of
> audio-emitting child processes (browser renderers) for each app. Check bundle IDs
> with e.g. `osascript -e 'id of app "Google Chrome"'` or `lsappinfo info -only bundleid <pid>`,
> and list current audio processes with `tapcapture record`'s error output.

## Layout

| File                | Role                                                                 |
| ------------------- | -------------------------------------------------------------------- |
| `main.swift`        | Arg dispatch: protocol mode / `record` / `--help`.                   |
| `Session.swift`     | Protocol mode: commands, source monitor, device changes, lifecycle.  |
| `Frame.swift`       | Length-prefixed binary frame encode/decode.                          |
| `Protocol.swift`    | stdin command parsing, event JSON builders.                          |
| `Clock.swift`       | The one shared host clock (ns) for both tracks.                      |
| `ProcessList.swift` | Core Audio HAL process enumeration + property helpers.               |
| `KnownApps.swift` / `known-apps.json` | Meeting apps and their bundle-ID prefixes.         |
| `MeetingTap.swift`  | Process tap + private aggregate device (meeting track).              |
| `MicCapture.swift`  | `AVAudioEngine` microphone track.                                    |
| `SilenceGuard.swift`| All-zero detection → `permission_suspected_missing`.                 |
| `DeviceWatcher.swift`| Default-device change and sleep/wake.                               |
| `Cleanup.swift`     | Tap/device teardown and orphan reaping; signal handlers.             |
| `WavWriter.swift` / `Recorder.swift` | `record` debug subcommand.                          |

## Status

Authored on Windows against Apple's current docs; **not yet compiled or run on a Mac**.
Expect a small fixup pass against the real SDK (Core Audio key constants, AVAudioEngine
device selection, and the IO-proc buffer handling are the most likely spots). The
acceptance checks in `docs/prompts/02-macos-capture.md` — real Zoom/Meet call,
unplugging headphones mid-recording, no leftover aggregate devices — must be run on a
Mac.
