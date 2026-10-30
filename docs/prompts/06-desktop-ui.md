# Prompt 6 — Desktop UI

Builds what the user sees: setup and permissions, the "start transcribing?" prompt, the consent reminder, and the floating live transcript window. Run on a Mac.

```text
Read CLAUDE.md (especially the UI colours and consent rules) and docs/plan.md. The Rust core from Prompt 5 exposes commands and events; read them in apps/desktop/src-tauri before starting. Work in apps/desktop/src (React + TypeScript).

Explore first, then propose the screens and component list and wait for my OK.

GOAL
A calm, small desktop UI that sits beside the meeting app. The interface itself is available in Traditional Chinese (default when the OS language is zh-TW) and English.

1. Design tokens
   - Put the palette from CLAUDE.md into CSS variables for light and dark mode (follow the OS setting). Use the listening coral ONLY for "audio is being captured". Every text colour must pass 4.5:1; add an automated contrast check in tests.
   - Fonts: Noto Sans TC for Chinese with a clean Latin sans for English; timestamps in a monospace font.

2. First run
   - Sign in (opens the system browser; the core handles the token).
   - A permissions screen that explains, in plain words, why the app needs the microphone and system-audio recording, opens the right OS settings page, and shows a green tick when each works (use the core's warnings to detect silence or blocked access).

3. Starting a meeting
   - Tray / menu-bar icon that is always present. When the core reports a meeting app playing audio, show a small notification: "Zoom is playing audio. Start transcribing?" with Start and Not now. For a browser, ask "Is this a meeting?" because it might be a video.
   - Before capture starts: a consent reminder with a one-click "Copy consent message" in Traditional Chinese and English, for the user to paste into the meeting chat. Option to stop showing the reminder only after the user has seen it 3 times.

4. Live transcript window
   - Floating, resizable, optional always-on-top, compact mode.
   - Header: coral "Listening" pill with elapsed time, meeting title (editable), platform, "not in the participant list" note, Stop button.
   - Lines: speaker avatar with initial, speaker name in its colour, timestamp, text. Partial (not final) text shown muted and italic, replaced in place when final. Auto-scroll that pauses when the user scrolls up, with a "Jump to live" button.
   - Click a speaker name to rename them ("S1" to "Amy"); the rename applies to all their lines.
   - Bookmark button and a keyboard shortcut to mark a moment.
   - Status bar: connection state, "Recording saved locally", warnings in plain words (e.g. "Can't hear the meeting. Check that Zoom audio is playing.").

5. After the meeting
   - "Saved. The final transcript will be ready in a few minutes." with buttons to open the recording folder and (once Prompt 8 exists) open the meeting in the library.
   - A simple local list of recent meetings.

6. Settings
   - Microphone, echo cancellation on/off, interface language, meetings folder (with a note that everything stays on this computer unless Google Drive sync is on), auto-delete recordings after 30 / 90 / 365 days / never, launch at login.

7. Accessibility
   - Full keyboard use, visible focus rings, 44 px minimum targets, screen-reader labels on icon buttons, live region for new final lines (not partials).

TESTS
- A mock core so the UI runs in a normal browser with fake segments (pnpm dev:mock), used for component tests with Vitest + Testing Library.
- Tests for: partial-to-final replacement, speaker rename, auto-scroll pause, contrast of every token pair, zh-TW and English strings present for every key.

ACCEPTANCE
- A full flow on a Mac: Zoom call detected, consent copied, live text appears, a speaker renamed, Stop, recording folder opens.
- No Simplified characters anywhere in the zh-TW interface strings (test with packages/zhtw).
- Update docs/progress.md with screenshots.
```

