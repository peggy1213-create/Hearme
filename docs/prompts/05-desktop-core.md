# Prompt 5 — Desktop core: echo cancellation, recording, streaming

Builds the engine inside the desktop app: it cleans the audio, records it to disk, and streams it to the gateway. No user interface yet. Run on a Mac first, then repeat the acceptance checks on Windows.

```text
Read CLAUDE.md, docs/plan.md, packages/shared/PROTOCOL.md and packages/shared/CAPTURE_HELPER.md and packages/shared/MEETING_FOLDER.md. Work in apps/desktop/src-tauri (Rust).

Explore first. Check that the Rust crates you plan to use are maintained and build on macOS and Windows (for example webrtc-audio-processing for echo cancellation, a Silero VAD crate, rubato for resampling, opus and ogg for encoding). Propose a plan and wait for my OK.

GOAL
The Rust core of the Tauri app that turns the capture helper's raw audio into (a) a recording on disk and (b) a live stream to the gateway.

1. Helper management
   - Bundle tapcapture as a Tauri sidecar for each platform. Start it, parse its frames, restart it if it crashes, and pass its events to the UI.

2. Audio pipeline (per meeting)
   - Align the mic and meeting tracks by host timestamp.
   - Echo cancellation: when the user has no headphones, the mic hears the meeting through the speakers. Run WebRTC AEC on the mic track with the meeting track as the far-end reference, plus noise suppression. Make this switchable for testing.
   - Voice activity detection on each track so only speech is streamed.
   - Resample: 48 kHz for recording, 16 kHz for streaming.

3. Recording (required in every meeting)
   - Folder: <meetings folder>/<yyyy-mm-dd HHmm> <title>/, where the meetings folder defaults to Documents/Hearme Meetings and the user can change it. It contains mic.ogg, meeting.ogg (Opus, mono, 48 kHz), mixed.ogg for playback, and meeting.json (meeting metadata plus the live segments as they arrive), exactly as MEETING_FOLDER.md defines. Nothing about the meeting is saved anywhere else.
   - Write continuously and flush at least every 10 seconds. After a crash or power loss, the next launch finds unfinished recordings, repairs and closes the files, and marks them "recovered".
   - Check free disk space before starting and warn below 1 GB.
   - Record everything, including silence, so timestamps in the transcript always match the audio.

4. Live streaming to the gateway
   - Opus 16 kHz mono 20 ms frames with sequence numbers, voiced audio only, as PROTOCOL.md defines.
   - Keep sent frames in an on-disk buffer until acked. On disconnect, keep recording, reconnect with backoff, and resend from lastSeq + 1.
   - If offline for a long time, stop buffering for live mode after 30 minutes; the final pass (Prompt 7) will transcribe from the recording instead.
   - Store auth tokens in the OS keychain (macOS Keychain, Windows Credential Manager).

5. API for the UI (Tauri commands and events)
   - Commands: list_sources, start_meeting { sourceId, title? }, stop_meeting, set_echo_cancellation, open_recording_folder.
   - Events: source_started/stopped, levels (mic and meeting, about 10 per second), segment, connection_status, recording_status, warnings (permission, disk space, no audio).
   - Define a TranscriptionEngine trait so an on-device engine (whisper.cpp) can be added later for local-only mode. Do not build that engine now.

TESTS
- Feed recorded WAV pairs through the pipeline in tests (no helper needed) and check the output files, sample alignment and sequence numbers.
- An echo test: a meeting track that leaks into the mic track at -12 dB must be strongly reduced after AEC (measure it).
- Kill the process mid-recording and confirm recovery on the next start.

ACCEPTANCE
- On a Mac, a real Zoom or Meet call produces a playable mixed.ogg and a live transcript arriving through the gateway.
- Pulling the network for 2 minutes loses no recording and no live text after reconnection.
- The same checks pass on Windows with the Windows helper.
- Update docs/progress.md.
```

