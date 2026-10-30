# Prompt 0 — Repo setup and CLAUDE.md

Creates the monorepo, the shared data types, and the `CLAUDE.md` that holds the rules every later prompt depends on.

```text
You are setting up the monorepo for Hearme, a desktop meeting-transcript app. The repo is github.com/peggy1213-create/Hearme, checked out at C:\Users\Peich\Documents\Hearme. Read docs/plan.md first; it is the approved development plan. docs/ already holds plan.md, prompts/ and design/palette.md: keep them.

Start by proposing a short plan (folders, tools, versions). Wait for my OK before writing files.

GOAL
Create the repo skeleton, tooling, shared types and a CLAUDE.md that future Claude Code sessions will follow.

1. Monorepo (pnpm workspaces + Turborepo, Node 22 LTS, TypeScript strict):
   apps/desktop           Tauri 2 app (Rust core in src-tauri, React + TypeScript + Vite UI)
   services/gateway       Node + Fastify: relays audio to STT and text to the LLM, holds accounts and usage; stores no meeting content
   native/macos-capture   Swift package, CLI helper for macOS audio capture
   native/windows-capture Rust crate, CLI helper for Windows audio capture
   packages/shared        TS types, zod schemas, protocol and file-format docs
   packages/stt           STT adapter interface + vendor adapters
   packages/zhtw          Traditional Chinese (Taiwan) text normalisation
   tools/benchmark        STT benchmark CLI
   docs/                  plan.md, prompts/, design/, progress.md, decisions/ (ADRs)
   Only create empty, buildable stubs for the app and services. Later prompts fill them in.

2. Tooling: ESLint + Prettier, Vitest, rustfmt + clippy, SwiftFormat, a root .editorconfig and .gitattributes (LF line endings), .gitignore (include .env and any local meeting folders), .env.example listing SONIOX_API_KEY, ASSEMBLYAI_API_KEY, DEEPGRAM_API_KEY, ANTHROPIC_API_KEY, GOOGLE_OAUTH_CLIENT_ID, MICROSOFT_OAUTH_CLIENT_ID, DATABASE_URL. docker-compose.yml with PostgreSQL 16 (accounts and usage only). GitHub Actions CI that lints, type-checks and tests on push, on Windows and macOS runners.

3. packages/shared: define with zod and export as TS types:
   - TranscriptSegment { id, meetingId, track: 'mic' | 'meeting', speaker: string ('me', 'S1', 'S2'...), speakerName?: string, startMs, endMs, text, lang: 'en' | 'zh-TW' | 'mixed', isFinal: boolean, pass: 'live' | 'final', confidence?: number }
   - Meeting { id, title, platform: 'zoom' | 'meet' | 'teams' | 'webex' | 'other', startedAt, endedAt?, folderPath, status, driveSync?: { state: 'off' | 'pending' | 'synced' | 'error', driveFolderId?, lastSyncedAt? } }
   - The gateway WebSocket protocol messages (client->server: start, audio, stop; server->client: segment, error, ended). Document them in packages/shared/PROTOCOL.md.
   - The local meeting folder format in packages/shared/MEETING_FOLDER.md: mic.ogg, meeting.ogg, mixed.ogg, meeting.json (metadata + live segments), transcript.json and transcript.md (final), summary.json and summary.md, edits.json (the user's corrections and speaker names). Include a version number in every JSON file.

4. Write CLAUDE.md at the repo root with exactly these product rules, plus commands for build, test and lint:

--- CLAUDE.md product rules ---
- The app NEVER joins a meeting. No bots, no extra participant, nothing in the attendee list. It only captures audio already playing on the user's computer plus the user's microphone. Do not use Teams Graph media bots, Meet bots or any meeting-bot service.
- Platforms: Zoom, Google Meet (in a browser), Microsoft Teams, Webex. Capture is per-app on macOS 14.2+ (Core Audio process taps) and Windows 10/11 (WASAPI process loopback).
- Languages: English and Taiwanese Mandarin, including switching mid-sentence. Chinese output is ALWAYS Traditional characters with Taiwan wording. Run all Chinese text through packages/zhtw (OpenCC s2twp) before it is shown or saved. Never display Simplified Chinese.
- Storage: every recording, transcript and summary is saved ONLY in a local meeting folder on the user's computer (default: Documents/Hearme Meetings; the user can change it), in the format of packages/shared/MEETING_FOLDER.md. Hearme's servers never store meeting content: the gateway keeps audio and text only in memory while relaying them to the STT and LLM vendors, and stores only accounts and usage counts.
- Sync: the only sync is opt-in Google Drive sync. It runs directly from the desktop app to the user's own Google Drive using the drive.file scope, never through Hearme's servers.
- Recording: every meeting is recorded as two tracks (mic, meeting) plus a mixed playback file. Flush to disk at least every 10 seconds so a crash loses little.
- Transcription has two passes: a live streaming pass (target under 2 s delay) and a final batch pass on the full recording after the meeting.
- Consent and privacy: a visible recording indicator whenever audio is captured; never capture silently. No transcript or audio content in logs, crash reports or analytics. Vendor API keys live only on the gateway, never in the desktop app.
- Accuracy is measured with CER for Chinese and WER for English (see tools/benchmark).
- UI colours ("Quiet Signal" palette, details in docs/design/palette.md): primary #2E44C4 (dark #8E9CFF); listening indicator #C2412D (dark #FF8A70), used only for "audio is being captured"; highlight #FFE7A3; ink #15181E; muted #5B6270; canvas #F6F6F3; line #E2E3DF. Speaker colours in order: #2E44C4 (me), #0F766E, #8A3B9A, #0A6AA6, #B0245A, #4A7310. Text must meet WCAG AA 4.5:1.
- Working style: propose a plan before large changes; keep docs/progress.md updated at the end of each session; record big technical choices as short ADRs in docs/decisions/.
--- end ---

ACCEPTANCE
- pnpm install && pnpm build && pnpm test pass on a clean clone on Windows and macOS.
- docker compose up starts PostgreSQL.
- CLAUDE.md, PROTOCOL.md, MEETING_FOLDER.md and docs/progress.md exist.

Finish by listing what you created and anything you were unsure about.
```

