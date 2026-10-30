import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import type { TranscriptSegment } from '@hearme/shared';
import type {
  SttAdapter,
  SttBatchOptions,
  SttStream,
  SttStreamHandlers,
  SttStreamOptions,
} from '../adapter.js';
import { SttNotImplementedError } from '../adapter.js';
import { detectLang, nextSegmentId, normaliseText, speakerKey } from '../util.js';

/**
 * Local baseline using faster-whisper via a Python subprocess.
 *
 * This is the on-device reference point for the Phase 0 benchmark and the basis
 * for the planned local-only mode (docs/plan.md). It is batch-only — whisper is
 * not a streaming model — so `openStream` throws. faster-whisper docs:
 * https://github.com/SYSTRAN/faster-whisper (consulted 2026-10-30).
 *
 * Needs Python with `faster-whisper` installed (`pip install faster-whisper`).
 * When it is missing, `transcribeBatch` throws a clear error and the benchmark
 * records whisper as unavailable rather than failing the whole run.
 */

interface WhisperSegment {
  start: number;
  end: number;
  text: string;
}

const SCRIPT_PATH = fileURLToPath(new URL('../../python/whisper_transcribe.py', import.meta.url));

export class WhisperAdapter implements SttAdapter {
  readonly name = 'whisper';
  readonly supportsCodeSwitching = true;
  private readonly model: string;
  private readonly pythonBin: string;

  constructor(config: { model?: string; pythonBin?: string } = {}) {
    this.model = config.model ?? process.env.WHISPER_MODEL ?? 'large-v3';
    this.pythonBin = config.pythonBin ?? process.env.PYTHON_BIN ?? 'python';
  }

  openStream(_options: SttStreamOptions, _handlers: SttStreamHandlers): Promise<SttStream> {
    // whisper is not a streaming model; only the final (batch) pass uses it.
    throw new SttNotImplementedError(this.name, 'openStream');
  }

  async transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]> {
    const normalize = options.normalizeChinese ?? true;
    const { segments } = await this.runPython(options.audioPath);
    return segments
      .map((seg): TranscriptSegment | undefined => {
        const text = normaliseText(seg.text.trim(), normalize);
        if (!text) return undefined;
        return {
          id: nextSegmentId(options.meetingId),
          meetingId: options.meetingId,
          track: 'meeting',
          speaker: speakerKey(undefined), // whisper has no diarization
          startMs: Math.round(seg.start * 1000),
          endMs: Math.round(seg.end * 1000),
          text,
          lang: detectLang(text),
          isFinal: true,
          pass: 'final',
        };
      })
      .filter((s): s is TranscriptSegment => s !== undefined);
  }

  private runPython(audioPath: string): Promise<{ segments: WhisperSegment[]; language: string }> {
    return new Promise((resolve, reject) => {
      const proc = spawn(this.pythonBin, ['-I', SCRIPT_PATH, audioPath, this.model]);
      let stdout = '';
      let stderr = '';
      proc.stdout.on('data', (d: Buffer) => (stdout += d.toString()));
      proc.stderr.on('data', (d: Buffer) => (stderr += d.toString()));
      proc.on('error', (err) =>
        reject(new Error(`failed to launch ${this.pythonBin}: ${err.message}`)),
      );
      proc.on('close', (code) => {
        if (code !== 0) {
          let detail = stderr.trim();
          try {
            detail = (JSON.parse(stderr) as { error?: string }).error ?? detail;
          } catch {
            // stderr was not JSON; use it as-is.
          }
          reject(new Error(`whisper exited ${code}: ${detail}`));
          return;
        }
        try {
          resolve(JSON.parse(stdout) as { segments: WhisperSegment[]; language: string });
        } catch (err) {
          reject(
            new Error(
              `could not parse whisper output: ${err instanceof Error ? err.message : err}`,
            ),
          );
        }
      });
    });
  }
}
