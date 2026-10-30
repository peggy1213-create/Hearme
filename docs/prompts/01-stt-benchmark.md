# Prompt 1 — Speech-to-text benchmark (Phase 0)

Builds the tool that decides which speech-to-text vendor to use, measured on your own English and zh-TW meetings.

```text
Read CLAUDE.md and docs/plan.md. This is Phase 0: we must pick the speech-to-text (STT) vendor using real Taiwanese meetings before building anything else.

Explore first, then propose a plan and wait for my OK.

GOAL
Build packages/zhtw, packages/stt and tools/benchmark, then run the benchmark and write a recommendation.

1. packages/zhtw
   - toTaiwanTraditional(text): convert with OpenCC (s2twp profile), so Simplified becomes Traditional and mainland terms become Taiwan terms.
   - normaliseForScoring(text): full-width to half-width for Latin and digits, remove punctuation, lowercase English, collapse spaces.
   - tokenizeMixed(text): Chinese characters become single tokens, English words become word tokens. This lets us score code-switched speech.
   - Unit tests with mixed sentences such as "這個 sprint 我們先 focus 在 Core Audio".

2. packages/stt
   - An SttAdapter interface with two modes: stream(audio frames) -> async segments, and transcribeFile(path) -> segments. Segments use TranscriptSegment from packages/shared.
   - Adapters for Soniox, AssemblyAI and Deepgram, plus a local baseline using faster-whisper (Python subprocess is fine).
   - Before writing each adapter, fetch that vendor's CURRENT API docs. APIs change; do not rely on memory. Note the doc URL and date in a comment.
   - Turn on each vendor's language auto-detection or code-switching mode and speaker diarization. Pass all Chinese output through toTaiwanTraditional.

3. tools/benchmark CLI
   - Input: a folder of audio files, each with a reference transcript (.txt). Add a helper command that creates reference-transcript templates.
   - For each vendor, run batch mode and simulated real-time streaming (feed audio at 1x speed in 100 ms frames).
   - Metrics per file and overall: mixed error rate (on tokenizeMixed tokens), CER on the Chinese parts only, WER on the English parts only, streaming latency p50/p95 (time from word spoken to word received), and estimated cost per audio hour from the vendor's published pricing (fetch it; note the date).
   - Also report: did any output contain Simplified characters before conversion?
   - Output: reports/benchmark-<date>.md with tables, plus a CSV.

4. Run it on the audio in ./benchmark-data (I will supply it). If the folder is missing or empty, stop and tell me rather than inventing data.

5. Write docs/decisions/0001-stt-vendor.md: the recommended vendor for live mode and for the final pass (they may differ), the numbers behind it, and the risks.

ACCEPTANCE
- Unit tests pass for zhtw and the scoring maths.
- pnpm benchmark --data ./benchmark-data produces the report for all four engines.
- The ADR names one vendor per pass and cites the report.

Do not put API keys in code. Read them from .env.
```

