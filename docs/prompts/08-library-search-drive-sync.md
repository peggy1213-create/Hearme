# Prompt 8 — Library, search, playback, export and Google Drive sync

Builds the place to read, search, correct and export meetings, all inside the desktop app, plus opt-in sync of meeting folders to the user's own Google Drive.

```text
Read CLAUDE.md, docs/plan.md and packages/shared/MEETING_FOLDER.md. Work in apps/desktop (UI and Rust core).

Explore first. Fetch Google's current docs for the Drive API v3, the drive.file scope, OAuth for installed (desktop) apps with a loopback redirect, and resumable uploads. Then propose the screens, data flow and module list and wait for my OK.

GOAL
Everything a user does with past meetings happens in the desktop app, using the local meeting folders. Optionally, those folders are copied to the user's own Google Drive.

1. Local index
   - A SQLite database in the app data folder that indexes the meeting folders: titles, dates, platforms, status, and transcript and summary text in an FTS5 full-text index.
   - Chinese search: use the FTS5 trigram tokenizer, with a LIKE fallback for 1–2 character queries. Normalise queries and text with packages/zhtw so a Simplified query still finds Traditional text.
   - The folders are the source of truth. The index can always be rebuilt from them (Settings > Rebuild index), and it notices folders added, changed or deleted while the app was closed.

2. Screens (Traditional Chinese / English, palette from CLAUDE.md)
   - Library: meetings newest first with title, date, length, platform, status, Drive sync state, and the first summary point. Filter by platform and date.
   - Meeting view: summary, decisions, action items (tick off, edit owner and date), then the transcript with speaker colours and timestamps. Search inside the meeting with highlighted hits.
   - Search across all meetings in Chinese and English. Results show the matching line with context and open that moment.

3. Playback
   - Click any line to play mixed.ogg from that moment; highlight the current line while playing. Keyboard: space to play or pause, arrows to skip 5 s.

4. Editing
   - Fix words in a line and rename speakers. Save to edits.json; never overwrite transcript.json, so the original is kept.
   - "Add to my vocabulary" for names and terms the transcript got wrong. Store the vocabulary locally and send it with future live and final transcription requests.

5. Export and copy
   - Markdown, .docx, .srt and .vtt with Traditional characters and speaker names; copy summary; reveal the audio files in Explorer or Finder.

6. Google Drive sync (opt-in, OFF by default)
   - Settings > Google Drive: connect a Google account with ONLY the drive.file scope, using OAuth for desktop apps (PKCE, loopback redirect). Tokens stay in the OS keychain on this computer; they are never sent to Hearme's gateway.
   - Hearme creates a "Hearme Meetings" folder in the user's Drive and mirrors each meeting folder into it after the final transcript and summary are ready. Choose per meeting what to sync: everything, or transcript and summary only (no audio).
   - One-way, computer to Drive. Re-upload files that change locally (edits, summary). Resumable uploads with retry; show sync state per meeting and an overall status.
   - Deleting a meeting on the computer asks whether to delete the Drive copy too. Disconnecting Drive stops syncing and leaves existing Drive files alone.
   - Sync goes directly from the computer to Google. Add a test that no Drive request is made through the gateway.

TESTS
- Index rebuild from fixture folders; Chinese and English search, including a Simplified query finding Traditional text and 1–2 character queries.
- Edits never change transcript.json; exports keep Traditional characters.
- Drive sync against a mocked Drive API: first sync, changed files, retry after network loss, disconnect, delete with and without the Drive copy.
- Then one manual test with a real Google account; record the result in docs/progress.md.

ACCEPTANCE
- A meeting from Prompt 7 can be found by searching a Chinese word, played from any line, corrected, exported to .docx, and synced to Google Drive.
- With sync off, no request goes to Google at all.
- Update docs/progress.md.
```

