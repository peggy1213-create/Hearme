import type { Lang, Platform, Track, TranscriptSegment } from '@hearme/shared';

/**
 * STT adapter interface. Every vendor (Soniox, AssemblyAI, Deepgram, …) and the
 * local whisper mode implement this, so the gateway can swap vendors without
 * changing anything else. The Phase 0 benchmark (`tools/benchmark`) picks the
 * default. See docs/plan.md.
 */

export interface SttStreamOptions {
  meetingId: string;
  platform: Platform;
  /** Languages to bias toward; English + Taiwanese Mandarin by default. */
  languages: Lang[];
  /** Sample rate of the PCM the adapter will receive, in Hz. */
  sampleRate: number;
  /** Optional custom vocabulary (names, products, jargon). */
  vocabulary?: string[];
}

export interface SttBatchOptions {
  meetingId: string;
  languages: Lang[];
  /** Path to an audio file on disk (the final-pass recording). */
  audioPath: string;
  vocabulary?: string[];
}

export interface SttStreamHandlers {
  /** Called for each live segment. Chinese must be normalised via packages/zhtw. */
  onSegment: (segment: TranscriptSegment) => void;
  onError: (error: Error) => void;
}

export interface SttStream {
  /** Feed one chunk of decoded audio for a track. */
  pushAudio(track: Track, chunk: Uint8Array, timestampMs: number): void;
  /** Flush and close the stream. */
  close(): Promise<void>;
}

export interface SttAdapter {
  readonly name: string;
  /** Whether the vendor handles Mandarin–English code-switching in one utterance. */
  readonly supportsCodeSwitching: boolean;
  /** Open a live streaming session (live pass, target < 2 s delay). */
  openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream>;
  /** Transcribe a full recording (final pass). */
  transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]>;
}

export class SttNotImplementedError extends Error {
  constructor(vendor: string, feature: string) {
    super(`${vendor}: ${feature} not implemented yet`);
    this.name = 'SttNotImplementedError';
  }
}
