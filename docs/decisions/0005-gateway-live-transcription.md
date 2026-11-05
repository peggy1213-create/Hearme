# ADR 0005: Gateway live transcription — vendor defaults, resume, and zero-retention

- Status: **accepted** (vendor defaults are **provisional** — see below)
- Date: 2026-11-03

## Context

Prompt 4 builds the streaming gateway: one WebSocket per meeting that relays two
audio tracks to the STT vendor and streams back live, Traditional-Chinese-safe,
speaker-labelled segments, while storing no meeting content. Several choices were
not fixed by earlier ADRs and are recorded here.

## Decisions

### Live and fallback vendor (provisional)

[ADR 0002](0002-stt-vendor.md) is still **proposed** — the Phase 0 benchmark has not
run on real audio, so no vendor is formally chosen. The gateway must still default
to _something_, so:

- **Live pass primary: Soniox. Automatic fallback: AssemblyAI.** Both are
  configurable via `STT_LIVE_VENDOR` / `STT_FALLBACK_VENDOR`; the final pass uses
  `STT_BATCH_VENDOR` (defaults to the live vendor).
- Rationale (from the ADR 0002 pricing table and vendor capabilities, not a
  benchmark): Soniox has the lowest effective per-hour cost with diarization and
  language identification included and handles Mandarin–English code-switching;
  AssemblyAI is the strongest independent code-switching streamer as a fallback.
- This is **provisional**. When the benchmark decides ADR 0002, update these two env
  defaults to match and flip that ADR to `accepted`. Swapping is one line
  (`createSttAdapter`), so nothing else changes.

Failover is implemented as `FailoverAdapter` in `packages/stt`: if the primary fails
to open or errors mid-meeting, the session transparently switches to the fallback
and keeps going; only if the fallback also fails does the client see an
`stt_unavailable` error. Audio already sent to the failed vendor is not replayed —
the final batch pass fills any gap.

### Resume

The protocol gains an `ack { track, lastSeq }` message (PROTOCOL_VERSION → 2). The
gateway keeps a dropped meeting in memory for a few minutes (`RESUME_WINDOW_MS`); a
reconnect with the same `meetingId` rebinds the running session and re-acks each
track, and the client resends from `lastSeq + 1`. Frames with a seq already seen are
dropped, so no audio is lost or duplicated. Nothing is persisted.

### Audio codec

Frames are Opus by default, decoded gateway-side with **opusscript** — a pure-JS
codec, chosen over native bindings (`@discordjs/opus`) so the gateway builds the same
on every OS with no node-gyp step. The protocol also accepts raw `pcm`, which the
replay tool and tests use to avoid needing an encoder.

### Accounts, usage, and tokens

- PostgreSQL + Drizzle ORM, **only** `users` and `usage` tables. Production uses the
  `postgres` driver via `DATABASE_URL`; tests use **PGlite** (in-process Postgres) so
  the suite runs with no Docker. The same migrations apply to both.
- Sign-in is OAuth 2 + PKCE (Google, Microsoft) with a loopback redirect; the gateway
  holds the client secret. Access and refresh tokens are **stateless signed JWTs**
  (distinguished by audience), so no token table is needed — keeping the "only users
  and usage" rule. A dev-only bypass token is honoured when `NODE_ENV=development`.
- A per-user monthly transcription-minute quota is enforced on `start`.

### Zero-retention / no-training vendor settings

Audio and transcript text exist only in memory while relaying; nothing is written to
disk, the database, logs, traces or crash reports (request logging is disabled and
the `Authorization` header is redacted; a replay test asserts no transcript text
reaches logs or the database). Vendor zero-retention / no-training options to enable
when keys are provisioned, per each vendor's docs (checked 2026-11-03):

- **Soniox:** no-storage / zero-retention streaming; do not opt into model
  improvement.
- **AssemblyAI:** account-level "do not store / no training"; EU/region options if
  needed.
- **Deepgram:** `mip_opt_out` (model-improvement opt-out) and no-retention tier.

These are account/request settings, not code paths, so they are recorded here rather
than hard-coded; confirm them against each vendor's current console when the keys are
added.

## Consequences

- The gateway is vendor-agnostic; the benchmark result changes two env values.
- Batch final-pass audio is streamed to a temp file only because the vendor batch API
  is path-based, and the file is deleted immediately. A fully streaming batch upload
  (no disk touch at all) is a worthwhile follow-up.

## Alternatives considered

- **Keep STT streams torn down on disconnect and rebuild on resume:** simpler, but
  diarization labels reset and words near the cut can be lost. Keeping the session
  alive for the resume window avoids both.
- **Native Opus decoder:** lower CPU, but native builds are fragile across macOS /
  Windows / Linux and CI; pure-JS is fast enough for 20 ms frames.
- **Stateful refresh tokens in a table:** easier revocation, but adds a third table
  and conflicts with the "only users and usage" rule; revisit if revocation is needed.
