# ADR 0006: Desktop core — audio pipeline, recording, and streaming

- Status: **accepted**
- Date: 2026-11-05

## Context

Prompt 5 builds the Rust core of the Tauri app (`apps/desktop/src-tauri`): it turns
the capture helper's raw audio frames into (a) a recording on disk and (b) a live
Opus stream to the gateway, with no UI. Several library and design choices were open;
they are recorded here. All crates were checked to build on this Windows machine
before being adopted; the macOS build and real-call acceptance remain hardware
follow-ups (see progress.md), matching Prompts 2–4.

## Decisions

### Echo cancellation + noise suppression: `sonora` (default), NLMS fallback

- **`sonora`** (pure-Rust port of WebRTC AudioProcessing M145: AEC3 + noise
  suppression + AGC2) is the default backend, behind the `sonora-aec` feature. It has
  **no C++/meson/ninja toolchain**, builds cleanly on Windows x86_64 and macOS, and
  processes the WebRTC-standard 10 ms frames. This was chosen over
  `webrtc-audio-processing` (tonarino, native C++ bindings) whose bundled build needs
  meson+ninja and whose Windows support is unverified.
- A dependency-free **normalized-LMS adaptive filter** (`NlmsAec`) is the fallback
  (builds with `--no-default-features`) and the **deterministic backend the echo test
  measures against**, so CI does not depend on sonora's adaptive convergence timing.
- A **passthrough** backend (`NoAec`) implements "echo cancellation off"
  (`set_echo_cancellation`), required to be switchable for testing.
- All three sit behind the `AecProcessor` trait; the mic track is the near-end, the
  meeting track the far-end reference. AEC runs on mono 48 kHz.

### Voice activity detection: energy gate (default), Silero opt-in

- The default **`EnergyVad`** is an adaptive energy gate with hangover — dependency
  free, deterministic, and used in tests. VAD gates only the **live stream**; the
  recorder always writes everything (including silence) so transcript timestamps line
  up with the audio.
- **Silero via `ort` (onnxruntime)** is kept behind the opt-in `silero-vad` feature.
  It is **not** default because `ort` pulls a native onnxruntime (and `openssl-sys` on
  Windows) and needs the model at runtime; the `Vad` trait keeps it swappable for a
  later prompt.

### Resampling, Opus, Ogg

- **`rubato`** (`FastFixedIn`, cubic) for mono sample-rate conversion: native → 48 kHz
  for recording, 48 kHz → 16 kHz for streaming. A 1:1 ratio is a zero-copy passthrough.
- **`audiopus`** (libopus, static-linked on Windows/macOS via `cc`) for Opus encode,
  and the **`ogg`** crate for the container. We write OpusHead/OpusTags and 20 ms
  frames ourselves (granule position in 48 kHz samples). This avoids `libopusenc`
  (needs cmake + the reference library).

### Recording layout and durability

- Three files per meeting: `mic.ogg`, `meeting.ogg` (the two tracks) and `mixed.ogg`
  (sum, clamped) for playback — mono 48 kHz Opus, as MEETING_FOLDER.md defines, plus
  `meeting.json`. Nothing about a meeting is written anywhere else.
- The Ogg writer holds the newest packet back so finalize can flag end-of-stream;
  `flush()` forces earlier packets' pages to disk. The driver flushes **every 10 s**,
  so a crash loses at most a frame or two.
- **Crash recovery:** on launch, meetings whose `meeting.json` still says `recording`
  are found; each Ogg file is truncated to its last complete page (`repair_ogg`), and
  the meeting is marked `recovered` + `stopped`. `meeting.recovered` is an **additive,
  optional** field in the shared `Meeting` schema (folder version unchanged).

### Live streaming, resume, and offline cutoff

- One WebSocket per meeting (PROTOCOL.md v2): `start`, then voiced 16 kHz mono 20 ms
  Opus frames with per-track `seq`, then `stop`. Server `ack { track, lastSeq }` drives
  resume.
- Sent-but-unacked frames are kept in an **on-disk outbox** per track (inside the
  meeting folder's `.hearme/`), pruned on ack and replayed from `lastSeq + 1` on
  reconnect. On disconnect the recording keeps going; the client reconnects with
  capped backoff and, after **30 minutes** offline, stops live buffering and leaves the
  rest to the final pass.
- The gateway auth token lives in the **OS keychain** (`keyring`: macOS Keychain,
  Windows Credential Manager) and is sent as a `Bearer` header, never in the URL.

### Sidecar management

- The helper runs as a child process managed by a supervisor task: it parses the
  binary frames, forwards events to the UI, logs the helper's stderr (diagnostics
  only), and **restarts the helper on crash**, re-issuing the active `start`. The
  helper is spawned with `tokio::process` (direct stdin/stdout pipes suit the binary
  framing better than the plugin-shell sidecar API). The binary is resolved from
  `HEARME_HELPER_PATH`, then next to the app executable; wiring it as a bundled
  Tauri sidecar (`externalBin`) is a packaging step for CI, once the per-OS binaries
  are produced.

### Engine abstraction

- `TranscriptionEngine` is the seam between the pipeline and transcription. The only
  implementation now is the cloud `GatewayEngine`; an on-device whisper.cpp engine for
  local-only mode (plan.md v1.1) can implement the same trait later and is **not**
  built now.

## Consequences

- The default build and `cargo test` need no network, no onnxruntime and no C++
  toolchain beyond the `cc`-driven libopus build, so they run in CI on Windows and
  macOS. Enabling `silero-vad` adds the onnxruntime/model requirement.
- Swapping the AEC backend to the native C++ crate later is a feature flag plus one
  `make_aec` branch; nothing else changes.
