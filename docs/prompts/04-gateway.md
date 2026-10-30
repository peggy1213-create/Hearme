# Prompt 4 — Streaming gateway and live transcription

Builds the server that receives audio from the desktop app, sends it to the chosen speech-to-text vendor, and returns live transcript lines. Also adds sign-in and a small accounts database. No meeting content is stored on the server.

```text
Read CLAUDE.md, docs/plan.md, packages/shared/PROTOCOL.md and docs/decisions/0001-stt-vendor.md (the vendor chosen in Phase 0). Work in services/gateway, packages/shared and packages/stt.

Explore first, then propose a plan and wait for my OK.

GOAL
A Node + Fastify service that turns two audio tracks per meeting into live, labelled, Traditional-Chinese-safe transcript segments, and sends them back to the desktop app. The desktop app saves them; the gateway keeps nothing.

1. Protocol (update PROTOCOL.md if needed, and keep zod schemas in packages/shared in sync)
   - One WebSocket per meeting: wss://<host>/v1/live?meetingId=...
   - Client sends: start { meetingId, platform, languageHint?, vocabulary? }, then binary audio frames: track id, sequence number, capture timestamp, and Opus-encoded 16 kHz mono 20 ms audio. Then stop.
   - Server sends: segment (partial or final TranscriptSegment), ack { track, lastSeq }, error { code, message, retryable }, ended.
   - Resume: if the socket drops, the client reconnects with the same meetingId and resends from lastSeq + 1. Keep a short in-memory resume window per meeting (a few minutes); no audio is lost or duplicated.

2. Transcription
   - Open one vendor stream per track. Mic track: speaker is always "me". Meeting track: vendor diarization labels map to S1, S2, ... and stay stable for the whole meeting.
   - Use the live-mode vendor from the ADR through the SttAdapter in packages/stt. Keep the second-best vendor as an automatic fallback if the first fails mid-meeting.
   - Pass the user's custom vocabulary (names, terms) to the vendor when supported.
   - Run every segment through toTaiwanTraditional from packages/zhtw before sending. Add a test that fails if any Simplified-only character reaches a client.
   - Follow each vendor's guidance on silence: the desktop only sends voiced audio, so handle gaps without closing the stream or paying for idle time where possible.
   - Use each vendor's zero-retention or no-training settings where offered, and note them in an ADR.

3. No meeting content on the server
   - Audio and transcript text exist only in memory while relaying. Never write them to disk, the database, logs, traces or crash reports. Add a test that runs a replay and then checks that no transcript text appears in logs or the database.

4. Accounts and usage
   - Sign-in with Google and Microsoft (OAuth 2 with PKCE). The desktop app opens the system browser and receives the code on a loopback redirect. Issue short-lived JWT access tokens plus refresh tokens.
   - A dev-only bypass token, enabled only when NODE_ENV=development.
   - PostgreSQL + Drizzle ORM with migrations. Tables: users, usage (per user per month: live minutes, final-pass minutes, LLM tokens). Nothing else.
   - Per-user monthly transcription-hour quota, enforced on start.

5. Operations
   - API keys only from environment variables. Log only ids, durations and error codes.
   - OpenTelemetry traces and metrics: end-to-end latency per segment, vendor errors, active meetings.
   - Dockerfile and a health endpoint.

TESTS
- A replay tool: pnpm replay --wav meeting.wav --mic mic.wav streams two WAV files at real-time speed through a fake client and prints the live transcript.
- Integration tests with a mocked vendor: reconnect and resume, vendor failover, quota exceeded, invalid token.
- A load test for 50 simultaneous meetings using the mock vendor.

ACCEPTANCE
- Replaying a mixed zh-TW/English recording shows live text within 2 s (p95) and only Traditional characters.
- Killing the WebSocket mid-replay and reconnecting loses no words.
- After a replay, the database holds only users and usage rows.
- Update docs/progress.md.
```

