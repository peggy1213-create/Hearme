# Claude Code prompts for Hearme

Each file below is one prompt. In Claude Code, from the repo root, either paste the text inside the code block, or say: `Read docs/prompts/<file> and follow it.`

## How to use these prompts

Run Prompt 0 first. It writes `CLAUDE.md`, which every later session reads automatically, so the product rules never need repeating.

**Before you start**

- [ ] Connect C:\\Users\\Peich\\Documents\\Hearme to github.com/peggy1213-create/Hearme (the plan and these prompts are already saved under docs/). The plan lives at `docs/plan.md`.
- [ ] A Mac on macOS 14.2 or later with Xcode, for Prompts 2, 5, 6 and 9.
- [ ] A Windows 11 PC with Rust and Visual Studio Build Tools, for Prompt 3 (Claude Code must run on that PC).
- [ ] API keys in a local `.env` (never committed): Soniox, AssemblyAI, Deepgram, Anthropic, plus a Google Cloud OAuth client with the Drive API enabled (for sign-in and Prompt 8).
- [ ] Docker for the local PostgreSQL database (accounts and usage only).
- [ ] For Prompt 1: about 10 hours of real meeting audio (with everyone's consent) and correct reference transcripts.

**Working rules**

1. One prompt per new Claude Code session, run from the repo root.
2. Each prompt asks Claude Code to explore and propose a plan first. Read the plan, correct it, then let it build.
3. Don't move on until the prompt's acceptance checks pass. Commit after each prompt.
4. If a prompt is too big for one session, tell Claude Code to stop at a working checkpoint, update `docs/progress.md`, and continue in a new session with "Continue Prompt N from docs/progress.md".

| Prompt | Builds | Needs first | Run on |
| --- | --- | --- | --- |
| 0 | Repo, CLAUDE.md, shared types, meeting-folder format | — | Any |
| 1 | STT benchmark, picks the vendor | 0 | Any |
| 2 | macOS capture helper | 0 | Mac |
| 3 | Windows capture helper | 0 | Windows |
| 4 | Streaming gateway (relays only) | 0, 1 | Any |
| 5 | Desktop core: echo, recording, streaming | 2 or 3, 4 | Mac or Windows, then the other |
| 6 | Desktop UI | 5 | Mac or Windows |
| 7 | Final transcript and summary, saved locally | 4, 5 | Any |
| 8 | Library, search, playback, export, Google Drive sync | 6, 7 | Mac or Windows |
| 9 | Packaging and beta hardening | 6, 7, 8 | Mac and Windows |

Prompts 2, 3 and 4 can run at the same time on different machines once Prompt 0 is merged.



## Files

- [Prompt 0 — Repo setup and CLAUDE.md](00-repo-setup.md)
- [Prompt 1 — Speech-to-text benchmark (Phase 0)](01-stt-benchmark.md)
- [Prompt 2 — macOS audio capture helper](02-macos-capture.md)
- [Prompt 3 — Windows audio capture helper](03-windows-capture.md)
- [Prompt 4 — Streaming gateway and live transcription](04-gateway.md)
- [Prompt 5 — Desktop core: echo cancellation, recording, streaming](05-desktop-core.md)
- [Prompt 6 — Desktop UI](06-desktop-ui.md)
- [Prompt 7 — Final transcript and summary, saved locally](07-final-transcript-summary.md)
- [Prompt 8 — Library, search, playback, export and Google Drive sync](08-library-search-drive-sync.md)
- [Prompt 9 — Packaging, signing, auto-update and beta hardening](09-packaging-beta.md)
