# tapcapture — Hearme Windows audio capture helper

A Rust command-line helper for **Windows 10 Build 20348+ / Windows 11** that the
desktop core runs as a sidecar. It captures a meeting app's output audio (WASAPI
**process loopback**) plus the microphone as a separate, time-aligned track, **without
joining the meeting**. It only captures and reports audio; recording, encoding and
streaming live in the Tauri core.

It is the Windows counterpart of [`native/macos-capture`](../macos-capture) and speaks
the **same** stdin commands, stdout binary frames, events and `record` subcommand, so
the core treats both interchangeably.

- Protocol (commands, binary frames, events): [`../../packages/shared/CAPTURE_HELPER.md`](../../packages/shared/CAPTURE_HELPER.md)
- Design decision and risks: [`../../docs/decisions/0004-windows-audio-capture.md`](../../docs/decisions/0004-windows-audio-capture.md)

## Build and test (Windows only)

```bash
cd native/windows-capture
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

`cargo test` runs the frame-codec, clock, config-matching, silence-guard, protocol and
session-detection unit tests headlessly (no audio device needed).

## Run

Protocol mode (what the core launches) — JSON commands on stdin, binary frames on
stdout:

```bash
cargo run
# then type, one JSON object per line:
# {"cmd":"list_sources"}
# {"cmd":"start","sourceId":"teams"}
# {"cmd":"stop"}
```

Debug recording — capture a process's output tree and the mic for 30 s into `.\out`:

```bash
cargo run -- record --process chrome.exe --seconds 30 --out .\out
# writes .\out\meeting.wav and .\out\mic.wav
```

The `--process` name is matched case-insensitively against the configured meeting apps'
executables; process loopback captures that process **and its child processes**
(`INCLUDE_TARGET_PROCESS_TREE`), so browser renderers and Teams' WebView2 children are
covered.

## Permissions (read this before wondering why audio is silent)

- **Process loopback** (the meeting track) needs no special permission on supported
  builds. On Windows older than build 20348 the activation fails; the helper reports a
  `fatal_unsupported` error rather than crashing.
- **Microphone** capture is gated by the Windows **microphone privacy** settings
  (Settings → Privacy & security → Microphone, and "Let desktop apps access your
  microphone"). When access is denied the helper emits `{"event":"mic_blocked"}` and
  keeps the meeting track running; the core surfaces this so the user can allow access
  without a restart.
- As on macOS, a track that delivers several seconds of exact-zero audio while its
  source is known to be playing fires `permission_suspected_missing`.

## Layout

| File                | Role                                                                    |
| ------------------- | ----------------------------------------------------------------------- |
| `main.rs`           | Arg dispatch: protocol mode / `record` / `--help`.                      |
| `session.rs`        | Protocol mode: one control thread for commands, the 1 s source monitor, device changes, lifecycle; serialized stdout. |
| `frame.rs`          | Length-prefixed binary frame encode/decode (shares the macOS test vectors). |
| `protocol.rs`       | stdin command parsing, event-JSON builders (sorted keys).               |
| `clock.rs`          | The one shared host clock (`QueryPerformanceCounter` → ns).             |
| `sessions.rs`       | Process snapshot (Toolhelp) + active audio sessions (`IAudioSessionManager2`); app/root-PID and producing-audio resolution. |
| `wasapi.rs`         | COM apartment, format parsing/downmix, the event-driven capture loop.   |
| `meeting.rs`        | Process-loopback client via `ActivateAudioInterfaceAsync` (meeting track, mixed to mono). |
| `mic.rs`            | Shared-mode WASAPI microphone capture.                                  |
| `device_watcher.rs` | `IMMNotificationClient` default-endpoint changes.                       |
| `silence_guard.rs`  | All-zero detection → `permission_suspected_missing`.                    |
| `wav.rs` / `recorder.rs` | `record` debug subcommand.                                         |
| `known-apps.json`   | Meeting apps and their executable names.                                |

## Status

The pure logic (framing, protocol, clock, config, silence guard) and the live
**detection** path are verified on this Windows 11 machine: `cargo build`, `cargo test`
(36 tests), `cargo clippy -- -D warnings` and `cargo fmt --check` all pass, and in
protocol mode `list_sources` correctly reports the real running meeting apps and
`ping`/frame encoding round-trip over stdout.

The **audio capture** paths (process loopback and mic) compile and are wired per
Microsoft's Application Loopback sample, but have **not yet been exercised against a
real meeting** here. The acceptance checks in
[`docs/prompts/03-windows-capture.md`](../../docs/prompts/03-windows-capture.md) — a
real Teams/Meet call producing two clean, aligned WAVs via `record`, and swapping
speakers→headset mid-call — must be run on a machine in an actual call. Likely spots for
a small fixup against real hardware: the loopback buffer duration / format negotiation
in `meeting.rs` and the mic mix-format handling in `mic.rs`.
