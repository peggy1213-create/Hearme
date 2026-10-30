import { containsSimplified } from '@hearme/zhtw';
import type { Lang, Platform } from '@hearme/shared';
import type { SttAdapter, SttVendor } from '@hearme/stt';
import { SttNotImplementedError } from '@hearme/stt';
import { decodePcm16, type DecodedAudio } from './audio.js';
import type { CorpusSample } from './corpus.js';

/**
 * Drives one vendor adapter over one corpus sample, in both passes:
 * - batch (final pass): hand the file to the adapter;
 * - streaming (live pass): feed decoded PCM at 1× speed in 100 ms frames and
 *   time how long each finalised segment takes to come back.
 *
 * Every pass runs with `normalizeChinese: false` so we can see the vendor's raw
 * output and flag Simplified characters before the harness converts for scoring.
 * Each pass is wrapped so one vendor failing (missing key, network, no ffmpeg)
 * records an error instead of aborting the whole benchmark.
 */

const FRAME_MS = 100;

export interface PassResult {
  ok: boolean;
  error?: string;
  /** Raw (unconverted) hypothesis text. */
  hypothesis: string;
  rawContainedSimplified: boolean;
}

export interface StreamPassResult extends PassResult {
  /** Latency (ms) from a segment's audio end time to when it arrived. */
  latenciesMs: number[];
}

export interface SampleRun {
  vendor: SttVendor;
  sample: string;
  reference: string | null;
  durationSec: number;
  batch: PassResult;
  stream: StreamPassResult;
}

export interface RunOptions {
  sampleRate: number;
  languages: Lang[];
  platform: Platform;
  /** Skip the streaming pass (e.g. whisper, or when ffmpeg is unavailable). */
  skipStream?: boolean;
  streamSkipReason?: string;
}

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

async function runBatch(
  adapter: SttAdapter,
  sample: CorpusSample,
  opts: RunOptions,
): Promise<PassResult> {
  try {
    const segments = await adapter.transcribeBatch({
      meetingId: `${adapter.name}-${sample.name}`,
      languages: opts.languages,
      audioPath: sample.audioPath,
      normalizeChinese: false,
    });
    const hypothesis = segments
      .map((s) => s.text)
      .join(' ')
      .trim();
    return { ok: true, hypothesis, rawContainedSimplified: containsSimplified(hypothesis) };
  } catch (err) {
    return {
      ok: false,
      error: err instanceof Error ? err.message : String(err),
      hypothesis: '',
      rawContainedSimplified: false,
    };
  }
}

async function runStream(
  adapter: SttAdapter,
  sample: CorpusSample,
  decoded: DecodedAudio,
  opts: RunOptions,
): Promise<StreamPassResult> {
  const empty: StreamPassResult = {
    ok: false,
    hypothesis: '',
    rawContainedSimplified: false,
    latenciesMs: [],
  };
  try {
    const received: { text: string; endMs: number; arrivalMs: number }[] = [];
    const errors: string[] = [];
    let start = 0;

    const stream = await adapter.openStream(
      {
        meetingId: `${adapter.name}-${sample.name}`,
        platform: opts.platform,
        languages: opts.languages,
        sampleRate: opts.sampleRate,
        normalizeChinese: false,
      },
      {
        onSegment: (seg) => {
          received.push({ text: seg.text, endMs: seg.endMs, arrivalMs: performance.now() - start });
        },
        onError: (e) => errors.push(e.message),
      },
    );

    const frameBytes = Math.round(opts.sampleRate * 2 * (FRAME_MS / 1000));
    start = performance.now();
    for (let off = 0; off < decoded.pcm.length; off += frameBytes) {
      const frame = decoded.pcm.subarray(off, Math.min(off + frameBytes, decoded.pcm.length));
      const targetElapsed = (off / (2 * opts.sampleRate)) * 1000;
      const wait = targetElapsed - (performance.now() - start);
      if (wait > 0) await sleep(wait);
      stream.pushAudio('meeting', new Uint8Array(frame), targetElapsed);
    }
    await stream.close();

    const hypothesis = received
      .map((r) => r.text)
      .join(' ')
      .trim();
    const latenciesMs = received.map((r) => Math.max(0, r.arrivalMs - r.endMs));
    return {
      ok: errors.length === 0,
      error: errors.length > 0 ? errors.join('; ') : undefined,
      hypothesis,
      rawContainedSimplified: containsSimplified(hypothesis),
      latenciesMs,
    };
  } catch (err) {
    if (err instanceof SttNotImplementedError) {
      return { ...empty, error: 'streaming not supported by this vendor' };
    }
    return { ...empty, error: err instanceof Error ? err.message : String(err) };
  }
}

const skippedStream = (reason: string): StreamPassResult => ({
  ok: false,
  hypothesis: '',
  rawContainedSimplified: false,
  latenciesMs: [],
  error: reason,
});

/** Run both passes for one vendor over one sample. */
export async function runSample(
  vendor: SttVendor,
  adapter: SttAdapter,
  sample: CorpusSample,
  opts: RunOptions,
): Promise<SampleRun> {
  const batch = await runBatch(adapter, sample, opts);

  // Decode once (for duration, and for the streaming pass). ffmpeg may be absent.
  let decoded: DecodedAudio | undefined;
  let decodeError: string | undefined;
  try {
    decoded = await decodePcm16(sample.audioPath, opts.sampleRate);
  } catch (err) {
    decodeError = err instanceof Error ? err.message : String(err);
  }

  let stream: StreamPassResult;
  if (opts.skipStream) {
    stream = skippedStream(opts.streamSkipReason ?? 'streaming not supported by this vendor');
  } else if (!decoded) {
    stream = skippedStream(decodeError ?? 'audio decode unavailable');
  } else {
    stream = await runStream(adapter, sample, decoded, opts);
  }

  return {
    vendor,
    sample: sample.name,
    reference: sample.reference,
    durationSec: decoded?.durationSec ?? 0,
    batch,
    stream,
  };
}
