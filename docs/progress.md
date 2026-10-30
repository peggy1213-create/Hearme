# Progress

A short log of what each working session changed. Newest first. Keep this updated at
the end of each session (see CLAUDE.md).

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
