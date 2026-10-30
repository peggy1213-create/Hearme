# Local meeting folder format

Version `1` (`MEETING_FOLDER_VERSION` in `src/meetingFolder.ts`).

Every recording, transcript and summary is saved **only** in a local meeting folder
on the user's computer (default root `Documents/Hearme Meetings`, changeable). This
folder is the single source of truth; Hearme's servers keep no copy. See
[CLAUDE.md](../../CLAUDE.md).

One folder per meeting, named by meeting id:

```
Hearme Meetings/
  2026-10-30-weekly-sync-<id>/
    mic.ogg          # the user's microphone track (Opus in Ogg)
    meeting.ogg      # the captured meeting-app audio track
    mixed.ogg        # mic + meeting mixed down, for playback
    meeting.json     # metadata + live-pass segments
    transcript.json  # final-pass transcript (machine-readable)
    transcript.md    # final transcript (human-readable)
    summary.json     # summary + action items (machine-readable)
    summary.md       # summary + action items (human-readable)
    edits.json        # the user's corrections and speaker names
```

## Audio files

- **`mic.ogg`** and **`meeting.ogg`** are the two recorded tracks (Opus in an Ogg
  container). Keeping them separate lets the user's own words always be labelled
  "Me", and lets the final pass re-run each track cleanly.
- **`mixed.ogg`** is the two tracks mixed for click-to-play from any transcript line.
- The app flushes audio to disk at least every 10 seconds so a crash loses little.

## JSON files

Every JSON file includes a `version` field equal to `MEETING_FOLDER_VERSION`. Bump it
and update this document whenever a file's shape changes. Schemas are in
[`src/meetingFolder.ts`](src/meetingFolder.ts).

### `meeting.json` — `MeetingFile`

Metadata and the live segments captured during the call.

```json
{
  "version": 1,
  "meeting": {
    "id": "…",
    "title": "Weekly sync",
    "platform": "zoom",
    "startedAt": "2026-10-30T09:00:00.000Z",
    "endedAt": "2026-10-30T09:45:00.000Z",
    "folderPath": "…/Hearme Meetings/2026-10-30-weekly-sync-…",
    "status": "done",
    "driveSync": { "state": "off" }
  },
  "liveSegments": []
}
```

### `transcript.json` — `TranscriptFile`

The cleaned final-pass transcript (`pass: "final"` segments). `transcript.md` is the
same content rendered for reading.

```json
{ "version": 1, "meetingId": "…", "generatedAt": "…", "segments": [] }
```

### `summary.json` — `SummaryFile`

Summary and action items, in the chosen output language (`en`, `zh-TW`, or `mixed`).
`summary.md` is the same content rendered for reading.

```json
{
  "version": 1,
  "meetingId": "…",
  "generatedAt": "…",
  "language": "zh-TW",
  "summary": "…",
  "actionItems": [{ "id": "a1", "text": "…", "owner": "me", "done": false }]
}
```

### `edits.json` — `EditsFile`

The user's corrections, kept separate from generated output so transcript and summary
can be regenerated without losing them. `speakerNames` maps a speaker key
(`me`, `S1`, …) to a display name.

```json
{
  "version": 1,
  "meetingId": "…",
  "updatedAt": "…",
  "speakerNames": { "me": "Peggy", "S1": "Alex" },
  "segmentEdits": [{ "segmentId": "seg-12", "text": "corrected text" }]
}
```

## Rules

- Chinese text in any file is **Traditional (Taiwan)**; never Simplified.
- Nothing here is uploaded to Hearme's servers. Opt-in Google Drive sync copies this
  folder directly to the user's own Drive (`drive.file` scope).
