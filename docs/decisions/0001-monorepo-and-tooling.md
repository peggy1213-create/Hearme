# ADR 0001: Monorepo and tooling

- Status: accepted
- Date: 2026-10-30

## Context

Hearme spans a Tauri desktop app (Rust + React/TS), a Node gateway, two native audio
helpers (Swift, Rust), and several shared TS libraries. We need shared types across
the TS parts, consistent tooling, and CI that covers both macOS and Windows.

## Decision

- **pnpm workspaces + Turborepo**, **Node 22 LTS**, **TypeScript 5 strict** for all
  TypeScript packages.
- One source of truth for cross-cutting types and the gateway protocol:
  `packages/shared`, defined with **zod** and exported as inferred TS types.
- **Tauri 2** for the desktop shell (small footprint beside a video call); Electron is
  the documented fallback if the team is JS-only.
- Native capture helpers stay **out of the pnpm build**: Swift builds on macOS only,
  the Windows crate on Windows only. The Tauri Rust core is compile-checked per-OS.
- Tooling: ESLint + Prettier (TS), rustfmt + clippy (Rust), SwiftFormat (Swift),
  Vitest for TS tests. LF line endings everywhere.
- **PostgreSQL 16** via docker-compose for accounts and usage only — never meeting
  content.

## Consequences

- `pnpm install && pnpm build && pnpm test` covers the TypeScript workspace on a clean
  clone on both OSes; native and Tauri-core builds run as separate CI jobs on their
  own runners.
- Changing a shared type is a single edit in `packages/shared`; consumers pick it up
  through `workspace:*`.
- Swapping an STT vendor means adding an adapter in `packages/stt`, nothing else.

## Alternatives considered

- **npm/yarn workspaces without Turborepo:** fine, but Turborepo's task graph and
  caching pay off as the number of packages grows.
- **Electron instead of Tauri:** larger install and memory footprint; kept as fallback.
- **A single shared `types.ts` without zod:** loses runtime validation at the gateway
  and file-format boundaries, where untrusted input arrives.
