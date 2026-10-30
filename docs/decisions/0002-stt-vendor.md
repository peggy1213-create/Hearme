# ADR 0002: STT vendor for live and final passes

- Status: **proposed — awaiting the Phase 0 benchmark run on real audio**
- Date: 2026-10-30

> This ADR is written but **not yet decided**. The tooling to decide it exists
> (`packages/zhtw`, `packages/stt`, `tools/benchmark`), but the benchmark has not
> run on real Taiwanese meetings because `benchmark-data/` and vendor API keys are
> not yet in place. The numbers below are intentionally blank. Fill them from
> `reports/benchmark-<date>.md`, then flip Status to `accepted`.

## Context

Hearme must transcribe English and Taiwanese Mandarin, including code-switching in
one sentence, and show Traditional (Taiwan) Chinese only (CLAUDE.md, docs/plan.md).
Transcription runs in two passes: a live streaming pass (target < 2 s delay) and a
final batch pass over the whole recording. The vendor for each pass may differ.

We decide this on measured accuracy on **real zh-TW/English meetings**, not vendor
marketing. The gate for Phase 0 is: STT vendor chosen.

## How we measure (built in this prompt)

- **`packages/zhtw`** — `toTaiwanTraditional` (OpenCC s2twp), `normaliseForScoring`,
  `tokenizeMixed` (Han char tokens + English word tokens), `containsSimplified`.
- **`packages/stt`** — one `SttAdapter` interface (stream + batch) with adapters for
  **Soniox**, **AssemblyAI**, **Deepgram** (language auto-detect / code-switching and
  diarization on, Chinese normalised to Traditional), plus a local **faster-whisper**
  baseline. Vendor API docs and pricing were fetched 2026-10-30; see the doc URLs and
  dates in each adapter's header comment.
- **`tools/benchmark`** — runs every vendor in batch and simulated real-time (audio at
  1× in 100 ms frames) over a corpus folder, and reports, per file and overall:
  - **mixed error rate** on `tokenizeMixed` tokens (the headline for code-switching),
  - **CER** on the Chinese parts only, **WER** on the English parts only,
  - streaming **latency p50/p95** (segment audio-end → arrival),
  - **estimated cost per audio hour** from published pricing,
  - whether any vendor returned **Simplified** characters before conversion.
  - Output: `reports/benchmark-<date>.md` + a CSV.

Published pricing captured 2026-10-30 (diarization-included effective rate):

| Vendor            | Live $/hr | Batch $/hr | Source                               |
| ----------------- | --------- | ---------- | ------------------------------------ |
| Soniox            | 0.12      | 0.10       | soniox.com/pricing (2026-06)         |
| AssemblyAI        | 0.27      | 0.23       | assemblyai.com/pricing (2026-10-27)  |
| Deepgram (Nova-3) | ~0.67     | ~0.31      | deepgram.com/pricing (PAYG, 2026-10) |
| faster-whisper    | local     | local      | on-device; compute cost only         |

## Decision (to be filled from the report)

- **Live pass:** _TBD_ — <vendor>, because <mixed ER / latency / cost from the report>.
- **Final pass:** _TBD_ — <vendor>, because <mixed ER / CER / WER from the report>.
- The two may differ: the live pass weighs latency and streaming accuracy; the final
  pass weighs best achievable accuracy and cost over the whole recording.

_Cite the exact `reports/benchmark-<date>.md` and its numbers here when deciding._

## Consequences (expected)

- The gateway defaults to the chosen vendor per pass via `createSttAdapter`; swapping
  later is one line (ADR 0001).
- faster-whisper is retained regardless, as the local-only mode baseline (v1.1).
- If no vendor meets the under-10% error-rate bar on real audio, Phase 0 does not pass
  its gate: revisit custom vocabulary, audio pre-processing (VAD, echo cancellation),
  or a two-vendor final pass before building Phase 1.

## Alternatives considered

- **One vendor for both passes:** simpler, but gives up latency on the live pass or
  accuracy/cost on the final pass if one vendor is not best at both.
- **Local-only (whisper) for everything:** no per-hour vendor fee and fully private,
  but slower and, pending the benchmark, likely less accurate on code-switched zh-TW;
  kept as the v1.1 local-only mode, not the default.
