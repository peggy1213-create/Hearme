import { readFile } from 'node:fs/promises';
import { basename } from 'node:path';
import WebSocket from 'ws';
import type { TranscriptSegment } from '@hearme/shared';
import type {
  SttAdapter,
  SttBatchOptions,
  SttStream,
  SttStreamHandlers,
  SttStreamOptions,
} from '../adapter.js';
import { detectLang, languageHints, nextSegmentId, normaliseText, speakerKey } from '../util.js';

/**
 * Soniox adapter.
 *
 * Docs consulted 2026-10-30:
 * - Real-time WebSocket API: https://soniox.com/docs/stt/api-reference/websocket-api
 * - Async transcription: https://soniox.com/docs/stt/core-concepts/async-transcription
 * - Pricing: https://soniox.com/pricing  (real-time $0.12/hr, async $0.10/hr;
 *   diarization + language identification included, updated June 2026)
 *
 * Soniox streams with per-token language identification and speaker diarization
 * and handles Mandarin–English code-switching, so it is a strong Phase 0
 * candidate. APIs change — re-check the pages above before relying on the fields
 * below. The raw `audio_format` token and the async transcript path are the two
 * fields most worth re-verifying.
 */

const RT_ENDPOINT = 'wss://stt-rt.soniox.com/transcribe-websocket';
const REST_HOST = 'https://api.soniox.com';
const RT_MODEL = 'stt-rt-v5';
const ASYNC_MODEL = 'stt-async-v5';

interface SonioxToken {
  text: string;
  start_ms?: number;
  end_ms?: number;
  confidence?: number;
  is_final?: boolean;
  speaker?: string;
  language?: string;
}

interface SonioxMessage {
  tokens?: SonioxToken[];
  finished?: boolean;
  error_code?: number;
  error_message?: string;
}

/** Group a run of tokens into one segment (used by both stream and batch). */
function tokensToSegment(
  tokens: SonioxToken[],
  meetingId: string,
  pass: 'live' | 'final',
  normalize: boolean,
): TranscriptSegment | undefined {
  const usable = tokens.filter((t) => t.text && t.text.trim() !== '');
  if (usable.length === 0) return undefined;
  const text = normaliseText(usable.map((t) => t.text).join(''), normalize);
  if (!text.trim()) return undefined;
  const first = usable[0]!;
  const last = usable[usable.length - 1]!;
  const confidences = usable.map((t) => t.confidence).filter((c): c is number => c !== undefined);
  return {
    id: nextSegmentId(meetingId),
    meetingId,
    track: 'meeting',
    speaker: speakerKey(first.speaker),
    startMs: first.start_ms ?? 0,
    endMs: last.end_ms ?? first.start_ms ?? 0,
    text,
    lang: detectLang(text),
    isFinal: pass === 'final' || (first.is_final ?? false),
    pass,
    confidence:
      confidences.length > 0
        ? confidences.reduce((a, b) => a + b, 0) / confidences.length
        : undefined,
  };
}

export class SonioxAdapter implements SttAdapter {
  readonly name = 'soniox';
  readonly supportsCodeSwitching = true;
  private readonly apiKey: string;

  constructor(config: { apiKey?: string } = {}) {
    this.apiKey = config.apiKey ?? process.env.SONIOX_API_KEY ?? '';
  }

  async openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream> {
    if (!this.apiKey) throw new Error('SONIOX_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const ws = new WebSocket(RT_ENDPOINT, {
      headers: { Authorization: `Bearer ${this.apiKey}` },
    });

    let closedResolve: () => void = () => {};
    const closed = new Promise<void>((resolve) => {
      closedResolve = resolve;
    });

    ws.on('message', (raw: WebSocket.RawData) => {
      let msg: SonioxMessage;
      try {
        msg = JSON.parse(raw.toString()) as SonioxMessage;
      } catch (err) {
        handlers.onError(err instanceof Error ? err : new Error(String(err)));
        return;
      }
      if (msg.error_code) {
        handlers.onError(new Error(`soniox ${msg.error_code}: ${msg.error_message ?? 'error'}`));
        return;
      }
      const finals = (msg.tokens ?? []).filter((t) => t.is_final);
      const segment = tokensToSegment(finals, options.meetingId, 'live', normalize);
      if (segment) handlers.onSegment(segment);
      if (msg.finished) closedResolve();
    });
    ws.on('error', (err) => handlers.onError(err));
    ws.on('close', () => closedResolve());

    await new Promise<void>((resolve, reject) => {
      ws.once('open', resolve);
      ws.once('error', reject);
    });

    ws.send(
      JSON.stringify({
        model: RT_MODEL,
        // Raw little-endian 16-bit PCM; the harness feeds this format.
        audio_format: 'pcm_s16le',
        sample_rate: options.sampleRate,
        num_channels: 1,
        language_hints: languageHints(options.languages),
        enable_speaker_diarization: true,
        enable_language_identification: true,
        ...(options.vocabulary && options.vocabulary.length > 0
          ? { context: { terms: options.vocabulary } }
          : {}),
      }),
    );

    return {
      pushAudio: (_track, chunk) => {
        if (ws.readyState === WebSocket.OPEN) ws.send(chunk);
      },
      close: async () => {
        // An empty text frame tells Soniox to flush and finish.
        if (ws.readyState === WebSocket.OPEN) ws.send('');
        await Promise.race([closed, new Promise((r) => setTimeout(r, 10_000))]);
        ws.close();
      },
    };
  }

  async transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]> {
    if (!this.apiKey) throw new Error('SONIOX_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const auth = { Authorization: `Bearer ${this.apiKey}` };

    // 1. Upload the audio file.
    const buf = await readFile(options.audioPath);
    const form = new FormData();
    form.append('file', new Blob([buf]), basename(options.audioPath));
    const upRes = await fetch(`${REST_HOST}/v1/files`, {
      method: 'POST',
      headers: auth,
      body: form,
    });
    if (!upRes.ok) throw new Error(`soniox upload failed: ${upRes.status} ${await upRes.text()}`);
    const fileId = ((await upRes.json()) as { id: string }).id;

    // 2. Create the transcription job.
    const createRes = await fetch(`${REST_HOST}/v1/transcriptions`, {
      method: 'POST',
      headers: { ...auth, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        file_id: fileId,
        model: ASYNC_MODEL,
        language_hints: languageHints(options.languages),
        enable_speaker_diarization: true,
        enable_language_identification: true,
        ...(options.vocabulary && options.vocabulary.length > 0
          ? { context: { terms: options.vocabulary } }
          : {}),
      }),
    });
    if (!createRes.ok)
      throw new Error(`soniox transcription failed: ${createRes.status} ${await createRes.text()}`);
    const jobId = ((await createRes.json()) as { id: string }).id;

    // 3. Poll until done.
    for (;;) {
      await new Promise((r) => setTimeout(r, 2000));
      const stRes = await fetch(`${REST_HOST}/v1/transcriptions/${jobId}`, { headers: auth });
      if (!stRes.ok) throw new Error(`soniox status failed: ${stRes.status}`);
      const status = ((await stRes.json()) as { status: string }).status;
      if (status === 'completed') break;
      if (status === 'error') throw new Error(`soniox transcription errored (job ${jobId})`);
    }

    // 4. Fetch the transcript tokens and group them into segments by speaker.
    const trRes = await fetch(`${REST_HOST}/v1/transcriptions/${jobId}/transcript`, {
      headers: auth,
    });
    if (!trRes.ok) throw new Error(`soniox transcript failed: ${trRes.status}`);
    const tokens = ((await trRes.json()) as SonioxMessage).tokens ?? [];

    const segments: TranscriptSegment[] = [];
    let run: SonioxToken[] = [];
    const flush = (): void => {
      const seg = tokensToSegment(run, options.meetingId, 'final', normalize);
      if (seg) segments.push(seg);
      run = [];
    };
    for (const tok of tokens) {
      const prev = run[run.length - 1];
      if (prev && prev.speaker !== tok.speaker) flush();
      run.push(tok);
    }
    flush();
    return segments;
  }
}
