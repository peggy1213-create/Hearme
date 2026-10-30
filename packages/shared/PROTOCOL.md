# Gateway WebSocket protocol

Version `1` (`PROTOCOL_VERSION` in `src/protocol.ts`).

The desktop app opens **one WebSocket per meeting** to the Hearme gateway. The
gateway relays audio to the speech-to-text (STT) vendor and streams transcript
segments back. **The gateway stores nothing**: audio and text exist only in memory
while they are relayed. See [CLAUDE.md](../../CLAUDE.md).

The canonical message shapes are the zod schemas in
[`src/protocol.ts`](src/protocol.ts); this document explains them.

## Session lifecycle

```
client                          gateway
  │ ── start ──────────────────▶ │  open STT stream
  │ ── audio (mic, seq 0) ─────▶ │  relay
  │ ── audio (meeting, seq 0) ─▶ │  relay
  │ ◀──────────────── segment ── │  live transcript, under ~2 s
  │ ── audio … ────────────────▶ │
  │ ◀──────────────── segment ── │
  │ ── stop ───────────────────▶ │  flush STT
  │ ◀──────────────── ended ──── │  close
```

An `error` may arrive at any time. If `fatal` is true the session is over and the
socket closes.

## Client → server

### `start`

Sent once, first.

| Field             | Type                                      | Notes                                     |
| ----------------- | ----------------------------------------- | ----------------------------------------- |
| `type`            | `"start"`                                 |                                           |
| `protocolVersion` | `1`                                       | Must match the gateway's.                 |
| `meetingId`       | string                                    |                                           |
| `platform`        | `zoom \| meet \| teams \| webex \| other` |                                           |
| `languages`       | `("en" \| "zh-TW" \| "mixed")[]`          | Bias hint; defaults to `["en", "zh-TW"]`. |
| `sampleRate`      | int (Hz)                                  | Sample rate of the decoded audio.         |
| `codec`           | `"opus"`                                  | Audio is Opus-encoded.                    |

### `audio`

One chunk of audio for a track. Flush often enough that a crash loses little (the
app also writes audio to disk at least every 10 s).

| Field         | Type             | Notes                                                                                                         |
| ------------- | ---------------- | ------------------------------------------------------------------------------------------------------------- |
| `type`        | `"audio"`        |                                                                                                               |
| `track`       | `mic \| meeting` |                                                                                                               |
| `seq`         | int              | Monotonic per track, from 0.                                                                                  |
| `timestampMs` | int              | Ms from meeting start of the first sample.                                                                    |
| `data`        | string           | base64 Opus. Implementations may instead send raw binary frames; this JSON form is the canonical description. |

### `stop`

Ends the session; the gateway flushes the STT stream and replies with `ended`.

| Field       | Type     |
| ----------- | -------- |
| `type`      | `"stop"` |
| `meetingId` | string   |

## Server → client

### `segment`

A live-pass transcript segment (`pass: "live"`). The payload is a
`TranscriptSegment` (see [`src/segment.ts`](src/segment.ts)). Chinese text is already
Traditional (Taiwan); run any vendor output through `packages/zhtw` before emitting.

### `error`

| Field     | Type                                                                           | Notes                                                             |
| --------- | ------------------------------------------------------------------------------ | ----------------------------------------------------------------- |
| `type`    | `"error"`                                                                      |                                                                   |
| `code`    | `bad_request \| unauthorized \| quota_exceeded \| stt_unavailable \| internal` |                                                                   |
| `message` | string                                                                         | Human-readable. **Must not contain transcript or audio content.** |
| `fatal`   | boolean                                                                        | If true, the session ends.                                        |

### `ended`

Final message of a session.

| Field          | Type      | Notes                               |
| -------------- | --------- | ----------------------------------- |
| `type`         | `"ended"` |                                     |
| `meetingId`    | string    |                                     |
| `segmentCount` | int       | Live segments emitted this session. |

## Notes

- The **final** transcription pass does not use this protocol: after the meeting the
  app uploads the full recording to the gateway's batch endpoint, which relays it to
  the batch STT model and returns `pass: "final"` segments. The batch endpoint is
  defined alongside the gateway, not here.
- Vendor API keys live only on the gateway.
