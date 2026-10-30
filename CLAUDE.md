# CLAUDE.md

Guidance for Claude Code (and any contributor) working in this repository.

Hearme is a botless desktop meeting-transcription app for macOS and Windows. The
approved plan is in [docs/plan.md](docs/plan.md). Read it before large changes.

## Product rules (do not break these)

- The app **NEVER joins a meeting**. No bots, no extra participant, nothing in the
  attendee list. It only captures audio already playing on the user's computer plus
  the user's microphone. Do not use Teams Graph media bots, Meet bots or any
  meeting-bot service.
- **Platforms:** Zoom, Google Meet (in a browser), Microsoft Teams, Webex. Capture is
  per-app on macOS 14.2+ (Core Audio process taps) and Windows 10/11 (WASAPI process
  loopback).
- **Languages:** English and Taiwanese Mandarin, including switching mid-sentence.
  Chinese output is **ALWAYS Traditional characters with Taiwan wording**. Run all
  Chinese text through `packages/zhtw` (OpenCC s2twp) before it is shown or saved.
  Never display Simplified Chinese.
- **Storage:** every recording, transcript and summary is saved **ONLY** in a local
  meeting folder on the user's computer (default: `Documents/Hearme Meetings`; the
  user can change it), in the format of
  [packages/shared/MEETING_FOLDER.md](packages/shared/MEETING_FOLDER.md). Hearme's
  servers never store meeting content: the gateway keeps audio and text only in
  memory while relaying them to the STT and LLM vendors, and stores only accounts and
  usage counts.
- **Sync:** the only sync is opt-in Google Drive sync. It runs directly from the
  desktop app to the user's own Google Drive using the `drive.file` scope, never
  through Hearme's servers.
- **Recording:** every meeting is recorded as two tracks (mic, meeting) plus a mixed
  playback file. Flush to disk at least every 10 seconds so a crash loses little.
- **Transcription** has two passes: a live streaming pass (target under 2 s delay) and
  a final batch pass on the full recording after the meeting.
- **Consent and privacy:** a visible recording indicator whenever audio is captured;
  never capture silently. No transcript or audio content in logs, crash reports or
  analytics. Vendor API keys live only on the gateway, never in the desktop app.
- **Accuracy** is measured with CER for Chinese and WER for English (see
  `tools/benchmark`).
- **UI colours** ("Quiet Signal" palette, details in
  [docs/design/palette.md](docs/design/palette.md)): primary `#2E44C4` (dark
  `#8E9CFF`); listening indicator `#C2412D` (dark `#FF8A70`), used only for "audio is
  being captured"; highlight `#FFE7A3`; ink `#15181E`; muted `#5B6270`; canvas
  `#F6F6F3`; line `#E2E3DF`. Speaker colours in order: `#2E44C4` (me), `#0F766E`,
  `#8A3B9A`, `#0A6AA6`, `#B0245A`, `#4A7310`. Text must meet WCAG AA 4.5:1.
- **Working style:** propose a plan before large changes; keep
  [docs/progress.md](docs/progress.md) updated at the end of each session; record big
  technical choices as short ADRs in [docs/decisions/](docs/decisions/).

## Repository layout

| Path                     | What it is                                                                                                   |
| ------------------------ | ------------------------------------------------------------------------------------------------------------ |
| `apps/desktop`           | Tauri 2 app — Rust core in `src-tauri`, React + TypeScript + Vite UI                                         |
| `services/gateway`       | Node + Fastify: relays audio to STT and text to the LLM, holds accounts and usage; stores no meeting content |
| `native/macos-capture`   | Swift package, CLI helper for macOS audio capture (Core Audio process taps)                                  |
| `native/windows-capture` | Rust crate, CLI helper for Windows audio capture (WASAPI process loopback)                                   |
| `packages/shared`        | TS types, zod schemas, protocol and file-format docs                                                         |
| `packages/stt`           | STT adapter interface + vendor adapters                                                                      |
| `packages/zhtw`          | Traditional Chinese (Taiwan) text normalisation (OpenCC s2twp)                                               |
| `tools/benchmark`        | STT benchmark CLI (CER / WER)                                                                                |
| `docs/`                  | plan.md, prompts/, design/, progress.md, decisions/ (ADRs)                                                   |

## Toolchain

- **Node 22 LTS** (see `.nvmrc`), **pnpm 9** (via Corepack), **TypeScript 5 strict**.
- **Rust** stable for `native/windows-capture` and the Tauri core.
- **Swift** 5.9+ for `native/macos-capture` (builds on macOS only).
- Line endings are **LF** everywhere (`.editorconfig`, `.gitattributes`).

## Commands

Run from the repository root unless noted.

```bash
# Install all workspace dependencies
pnpm install

# Build every package (JS/TS workspace)
pnpm build

# Type-check, lint, format-check, test
pnpm typecheck
pnpm lint
pnpm format        # write
pnpm format:check  # verify
pnpm test

# Local accounts/usage database (PostgreSQL 16, no meeting content)
docker compose up -d
```

Native helpers and the Tauri Rust core build per-OS and are not part of the root
`pnpm build`:

```bash
# macOS capture helper (macOS only)
cd native/macos-capture && swift build && swift test

# Windows capture helper (Windows only)
cd native/windows-capture && cargo build && cargo test
cd native/windows-capture && cargo fmt --check && cargo clippy --all-targets -- -D warnings

# Tauri Rust core (per-OS)
cd apps/desktop/src-tauri && cargo check

# Desktop app UI (dev)
pnpm --filter @hearme/desktop dev
```

## Conventions

- Shared types and the gateway protocol live in `packages/shared`; define them with
  **zod** and export inferred TS types. Change the schema there, not in copies.
- Bump the `version` field in any meeting-folder JSON file whose shape changes, and
  update [packages/shared/MEETING_FOLDER.md](packages/shared/MEETING_FOLDER.md).
- Never log or report audio or transcript content. Scrub it from errors and telemetry.
