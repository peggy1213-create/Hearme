import type { TranscriptSegment } from '@hearme/shared';
import type {
  SttAdapter,
  SttBatchOptions,
  SttStream,
  SttStreamHandlers,
  SttStreamOptions,
} from '../adapter.js';
import { detectLang, nextSegmentId, normaliseText, speakerKey } from '../util.js';

/**
 * Deterministic in-memory adapter used by the gateway integration tests, the load
 * test and `replay --vendor mock`. It transcribes nothing real: it emits a scripted
 * line every `framesPerSegment` audio chunks, cycling through `speakers`.
 *
 * The default script deliberately includes a Simplified-Chinese source line
 * (`这个 sprint …`) so tests can prove that normalisation (and the gateway's
 * Traditional-only guard) turns it into `這個 …` before it reaches a client.
 *
 * Set `failAfterFrames` to make a stream call `onError` mid-meeting, which drives
 * the vendor-failover path.
 */

const DEFAULT_SCRIPT: readonly string[] = [
  '大家好 let us begin',
  '这个 sprint 我们先 focus 在 Core Audio', // Simplified source on purpose
  'sounds good to me',
  '我同意，那就这样',
];

export interface MockAdapterConfig {
  /** Lines emitted in order, looping. */
  script?: readonly string[];
  /** Emit one segment every this many pushAudio calls (default 3). */
  framesPerSegment?: number;
  /** Speaker keys cycled across emitted segments (default S1/S2). */
  speakers?: readonly string[];
  /** If set, the stream calls `onError` after this many pushAudio calls. */
  failAfterFrames?: number;
}

export class MockAdapter implements SttAdapter {
  readonly name = 'mock';
  readonly supportsCodeSwitching = true;
  private readonly script: readonly string[];
  private readonly framesPerSegment: number;
  private readonly speakers: readonly string[];
  private readonly failAfterFrames: number | undefined;

  constructor(config: MockAdapterConfig = {}) {
    this.script = config.script ?? DEFAULT_SCRIPT;
    this.framesPerSegment = config.framesPerSegment ?? 3;
    this.speakers = config.speakers ?? ['S1', 'S2'];
    this.failAfterFrames = config.failAfterFrames;
  }

  openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream> {
    const normalize = options.normalizeChinese ?? true;
    let frames = 0;
    let emitted = 0;
    let segmentStartMs = 0;
    let closed = false;

    return Promise.resolve({
      pushAudio: (_track, _chunk, timestampMs) => {
        if (closed) return;
        if (frames === 0) segmentStartMs = timestampMs;
        frames += 1;

        if (this.failAfterFrames !== undefined && frames >= this.failAfterFrames) {
          closed = true;
          handlers.onError(new Error('mock: simulated vendor failure'));
          return;
        }

        if (frames % this.framesPerSegment === 0) {
          const raw = this.script[emitted % this.script.length]!;
          const text = normaliseText(raw, normalize);
          if (text.trim()) {
            handlers.onSegment({
              id: nextSegmentId(options.meetingId),
              meetingId: options.meetingId,
              track: 'meeting',
              speaker: speakerKey(this.speakers[emitted % this.speakers.length]!.replace(/^S/, '')),
              startMs: segmentStartMs,
              endMs: timestampMs,
              text,
              lang: detectLang(text),
              isFinal: true,
              pass: 'live',
              confidence: 0.95,
            });
          }
          emitted += 1;
          segmentStartMs = timestampMs;
        }
      },
      close: async () => {
        closed = true;
      },
    });
  }

  transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]> {
    const normalize = options.normalizeChinese ?? true;
    const segments = this.script
      .map((raw, i): TranscriptSegment | undefined => {
        const text = normaliseText(raw, normalize);
        if (!text.trim()) return undefined;
        return {
          id: nextSegmentId(options.meetingId),
          meetingId: options.meetingId,
          track: 'meeting',
          speaker: speakerKey(this.speakers[i % this.speakers.length]!.replace(/^S/, '')),
          startMs: i * 2000,
          endMs: i * 2000 + 1800,
          text,
          lang: detectLang(text),
          isFinal: true,
          pass: 'final',
          confidence: 0.95,
        };
      })
      .filter((s): s is TranscriptSegment => s !== undefined);
    return Promise.resolve(segments);
  }
}
