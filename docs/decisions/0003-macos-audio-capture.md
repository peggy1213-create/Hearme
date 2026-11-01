# ADR 0003: macOS audio capture via Core Audio process taps

- Status: accepted
- Date: 2026-11-01

## Context

Hearme must capture a meeting app's output audio plus the microphone on macOS
**without joining the meeting** and without installing a virtual audio driver
(CLAUDE.md). The desktop core (Tauri) runs a native helper as a sidecar to do the
capture. macOS 14.2+ exposes Core Audio **process taps** (`CATapDescription`,
`AudioHardwareCreateProcessTap`), which capture the output of specific processes and
can be read like a microphone when placed in an **aggregate device**.

Two things about this API shape the design and carry risk:

1. A tap delivers no audio on its own — it must be added to an aggregate device, and
   the system permission prompt fires on the **first IO start** of that device, not
   when the tap is created.
2. macOS attributes the system-audio-recording permission to the **responsible
   process**, not necessarily the binary that calls the API. A bare CLI launched from
   a terminal is attributed to the terminal (which has no usage-description key); the
   request is denied and the tap then delivers **all-zero buffers** rather than an
   error. This directly affects a sidecar launched by the Tauri app.

## Decision

- **`tapcapture`**, a Swift CLI in `native/macos-capture`, is the helper. It only
  captures and reports audio; recording, encoding and streaming stay in the core.
- **Meeting track:** a `CATapDescription(monoMixdownOfProcesses:)` over _every_
  audio-producing process that belongs to the chosen app (main process **and** its
  audio helpers / browser renderers, matched by bundle-ID prefix), `muteBehavior =
.unmuted` so the user still hears the meeting, inside a **private** aggregate device
  (`kAudioAggregateDeviceIsPrivateKey`) so nothing appears in Audio MIDI Setup.
- **Mic track:** `AVAudioEngine` input node, captured as a separate track.
- **One shared clock:** both tracks stamp frames with `mach_absolute_time` → ns so the
  core can align them. Protocol in `packages/shared/CAPTURE_HELPER.md`.
- **Permission attribution (sidecar):** ship `tapcapture` **inside** the signed `.app`
  bundle (`Contents/MacOS` or a sidecar location), launched as a child of the app, and
  declare `NSAudioCaptureUsageDescription` **and** `NSMicrophoneUsageDescription` in
  the **app** bundle's Info.plist. The responsible process then resolves to the host
  app, so the user sees one prompt attributed to Hearme. **This must be verified on a
  real Mac** — confirm that running `tapcapture` as a Tauri child is attributed to the
  app and not to Terminal.
- **Missing-permission detection:** because a denied tap yields silent zeros, the
  helper watches for several seconds of exact-zero audio while the source is known to
  be playing and emits `permission_suspected_missing`.
- **Cleanup:** destroy tap + aggregate device on `stop`, on signal (SIGINT/SIGTERM via
  `DispatchSource`), and sweep orphaned devices by UID prefix on launch. Private
  aggregates are also torn down by the system when the process dies.

## Consequences

- No virtual audio driver and no kernel extension; capture works per-app across Zoom,
  Teams, Webex and browsers for Meet with one mechanism.
- Packaging is load-bearing: if the helper is ever run outside the signed app bundle
  (e.g. from a terminal during development), expect denied permission and zero audio.
  `tapcapture record` surfaces this, and the live path emits
  `permission_suspected_missing`.
- Bundle IDs for the meeting apps and, especially, the set of **child processes** that
  actually emit audio (browser renderers vary by browser and version) must be verified
  on real machines and kept current in `known-apps.json`. On macOS 26 the newer
  `CATapDescription.bundleIDs` could replace prefix matching; we target 14.2+ and match
  processes ourselves for now.

## Alternatives considered

- **Full-system tap** (`monoGlobalTapButExcludeProcesses:`): captures everything,
  including notifications and music, and can't separate the meeting. Kept only as a
  fallback for apps whose per-process audio can't be resolved.
- **Virtual audio driver (e.g. a BlackHole-style device):** needs an install step and
  a system extension; heavier and more fragile across OS updates than process taps.
- **ScreenCaptureKit audio:** captures system/app audio too, but is oriented around
  screen capture, shows a capture indicator tied to screen recording, and is a coarser
  fit than per-process taps for meeting-only audio.
