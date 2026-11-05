# Gateway WebSocket protocol

Version `2` (`PROTOCOL_VERSION` in `src/protocol.ts`).

> **v2 changes:** `start` gained `vocabulary`; `codec` now accepts `pcm` as well as
> `opus`; a new `ack` server message reports how far each track has been consumed,
> which is what reconnecting clients use to resume (see **Resume** below).

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
  │ ◀── ack (mic, lastSeq -1) ── │  resume point per track
  │ ◀── ack (meeting, -1) ────── │
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

## Resume

If the socket drops, the client reconnects and sends `start` again with the **same
`meetingId`**. The gateway keeps a short in-memory resume window per meeting (a few
minutes), so it recognises the meeting, replies with an `ack` per track giving the
last sequence number it has, and the client resends from `lastSeq + 1`. The gateway
ignores any `audio` frame whose `seq` it has already seen, so no audio is lost or
duplicated. Nothing is persisted; if the window has expired the meeting starts fresh.

## Client → server

### `start`

Sent once, first.

| Field             | Type                                      | Notes                                                            |
| ----------------- | ----------------------------------------- | ---------------------------------------------------------------- |
| `type`            | `"start"`                                 |                                                                  |
| `protocolVersion` | `1`                                       | Must match the gateway's.                                        |
| `meetingId`       | string                                    |                                                                  |
| `platform`        | `zoom \| meet \| teams \| webex \| other` |                                                                  |
| `languages`       | `("en" \| "zh-TW" \| "mixed")[]`          | Bias hint; defaults to `["en", "zh-TW"]`.                        |
| `sampleRate`      | int (Hz)                                  | Sample rate of the decoded audio.                                |
| `codec`           | `"opus" \| "pcm"`                         | `opus` (default, decoded gateway-side) or raw 16-bit mono `pcm`. |
| `vocabulary`      | string[]                                  | Optional custom terms passed to the vendor.                      |

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

### `ack`

Reports how far a track has been consumed, for resume. Sent after `start` and as
frames are relayed.

| Field     | Type             | Notes                                                |
| --------- | ---------------- | ---------------------------------------------------- |
| `type`    | `"ack"`          |                                                      |
| `track`   | `mic \| meeting` |                                                      |
| `lastSeq` | int (≥ -1)       | Highest contiguous `seq` consumed; `-1` if none yet. |

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
