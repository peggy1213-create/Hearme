import { readFile } from 'node:fs/promises';
import { extname } from 'node:path';
import WebSocket from 'ws';
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
 * Deepgram adapter (Nova-3).
 *
 * Docs consulted 2026-10-30:
 * - Pre-recorded: https://developers.deepgram.com/docs/pre-recorded-audio
 * - Streaming WebSocket: https://developers.deepgram.com/reference/speech-to-text-api/listen-streaming
 * - Pricing: https://deepgram.com/pricing  (streaming multilingual $0.0092/min +
 *   diarization $0.0020/min; pre-recorded multilingual $0.0052/min, diarization
 *   included — pay-as-you-go)
 *
 * `language=multi` turns on Nova-3 multilingual recognition, which handles
 * Mandarin–English code-switching. The `diarize` flag is marked deprecated in
 * favour of `diarize_model`; it still returns a `speaker` on each word, so we use
 * it, but re-check before launch. APIs change — re-check the pages above.
 */

const STREAM_ENDPOINT = 'wss://api.deepgram.com/v1/listen';
const REST_ENDPOINT = 'https://api.deepgram.com/v1/listen';
const MODEL = 'nova-3';

interface DgWord {
  word: string;
  punctuated_word?: string;
  start?: number;
  end?: number;
  confidence?: number;
  speaker?: number;
  language?: string;
}

interface DgResults {
  is_final?: boolean;
  channel?: { alternatives?: { transcript?: string; words?: DgWord[] }[] };
}

interface DgBatchResponse {
  results?: { channels?: { alternatives?: { transcript?: string; words?: DgWord[] }[] }[] };
}

const CONTENT_TYPES: Record<string, string> = {
  '.wav': 'audio/wav',
  '.mp3': 'audio/mpeg',
  '.m4a': 'audio/mp4',
  '.flac': 'audio/flac',
  '.ogg': 'audio/ogg',
  '.opus': 'audio/opus',
  '.webm': 'audio/webm',
};

/** Seconds (Deepgram) → integer milliseconds. */
const toMs = (s: number | undefined): number => Math.round((s ?? 0) * 1000);

export class DeepgramAdapter implements SttAdapter {
  readonly name = 'deepgram';
  // Nova-3 with language=multi handles code-switching.
  readonly supportsCodeSwitching = true;
  private readonly apiKey: string;

  constructor(config: { apiKey?: string } = {}) {
    this.apiKey = config.apiKey ?? process.env.DEEPGRAM_API_KEY ?? '';
  }

  async openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream> {
    if (!this.apiKey) throw new Error('DEEPGRAM_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const url =
      `${STREAM_ENDPOINT}?model=${MODEL}&language=multi&diarize=true` +
      `&punctuate=true&smart_format=true&interim_results=true` +
      `&encoding=linear16&sample_rate=${options.sampleRate}&channels=1`;
    const ws = new WebSocket(url, { headers: { Authorization: `Token ${this.apiKey}` } });

    ws.on('message', (raw: WebSocket.RawData) => {
      let msg: DgResults;
      try {
        msg = JSON.parse(raw.toString()) as DgResults;
      } catch (err) {
        handlers.onError(err instanceof Error ? err : new Error(String(err)));
        return;
      }
      if (!msg.is_final) return;
      const alt = msg.channel?.alternatives?.[0];
      const transcript = (alt?.transcript ?? '').trim();
      if (!transcript) return;
      const words = alt?.words ?? [];
      const text = normaliseText(transcript, normalize);
      handlers.onSegment({
        id: nextSegmentId(options.meetingId),
        meetingId: options.meetingId,
        track: 'meeting',
        speaker: speakerKey(words[0]?.speaker),
        startMs: toMs(words[0]?.start),
        endMs: toMs(words[words.length - 1]?.end),
        text,
        lang: detectLang(text),
        isFinal: true,
        pass: 'live',
      });
    });
    ws.on('error', (err) => handlers.onError(err));

    await new Promise<void>((resolve, reject) => {
      ws.once('open', resolve);
      ws.once('error', reject);
    });

    return {
      pushAudio: (_track, chunk) => {
        if (ws.readyState === WebSocket.OPEN) ws.send(chunk);
      },
      close: async () => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: 'Finalize' }));
          ws.send(JSON.stringify({ type: 'CloseStream' }));
        }
        await new Promise((r) => setTimeout(r, 500));
        ws.close();
      },
    };
  }

  async transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]> {
    if (!this.apiKey) throw new Error('DEEPGRAM_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const query = `?model=${MODEL}&language=multi&diarize=true&punctuate=true&smart_format=true&utterances=true`;
    const buf = await readFile(options.audioPath);
    const contentType = CONTENT_TYPES[extname(options.audioPath).toLowerCase()] ?? 'audio/wav';

    const res = await fetch(`${REST_ENDPOINT}${query}`, {
      method: 'POST',
      headers: { Authorization: `Token ${this.apiKey}`, 'Content-Type': contentType },
      body: buf,
    });
    if (!res.ok)
      throw new Error(`deepgram transcription failed: ${res.status} ${await res.text()}`);
    const body = (await res.json()) as DgBatchResponse;
    const alt = body.results?.channels?.[0]?.alternatives?.[0];
    const words = alt?.words ?? [];
    if (words.length === 0) {
      const text = normaliseText((alt?.transcript ?? '').trim(), normalize);
      return text
        ? [
            {
              id: nextSegmentId(options.meetingId),
              meetingId: options.meetingId,
              track: 'meeting',
              speaker: 'S1',
              startMs: 0,
              endMs: 0,
              text,
              lang: detectLang(text),
              isFinal: true,
              pass: 'final',
            },
          ]
        : [];
    }

    // Group consecutive words into segments by speaker.
    const segments: TranscriptSegment[] = [];
    let run: DgWord[] = [];
    const flush = (): void => {
      if (run.length === 0) return;
      const text = normaliseText(run.map((w) => w.punctuated_word ?? w.word).join(' '), normalize);
      if (text.trim()) {
        const confidences = run
          .map((w) => w.confidence)
          .filter((c): c is number => c !== undefined);
        segments.push({
          id: nextSegmentId(options.meetingId),
          meetingId: options.meetingId,
          track: 'meeting',
          speaker: speakerKey(run[0]!.speaker),
          startMs: toMs(run[0]!.start),
          endMs: toMs(run[run.length - 1]!.end),
          text,
          lang: detectLang(text),
          isFinal: true,
          pass: 'final',
          confidence:
            confidences.length > 0
              ? confidences.reduce((a, b) => a + b, 0) / confidences.length
              : undefined,
        });
      }
      run = [];
    };
    for (const w of words) {
      const prev = run[run.length - 1];
      if (prev && prev.speaker !== w.speaker) flush();
      run.push(w);
    }
    flush();
    return segments;
  }
}
