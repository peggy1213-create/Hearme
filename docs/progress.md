# Progress

A short log of what each working session changed. Newest first. Keep this updated at
the end of each session (see CLAUDE.md).

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
