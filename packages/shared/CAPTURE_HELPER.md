# Capture helper protocol

Version `1` (`CAPTURE_HELPER_PROTOCOL_VERSION`).

The desktop core runs a small native **capture helper** as a sidecar process, one per
OS:

- macOS 14.2+: [`native/macos-capture`](../../native/macos-capture) (`tapcapture`,
  Swift, Core Audio process taps).
- Windows 10/11: [`native/windows-capture`](../../native/windows-capture) (Rust,
  WASAPI process loopback).

Both helpers speak the **same** protocol described here so the core treats them
interchangeably. The helper **only captures and reports audio**. It does no recording
to disk, no encoding for the gateway, and no streaming; the core (Tauri, Prompt 5)
owns all of that. The helper never joins a meeting and never starts capturing on its
own — the core asks the user first and only then sends `start`.

## Transport

- The core **writes commands** to the helper's **stdin**, one **JSON object per line**
  (UTF-8, `\n`-terminated).
- The helper **writes audio and events** to its **stdout** as **length-prefixed
  binary frames** (below).
- The helper's **stderr** is for human-readable logs only. It **must never** contain
  audio samples or transcript text (see [CLAUDE.md](../../CLAUDE.md)).

Both streams are binary; the core must open stdout in binary mode and must not assume
line buffering.

## Commands (core → helper, stdin, JSON lines)

| `cmd`          | Fields                      | Meaning                                                              |
| -------------- | --------------------------- | ------------------------------------------------------------------- |
| `list_sources` | —                           | Reply with one `sources` event listing detectable meeting apps.     |
| `start`        | `sourceId`, `micDeviceId?`  | Begin capturing the meeting app `sourceId` plus the microphone.     |
| `stop`         | —                           | Stop all capture and tear down taps/devices. Process keeps running. |
| `ping`         | —                           | Reply with a `pong` event. Liveness check.                          |

- `sourceId` — an id from a `sources` event (e.g. `"zoom"`, `"meet-chrome"`). The
  helper resolves it to the set of audio-producing processes for that app at `start`
  and keeps that set current while capturing.
- `micDeviceId` — a Core Audio device UID (macOS) / device id (Windows). Omit for the
  system default input.

Unknown commands are answered with an `error` event and otherwise ignored.

Example stdin lines:

```json
{"cmd":"list_sources"}
{"cmd":"start","sourceId":"zoom","micDeviceId":"AppleUSBAudioEngine:..."}
{"cmd":"stop"}
```

## Binary frames (helper → core, stdout)

Every frame is a little-endian **length prefix** followed by a fixed **header** and a
**payload**:

```
┌───────────────┬──────────────────────────────── header (19 bytes) ────────────────────────────────┬─────────┐
│ length u32    │ type u8 │ track u8 │ hostTimeNs u64 │ sampleRate u32 │ channels u8 │ frameCount u32 │ payload │
└───────────────┴─────────┴──────────┴────────────────┴────────────────┴─────────────┴────────────────┴─────────┘
```

- `length` — number of bytes that follow (header + payload), i.e. `19 + payloadLen`.
- `type` — `1` audio, `2` event.
- `track` — `0` mic, `1` meeting. Ignored for event frames (sent as `0`).
- `hostTimeNs` — the one shared clock (below), in nanoseconds, of the first sample in
  the payload (audio) or of the moment the event was emitted (event).
- `sampleRate` — payload sample rate in Hz (audio); `0` for events.
- `channels` — payload channel count (audio); `0` for events.
- `frameCount` — number of PCM sample frames in the payload (audio); for events, the
  byte length of the JSON payload.

All multi-byte integers are **little-endian**. Audio samples are **little-endian
`float32`**. Both target platforms (macOS arm64/x86-64, Windows x86-64/arm64) are
little-endian; a big-endian host is out of scope.

### Audio payload (`type = 1`)

Interleaved `float32` PCM. The helper mixes the meeting track down to **mono**
(`channels = 1`) and emits the mic track at its native channel count (the core
downmixes if it wants). `sampleRate` is the device's native rate; the core resamples.
Payload length is always `frameCount * channels * 4` bytes.

### Event payload (`type = 2`)

UTF-8 JSON with an `"event"` field. `frameCount` is the JSON byte length.

| `event`                       | Extra fields                             | Meaning                                                                              |
| ----------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------ |
| `sources`                     | `sources: Source[]`                      | Reply to `list_sources`.                                                             |
| `source_started`              | `sourceId`, `bundleId`, `name`           | A known meeting app began producing audio.                                           |
| `source_stopped`              | `sourceId`, `bundleId`, `name`           | It stopped (quit, or went silent).                                                   |
| `capture_started`             | `sourceId`, `sampleRate`, `channels`     | Capture is running after a `start`.                                                  |
| `capture_stopped`             | `sourceId?`                              | Capture ended (after `stop`, or the source disappeared).                             |
| `permission_suspected_missing`| `track`                                  | Several seconds of exact-zero audio while the source is known to be playing — likely a missing OS permission (see below). |
| `device_changed`              | `kind: "output" \| "input"`              | The default output/input device changed; capture was re-pointed automatically.       |
| `pong`                        | —                                        | Reply to `ping`.                                                                     |
| `error`                       | `code`, `message`                        | Non-fatal unless `code` is `fatal_*`. `message` never contains audio/transcript text.|

`Source` is `{ "sourceId": string, "name": string, "bundleId": string, "running": bool, "producingAudio": bool }`.

Example event frame payloads:

```json
{"event":"source_started","sourceId":"zoom","bundleId":"us.zoom.xos","name":"zoom.us"}
{"event":"permission_suspected_missing","track":"meeting"}
```

## The shared clock

`hostTimeNs` on **both** tracks comes from one monotonic host clock, so the core can
align mic and meeting samples that were captured at the same instant:

- macOS: `mach_absolute_time()` scaled to nanoseconds via `mach_timebase_info`.
- Windows: `QueryPerformanceCounter` scaled to nanoseconds via `QueryPerformanceFrequency`.

The clock is arbitrary-zeroed (not wall-clock time); only differences between frames
are meaningful. The helper does **not** know when the meeting started — the core maps
`hostTimeNs` to meeting-relative `timestampMs` for the gateway (see
[PROTOCOL.md](PROTOCOL.md)).

## Permission and silent audio

On macOS, if the app lacks system-audio-recording permission the OS may deliver
**all-zero** buffers instead of an error. The helper watches for several seconds of
exact zeros on a track whose source is known to be playing and emits
`permission_suspected_missing`; it keeps running so permission can be granted without a
restart. The core surfaces this to the user. See the macOS helper's README and ADR for
how the permission is attributed when the helper runs as a sidecar.

## Versioning

Bump `CAPTURE_HELPER_PROTOCOL_VERSION` and this document's version when any frame
layout, command, or event shape changes, and keep both helpers in lockstep.
