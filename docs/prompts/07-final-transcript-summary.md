# Prompt 7 — Final transcript and summary, saved locally

Builds the second transcription pass on the full recording, then the summary and action items. Audio and text pass through the gateway only for processing; every result is saved in the local meeting folder.

```text
Read CLAUDE.md, docs/plan.md, docs/decisions/ (especially the STT vendor ADR), packages/shared/MEETING_FOLDER.md and services/gateway. Work in services/gateway, packages/stt, packages/shared and apps/desktop.

Explore first, then propose a plan and wait for my OK.

GOAL
Within a few minutes of a meeting ending, replace the live transcript with a more accurate final one and add a summary, decisions and action items. Every result is written to the local meeting folder. The gateway processes and forgets.

1. Settings (desktop)
   - "Improve the transcript after each meeting": ON by default. At first run, explain in plain words that the recording is sent through Hearme's gateway to the speech-to-text service to be transcribed again, and that neither keeps a copy.
   - When it is off, the live transcript becomes the final one and only the summary step runs.

2. Final transcription (gateway endpoint /v1/final)
   - The desktop streams mic.ogg and meeting.ogg to the gateway (resumable; retry later if offline). The gateway passes them straight to the final-pass vendor's batch API (from the ADR), using zero-retention settings, and keeps them in memory or a temp file deleted as soon as the request ends.
   - Mic track = "me"; meeting track with diarization.
   - Merge both tracks into one timeline. Remove echo duplicates: if the same words appear on both tracks within about 1 s, keep the meeting-track copy unless it is the user's own voice.
   - Pass all Chinese through toTaiwanTraditional. Chinese text uses full-width punctuation; English uses half-width.
   - Return the merged segments to the desktop. Record only usage minutes in the database.

3. Writing results (desktop)
   - Write transcript.json and a readable transcript.md into the meeting folder (format from MEETING_FOLDER.md). Keep the user's speaker names from edits.json by mapping final speakers to live speakers by time overlap.
   - Write atomically (temp file then rename) so a crash never leaves a half-written file.

4. Summary (gateway endpoint /v1/summary)
   - The desktop sends the final transcript text; the gateway calls the Anthropic API and returns JSON. Read current model names from Anthropic's docs and make the model an environment variable.
   - Output, validated with zod: summary (5–8 bullet points), decisions, action items { text, owner?, dueDate? }, open questions, topics with start times.
   - Write it in the meeting's main language (by share of speech) unless the user set a preferred summary language. zh-TW output must be Traditional with Taiwan wording; check with packages/zhtw.
   - Long meetings: summarise in time-based chunks, then combine. Owners must be names actually said in the meeting; never guess.
   - Keep prompts in versioned files under services/gateway/prompts/.
   - The desktop writes summary.json and summary.md into the meeting folder.

5. Status
   - Per meeting in meeting.json: transcribing, summarising, ready, failed (with a retry button in the UI). Jobs are resumable after the app restarts.
   - Show "Final transcript ready" when done.

TESTS
- Golden tests: two fixture recordings (one English, one mixed zh-TW/English) with expected merged output, against a mocked vendor.
- Echo-duplicate removal, speaker-name mapping, chunked summaries, atomic writes.
- A test that the gateway leaves no audio or transcript text on disk, in the database or in logs after /v1/final and /v1/summary.
- A test that summaries never contain Simplified-only characters.

ACCEPTANCE
- A real 30-minute meeting has transcript.md and summary.md in its folder within 5 minutes of stopping.
- Turning off the network during processing pauses and later resumes without losing anything.
- Update docs/progress.md.
```

