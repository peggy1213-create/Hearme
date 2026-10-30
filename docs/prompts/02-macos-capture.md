# Prompt 2 — macOS audio capture helper

Builds the small native program that listens to Zoom, Teams, Webex or a browser on a Mac, plus the microphone, without joining the meeting. Run on a Mac.

```text
Read CLAUDE.md, docs/plan.md and packages/shared/PROTOCOL.md. Work only in native/macos-capture (plus a short doc in packages/shared).

Explore first. Fetch Apple's current docs and sample for "Capturing system audio with Core Audio taps" (AudioHardwareCreateProcessTap, CATapDescription, aggregate devices). Then propose a plan and wait for my OK.

GOAL
A Swift command-line helper, "tapcapture", for macOS 14.2+, that the desktop app will run as a sidecar process. It only captures and reports audio. All processing, recording and streaming happen later in the desktop core (Prompt 5).

1. Interface (document it in packages/shared/CAPTURE_HELPER.md; Prompt 3 must match it):
   - Commands as JSON lines on stdin: list_sources, start { sourceId, micDeviceId? }, stop, ping.
   - Output on stdout as length-prefixed binary frames:
     header = type (u8: 1 audio, 2 event), track (u8: 0 mic, 1 meeting), hostTimeNs (u64), sampleRate (u32), channels (u8), frameCount (u32), then the payload.
     Audio payload: float32 PCM, mono, at the device's native rate (the core resamples).
     Event payload: UTF-8 JSON, e.g. {"event":"source_started","bundleId":"us.zoom.xos"}.
   - Use one shared clock (host time) for both tracks so the core can align them.

2. Meeting-app detection
   - Read the Core Audio process list and report which processes are producing audio, with bundle ID and name.
   - Keep known meeting apps in a JSON config (Zoom, Microsoft Teams, Webex; browsers Chrome, Edge, Safari, Arc for Google Meet). Verify the real bundle IDs on this Mac and note how you checked.
   - Many apps play audio from helper or child processes (browser renderers, WebView helpers). Tap every audio-producing process that belongs to the chosen app, not just the main one.
   - Emit source_started and source_stopped events as apps begin and stop playing audio. Do not start capturing on its own; the app asks the user first.

3. Capture
   - Meeting track: a process tap (mixdown to mono) on the chosen app's processes, unmuted so the user still hears the meeting, inside a private aggregate device.
   - Mic track: the selected or default input device.
   - Handle: default output or input device changing (headphones plugged in), sleep and wake, the meeting app restarting, and the helper being killed (clean up the tap and aggregate device on exit and on next launch).

4. Permissions
   - The app bundle will need NSAudioCaptureUsageDescription and NSMicrophoneUsageDescription. Find out which process macOS attributes the permission to when tapcapture runs as a sidecar inside a Tauri .app, and document it.
   - Without permission, macOS can deliver silent (all-zero) buffers instead of an error. Detect several seconds of exact zeros while the source is known to be playing and emit {"event":"permission_suspected_missing"}.

5. Testing
   - A debug subcommand: tapcapture record --bundle us.zoom.xos --seconds 30 --out ./out writes mic.wav and meeting.wav so I can listen to them.
   - An automated test that plays a known tone with afplay, taps afplay, and checks the captured tone's frequency.
   - Swift unit tests for the frame encoder.

ACCEPTANCE
- With a real Zoom or Meet call, tapcapture record produces two clean, aligned WAV files.
- Unplugging headphones mid-recording does not stop capture.
- No leftover aggregate devices in Audio MIDI Setup after quitting or killing the helper.
- Update docs/progress.md and add an ADR for any key decision.
```

