#!/usr/bin/env python3
"""Local faster-whisper baseline for the Hearme STT benchmark.

Invoked by packages/stt WhisperAdapter as:

    python -I whisper_transcribe.py <audio_path> [model_name]

Prints a single JSON object to stdout:

    {"segments": [{"start": <sec>, "end": <sec>, "text": "..."}], "language": "zh"}

On a missing dependency it prints a JSON error to stderr and exits non-zero, so
the adapter can report "whisper unavailable" instead of crashing the benchmark.
Chinese normalisation to Traditional (Taiwan) happens on the TypeScript side.
"""

import json
import sys


def main() -> int:
    if len(sys.argv) < 2:
        print(json.dumps({"error": "usage: whisper_transcribe.py <audio> [model]"}), file=sys.stderr)
        return 2

    audio_path = sys.argv[1]
    model_name = sys.argv[2] if len(sys.argv) > 2 else "large-v3"

    try:
        from faster_whisper import WhisperModel
    except Exception as exc:  # noqa: BLE001 - report any import failure uniformly
        print(json.dumps({"error": f"faster-whisper not installed: {exc}"}), file=sys.stderr)
        return 2

    try:
        model = WhisperModel(model_name, device="auto", compute_type="default")
        # language=None lets whisper auto-detect; the benchmark meetings mix
        # English and Taiwanese Mandarin.
        segments, info = model.transcribe(audio_path, beam_size=5, language=None)
        out = [
            {"start": float(seg.start), "end": float(seg.end), "text": seg.text}
            for seg in segments
        ]
    except Exception as exc:  # noqa: BLE001
        print(json.dumps({"error": f"whisper transcription failed: {exc}"}), file=sys.stderr)
        return 1

    json.dump({"segments": out, "language": info.language}, sys.stdout, ensure_ascii=False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
