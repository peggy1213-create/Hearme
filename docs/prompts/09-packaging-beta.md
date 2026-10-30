# Prompt 9 — Packaging, signing, auto-update and beta hardening

Turns the working app into something 30–50 beta users can install and trust. You will need an Apple Developer account and a Windows code-signing certificate before the signing steps.

```text
Read CLAUDE.md, docs/plan.md and docs/progress.md. This prompt gets us ready for the private beta (Phase 3). Work across the repo.

Explore first, then propose a checklist-style plan and wait for my OK. Ask me for any certificates or accounts you need; never invent or hard-code secrets.

1. Desktop packaging
   - macOS: universal .dmg, hardened runtime, the correct entitlements and usage descriptions (microphone and system-audio capture), the tapcapture sidecar signed, then notarised and stapled. Document every step in docs/release.md.
   - Windows: signed MSI or NSIS installer with the sidecar signed too.
   - Tauri auto-updater with signed update manifests and a beta release channel.
   - CI workflows that build, sign and publish both installers on a version tag, with secrets from GitHub Actions secrets.

2. Deployment
   - Deploy the gateway to a managed cloud in the Tokyo or Taiwan region (propose the provider and record an ADR). Managed PostgreSQL with backups for accounts and usage only, TLS everywhere, and no object storage. Add a check that the deployed service never writes audio or transcript text to disk.
   - Separate staging and production environments.

3. Reliability
   - Crash reporting for desktop and services (e.g. Sentry) with audio and transcript text scrubbed. Add a test that a sample transcript string never appears in a captured event.
   - Dashboards: live latency p95, vendor error rate, meetings per day, job failures, cost per meeting hour.
   - Soak test: a 2-hour meeting on a laptop with lid-close, Wi-Fi drop and headphone swap; record CPU, memory and battery use in docs/progress.md. Fix leaks.

4. Privacy and consent review
   - Check every path where audio or text leaves the device and list it in docs/privacy-data-flows.md (what, where, how long kept, who can access).
   - Confirm the recording indicator is visible whenever capture runs, including when the window is minimised (tray icon state).
   - Prepare the Google OAuth consent screen and verification for the sign-in and drive.file scopes.
   - Draft a plain-language privacy policy and terms for review by a lawyer (mark clearly as a draft).

5. Beta support
   - In-app "Send feedback" that attaches logs only with the user's permission, never audio or transcripts.
   - A short beta onboarding guide in Traditional Chinese and English (docs/beta-guide.md).

ACCEPTANCE
- A fresh Mac and a fresh Windows PC can install, grant permissions, transcribe a real meeting, and receive an update from the beta channel.
- The success metrics in docs/plan.md have a dashboard or report showing each one.
- Update docs/progress.md with what is ready and what is still open before inviting beta users.
```

**After Prompt 9:** run the beta, then return to the roadmap in `docs/plan.md` for v1.1 (calendar, local-only mode, ask-the-meeting chat) and write new prompts in the same shape.
