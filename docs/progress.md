# Progress

A short log of what each working session changed. Newest first. Keep this updated at
the end of each session (see CLAUDE.md).

## 2026-11-03 — Prompt 4: streaming gateway + live transcription

Built the gateway that turns two audio tracks per meeting into live, labelled,
Traditional-Chinese-safe transcript segments and streams them back, storing no
meeting content. Verified locally with `pnpm build / typecheck / lint / format:check
/ test` (all packages); the integration/load/privacy tests run on in-process PGlite +
the mock STT vendor, so no Docker, network or vendor keys are needed.

- **Protocol (`packages/shared`, v1 → v2):** `start` gained `vocabulary`; `codec`
  now also accepts `pcm`; new `ack { track, lastSeq }` server message drives resume.
  `PROTOCOL.md` updated with the resume handshake.
- **STT (`packages/stt`):** `MockAdapter` (deterministic, scripted, with a
  Simplified-source line and a `failAfterFrames` switch) and `FailoverAdapter`
  (primary → fallback, transparent mid-stream). `createSttAdapter('mock')` added.
- **Gateway (`services/gateway`):**
  - `GET /v1/live` WebSocket: one session per meeting, a vendor stream per track
    (mic → `me`, meeting → stable `S1,S2…`), Opus decode via **opusscript** (pure-JS,
    no native build), per-track seq dedup + a few-minute in-memory **resume** window,
    automatic **vendor failover**, and a `toTaiwanTraditional` guard so no Simplified
    character can ever reach a client.
  - **Accounts/usage:** Drizzle ORM, only `users` + `usage` tables; PostgreSQL in
    prod (`DATABASE_URL`), **PGlite** in tests. Monthly minute **quota** enforced on
    `start`; usage written on `stop`.
  - **Auth:** OAuth 2 + PKCE (Google, Microsoft) loopback flow; stateless JWT access
    - refresh tokens (no token table); dev bypass token gated to development.
  - **Ops:** keys from env only; request logging disabled and `Authorization`
    redacted (no content in logs); OpenTelemetry metrics (segment latency, vendor
    errors, active meetings) exporting only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set;
    `Dockerfile` + `/health`. `POST /v1/batch` final pass (temp file deleted
    immediately).
- **`tools/replay`:** `pnpm replay --wav meeting.wav --mic mic.wav` streams two WAVs
  at real time through a fake client (raw PCM, no ffmpeg) and prints the live
  transcript.
- **Tests:** reconnect/resume loses no words, vendor failover, quota exceeded,
  invalid token, Traditional-only guard, a privacy check (no transcript text in logs
  or DB; only `users`/`usage` tables), a 50-meeting load test, and auth/token units.
- **ADR:** `docs/decisions/0005-gateway-live-transcription.md` (accepted; the
  Soniox-live / AssemblyAI-fallback default is **provisional** until ADR 0002's
  benchmark runs — then update `STT_LIVE_VENDOR` / `STT_FALLBACK_VENDOR`).

**Next (needs you):** provision vendor keys + a real Postgres (`docker compose up`,
`pnpm --filter @hearme/gateway db:migrate`), set the OAuth client credentials, and
replay a real mixed zh-TW/English recording against a live vendor to confirm the
under-2 s (p95) latency and Traditional-only output on real audio.

## 2026-11-01 — Prompt 3: Windows capture helper (`tapcapture.exe`)

Built the Windows audio capture sidecar in `native/windows-capture` (Rust, `windows`
crate), the counterpart to the macOS helper, speaking the **same** protocol so the core
treats both the same way. Verified on this Windows 11 machine: `cargo build`,
`cargo test` (36 tests), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`
all green; and in protocol mode `list_sources` correctly reported the real running apps
(detected Teams + Edge) and `ping` round-tripped a valid event frame over stdout. The
**audio capture** paths (process loopback + mic) compile and are wired per Microsoft's
Application Loopback sample but are **not yet exercised against a real call** — see the
README's Status section.

- **ADR:** `docs/decisions/0004-windows-audio-capture.md` (accepted). WASAPI process
  loopback via `ActivateAudioInterfaceAsync` +
  `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` +
  `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`. Confirmed minimum build from
  Microsoft's docs: **Windows 10 Build 20348**.
- **Helper (`tapcapture.exe`):** renamed the Cargo bin from the Prompt 0 stub. Frame
  codec ported byte-for-byte from the Swift helper (same test vectors); shared
  `QueryPerformanceCounter` → ns clock; process detection via Toolhelp snapshot +
  `IAudioSessionManager2` active sessions, with process-tree ancestry so browser
  renderers and Teams' WebView2 children count toward "producing audio"; `known-apps.json`
  by **exe name** (Zoom, new Teams `ms-teams.exe`, Webex, Chrome/Edge/Firefox for Meet).
- **Capture:** meeting track = process loopback on the app's **root PID** (whole tree),
  mixed to mono float32; mic track = shared-mode WASAPI on the default/selected input;
  both event-driven on their own threads writing frames to a mutex-guarded stdout. One
  serial control thread (stdin commands + 1 s source monitor + device-change callbacks)
  mirrors the macOS control queue; re-targets the meeting app on restart; `device_changed`
  - mic re-point on default-input change; all-zero **silence guard**; Windows mic-privacy
    denial → new `mic_blocked` event; unsupported build → `fatal_unsupported` error.
- **Debug:** `tapcapture.exe record --process <exe> [--seconds n] [--out dir]` writes
  `meeting.wav` + `mic.wav` (16-bit PCM).
- **Next (needs a real machine in a call):** run `record` against a live Teams/Meet call
  for two clean aligned WAVs, and swap speakers→headset mid-call; fix up loopback buffer/
  format negotiation and mic mix-format handling if needed.

## 2026-11-01 — Prompt 2: macOS capture helper (`tapcapture`)

Built the macOS audio capture sidecar in `native/macos-capture`, turning the Prompt 0
stub into a working helper. **Authored on Windows against Apple's current Core Audio
taps docs — not yet compiled or run on a Mac**; expect a small fixup pass against the
real SDK.

- **Interface:** `packages/shared/CAPTURE_HELPER.md` — the shared helper protocol both
  OS helpers speak: JSON commands on stdin (`list_sources`, `start`, `stop`, `ping`),
  length-prefixed binary frames on stdout (19-byte header: type/track/hostTimeNs/
  sampleRate/channels/frameCount), event payloads, and the one shared host clock. This
  is the contract Prompt 3 (Windows) must match.
- **Helper (`tapcapture`):** renamed the SwiftPM target from `macos-capture`. Frame
  codec + shared `mach_absolute_time` clock; HAL process enumeration; `known-apps.json`
  (Zoom, Teams, Webex, and Chrome/Edge/Safari/Arc for Meet) matched by bundle-ID prefix
  so browser helper/renderer processes are tapped too; meeting track via
  `CATapDescription(monoMixdownOfProcesses:)` in a **private, unmuted** aggregate
  device; mic track via `AVAudioEngine`; `source_started/stopped` monitoring; device
  change + sleep/wake handling; all-zero **silence guard** →
  `permission_suspected_missing`; signal-handler teardown + orphan-device reaping; and
  the `record --bundle --seconds --out` debug subcommand (writes `mic.wav`/`meeting.wav`).
- **Tests:** frame round-trip, config matching, silence guard, protocol parsing (all
  headless), plus an opt-in `afplay` tone-capture integration test
  (`HEARME_RUN_AUDIO_TESTS=1`, needs a Mac with audio permission).
- **Docs:** `native/macos-capture/README.md` (build/run + the sidecar permission trap),
  ADR `0003-macos-audio-capture.md`.

**Next (needs you, on a Mac):** `swift build && swift test`; fix any real-SDK
mismatches (Core Audio aggregate-device key constants, `AVAudioEngine` device
selection, IO-proc buffer handling are the likely spots); verify `known-apps.json`
bundle IDs and that the tap permission is attributed to the host app when `tapcapture`
runs as a Tauri sidecar; then the Prompt 2 acceptance checks (real Zoom/Meet call,
unplug headphones mid-recording, no leftover aggregate devices).

## 2026-10-30 — Prompt 1: STT benchmark (Phase 0)

Built the tooling to pick the STT vendor on real zh-TW/English audio. Not yet run —
awaiting `benchmark-data/` and vendor API keys; the harness stops cleanly when the
corpus is missing rather than inventing data.

- **`packages/zhtw`:** added `toTaiwanTraditional` (OpenCC s2twp; `toTaiwan` kept as an
  alias), `normaliseForScoring`, `tokenizeMixed` (Han chars → single tokens, English →
  word tokens), `containsSimplified`. 15 unit tests incl. code-switched sentences.
- **`packages/stt`:** implemented the Soniox, AssemblyAI and Deepgram adapters (stream +
  batch, language auto-detect/code-switching and diarization on, Chinese normalised to
  Traditional) and a local **faster-whisper** baseline (`WhisperAdapter`, batch-only via
  a Python subprocess). Vendor API docs + pricing fetched 2026-10-30 — URLs/dates in each
  adapter header. Added `'whisper'` to `SttVendor` and a `normalizeChinese` option so the
  benchmark can inspect raw output for Simplified. Deps: `ws`, `@types/ws`, `@types/node`.
- **`tools/benchmark`:** grew the CLI into a corpus harness — `init-refs` writes reference
  templates; the default run scores each vendor in batch + simulated real-time (1×, 100 ms
  frames) and reports mixed ER, CER (zh), WER (en), latency p50/p95, est. $/audio-hour, and
  a Simplified-before-conversion flag, to `reports/benchmark-<date>.md` + CSV. Needs ffmpeg
  for the streaming pass (not installed on this machine — batch still runs). Root `benchmark`
  script added. 16 metric unit tests.
- **ADR:** `docs/decisions/0002-stt-vendor.md` (numbered 0002 — 0001 was taken), status
  **proposed**, with the methodology and pricing table; vendor choice to be filled from the
  first real report.
- Verified: `pnpm build / typecheck / test / lint / format:check` all green across 7 packages.

**Next (needs you):** drop real audio + `<name>.txt` references into `benchmark-data/`, put
vendor keys in `.env`, install ffmpeg, then `pnpm benchmark --data ./benchmark-data`; fill in
ADR 0002 from the report.

## 2026-10-30 — Prompt 0: repo setup

Created the monorepo skeleton, tooling, shared types and `CLAUDE.md`.

- **Monorepo:** pnpm workspaces + Turborepo, Node 22 LTS, TypeScript 5 strict. Root
  configs: `pnpm-workspace.yaml`, `turbo.json`, `tsconfig.base.json`, `.editorconfig`,
  `.gitattributes` (LF), `.gitignore`, `.env.example`, Prettier, ESLint (flat),
  `vitest.workspace.ts`, `.nvmrc`.
- **Packages created as buildable stubs:**
  - `packages/shared` — zod schemas + TS types for `TranscriptSegment`, `Meeting`, the
    gateway WebSocket protocol, and the meeting-folder JSON files; `PROTOCOL.md` and
    `MEETING_FOLDER.md`.
  - `packages/zhtw` — `toTaiwan()` over OpenCC s2twp (`opencc-js`).
  - `packages/stt` — `SttAdapter` interface + Soniox/AssemblyAI/Deepgram stubs.
  - `tools/benchmark` — CER/WER metrics + CLI skeleton.
  - `services/gateway` — Fastify server with `/health`; stores no meeting content.
  - `apps/desktop` — Tauri 2 (Rust core in `src-tauri`) + React/TS/Vite UI shell.
  - `native/macos-capture` — Swift package CLI stub.
  - `native/windows-capture` — Rust crate CLI stub.
- **Infra:** `docker-compose.yml` (PostgreSQL 16, accounts/usage only); GitHub Actions
  CI linting, type-checking and testing on Windows + macOS, plus per-OS native builds.
- **Docs:** this file and `docs/decisions/0001-monorepo-and-tooling.md`.

**Next:** Prompt 1 — STT benchmark on real zh-TW/English audio to pick the vendor.
