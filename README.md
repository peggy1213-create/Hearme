# Hearme

Hearme is a desktop app for macOS and Windows that transcribes meetings in Zoom, Google Meet, Microsoft Teams and Webex **without joining the meeting**. It listens to the audio already playing on your computer plus your microphone, records it, and produces a live transcript in English and Taiwanese Mandarin (Traditional Chinese), then a cleaner final transcript, a summary and action items.

Everything is saved in a local folder on your computer. Syncing to your own Google Drive is optional.

## Status

Planning. The code has not been started yet.

## Docs

- [Development plan](docs/plan.md): capture strategy, architecture, stack, features, roadmap, privacy, risks
- [Claude Code prompts](docs/prompts/README.md): ten prompts that build the app step by step, starting with [Prompt 0](docs/prompts/00-repo-setup.md)
- [Colour palette](docs/design/palette.md): the "Quiet Signal" UI colours

## Getting started

1. Open this folder in Claude Code.
2. Run Prompt 0 (`docs/prompts/00-repo-setup.md`). It creates the monorepo and `CLAUDE.md` with the product rules.
3. Continue with the prompts in order. See `docs/prompts/README.md` for what each needs and where to run it.
