# Prompt 3 — Windows audio capture helper

The Windows version of Prompt 2: same commands and output, so the desktop app treats both the same way. Run on a Windows PC.

```text
Read CLAUDE.md, docs/plan.md, packages/shared/PROTOCOL.md and packages/shared/CAPTURE_HELPER.md. CAPTURE_HELPER.md is the contract: match it exactly. Work only in native/windows-capture.

Explore first. Fetch Microsoft's current docs and the ApplicationLoopback sample for WASAPI process loopback (ActivateAudioInterfaceAsync with AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE). Confirm the minimum Windows build that supports it and record it in an ADR. Then propose a plan and wait for my OK.

GOAL
A Rust command-line helper, "tapcapture.exe" (use the windows crate), with the same stdin commands, stdout frame format, events and debug subcommand as the macOS helper.

1. Meeting-app detection
   - Enumerate active audio sessions (IAudioSessionManager2) to see which processes are playing audio, with exe name and PID.
   - Known meeting apps in the shared JSON config: Zoom, new Microsoft Teams (ms-teams.exe, which plays audio through WebView2 child processes), Webex, and browsers Chrome, Edge, Firefox for Google Meet. Verify the real process names on this PC.
   - Capture the whole process tree of the chosen app (INCLUDE_TARGET_PROCESS_TREE) so child processes are covered.
   - Emit source_started and source_stopped events. Never start capturing on its own.

2. Capture
   - Meeting track: process loopback on the chosen app's root process, mixed to mono float32.
   - Mic track: the selected or default capture endpoint, shared mode.
   - Shared host clock for both tracks (QueryPerformanceCounter converted to ns) so the core can align them.
   - Handle: default device changes, sleep and wake, the meeting app restarting (re-target the new PID), and clean shutdown.

3. Permissions and errors
   - Detect Windows microphone privacy settings blocking access and emit {"event":"mic_blocked"} with a short message the UI can show.
   - Report unsupported Windows builds clearly instead of crashing.

4. Testing
   - tapcapture.exe record --process zoom.exe --seconds 30 --out .\out writes mic.wav and meeting.wav.
   - An automated test that plays a known tone from a child process and checks it is captured.
   - Unit tests for the frame encoder, shared with or mirrored from the macOS helper's test vectors.

ACCEPTANCE
- With a real Teams or Meet call, the record command produces two clean, aligned WAV files.
- Switching from speakers to a headset mid-call keeps capturing.
- cargo clippy -- -D warnings and cargo test pass.
- Update docs/progress.md.
```

