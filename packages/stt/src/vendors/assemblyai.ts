import { readFile } from 'node:fs/promises';
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
 * AssemblyAI adapter.
 *
 * Docs consulted 2026-10-30:
 * - Streaming (v3) + diarization: https://assemblyai.com/docs/streaming/streaming-diarization
 * - Async transcript submit: https://www.assemblyai.com/docs/api-reference/transcripts/submit
 * - Pricing: https://www.assemblyai.com/pricing  (streaming $0.15/hr; async
 *   Universal-3.5 Pro $0.21/hr + diarization $0.02/hr; streaming diarization
 *   +$0.12/hr; updated 2026-10-27)
 *
 * The multilingual streaming model handles Mandarin–English code-switching. APIs
 * change — re-check the pages above. Streaming is billed on WebSocket session
 * time, so the harness opens the socket as late as possible and closes promptly.
 */

const STREAM_ENDPOINT = 'wss://streaming.assemblyai.com/v3/ws';
const REST_HOST = 'https://api.assemblyai.com';
const STREAM_MODEL = 'universal-streaming-multilingual';
const ASYNC_MODEL = 'universal-3-5-pro';

interface AaiWord {
  text: string;
  start?: number;
  end?: number;
  confidence?: number;
  speaker?: string;
}

interface AaiTurn {
  type?: string;
  transcript?: string;
  words?: AaiWord[];
  speaker_label?: string;
  end_of_turn?: boolean;
  turn_is_formatted?: boolean;
}

export class AssemblyAIAdapter implements SttAdapter {
  readonly name = 'assemblyai';
  readonly supportsCodeSwitching = true;
  private readonly apiKey: string;

  constructor(config: { apiKey?: string } = {}) {
    this.apiKey = config.apiKey ?? process.env.ASSEMBLYAI_API_KEY ?? '';
  }

  async openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream> {
    if (!this.apiKey) throw new Error('ASSEMBLYAI_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const url =
      `${STREAM_ENDPOINT}?sample_rate=${options.sampleRate}` +
      `&speech_model=${STREAM_MODEL}&speaker_labels=true&format_turns=true`;
    const ws = new WebSocket(url, { headers: { Authorization: this.apiKey } });

    // Emit each turn once, when it reaches end_of_turn.
    const emittedTurns = new Set<string>();

    ws.on('message', (raw: WebSocket.RawData) => {
      let msg: AaiTurn;
      try {
        msg = JSON.parse(raw.toString()) as AaiTurn;
      } catch (err) {
        handlers.onError(err instanceof Error ? err : new Error(String(err)));
        return;
      }
      if (msg.type !== 'Turn' || !msg.end_of_turn) return;
      const transcript = (msg.transcript ?? '').trim();
      if (!transcript) return;
      const words = msg.words ?? [];
      const key = `${msg.speaker_label ?? ''}:${words[0]?.start ?? ''}:${transcript}`;
      if (emittedTurns.has(key)) return;
      emittedTurns.add(key);
      const text = normaliseText(transcript, normalize);
      handlers.onSegment({
        id: nextSegmentId(options.meetingId),
        meetingId: options.meetingId,
        track: 'meeting',
        speaker: speakerKey(msg.speaker_label),
        startMs: words[0]?.start ?? 0,
        endMs: words[words.length - 1]?.end ?? words[0]?.start ?? 0,
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
        if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: 'Terminate' }));
        await new Promise((r) => setTimeout(r, 500));
        ws.close();
      },
    };
  }

  async transcribeBatch(options: SttBatchOptions): Promise<TranscriptSegment[]> {
    if (!this.apiKey) throw new Error('ASSEMBLYAI_API_KEY is not set');
    const normalize = options.normalizeChinese ?? true;
    const auth = { authorization: this.apiKey };

    // 1. Upload the raw audio bytes.
    const buf = await readFile(options.audioPath);
    const upRes = await fetch(`${REST_HOST}/v2/upload`, {
      method: 'POST',
      headers: { ...auth, 'Content-Type': 'application/octet-stream' },
      body: buf,
    });
    if (!upRes.ok)
      throw new Error(`assemblyai upload failed: ${upRes.status} ${await upRes.text()}`);
    const uploadUrl = ((await upRes.json()) as { upload_url: string }).upload_url;

    // 2. Create the transcription job (diarization + language detection on).
    const createRes = await fetch(`${REST_HOST}/v2/transcript`, {
      method: 'POST',
      headers: { ...auth, 'content-type': 'application/json' },
      body: JSON.stringify({
        audio_url: uploadUrl,
        speech_models: [ASYNC_MODEL],
        speaker_labels: true,
        language_detection: true,
      }),
    });
    if (!createRes.ok)
      throw new Error(
        `assemblyai transcript failed: ${createRes.status} ${await createRes.text()}`,
      );
    const jobId = ((await createRes.json()) as { id: string }).id;

    // 3. Poll until completed.
    let result: { status: string; utterances?: AaiUtterance[]; words?: AaiWord[]; text?: string };
    for (;;) {
      await new Promise((r) => setTimeout(r, 3000));
      const res = await fetch(`${REST_HOST}/v2/transcript/${jobId}`, { headers: auth });
      if (!res.ok) throw new Error(`assemblyai status failed: ${res.status}`);
      result = (await res.json()) as typeof result;
      if (result.status === 'completed') break;
      if (result.status === 'error') throw new Error(`assemblyai transcription errored (${jobId})`);
    }

    // 4. Prefer utterances (one per speaker turn); fall back to the whole text.
    const utterances = result.utterances ?? [];
    if (utterances.length > 0) {
      return utterances
        .map((u): TranscriptSegment | undefined => {
          const text = normaliseText((u.text ?? '').trim(), normalize);
          if (!text) return undefined;
          return {
            id: nextSegmentId(options.meetingId),
            meetingId: options.meetingId,
            track: 'meeting',
            speaker: speakerKey(u.speaker),
            startMs: u.start ?? 0,
            endMs: u.end ?? u.start ?? 0,
            text,
            lang: detectLang(text),
            isFinal: true,
            pass: 'final',
            confidence: u.confidence,
          };
        })
        .filter((s): s is TranscriptSegment => s !== undefined);
    }

    const text = normaliseText((result.text ?? '').trim(), normalize);
    if (!text) return [];
    const words = result.words ?? [];
    return [
      {
        id: nextSegmentId(options.meetingId),
        meetingId: options.meetingId,
        track: 'meeting',
        speaker: 'S1',
        startMs: words[0]?.start ?? 0,
        endMs: words[words.length - 1]?.end ?? 0,
        text,
        lang: detectLang(text),
        isFinal: true,
        pass: 'final',
      },
    ];
  }
}

interface AaiUtterance {
  speaker?: string;
  text?: string;
  start?: number;
  end?: number;
  confidence?: number;
}
