import WebSocket from 'ws';
import {
  PROTOCOL_VERSION,
  ServerMessageSchema,
  type ClientMessage,
  type Track,
} from '@hearme/shared';
import { readWav, type WavAudio } from './wav.js';

/**
 * Replay tool: stream two WAV files (meeting + mic) through the gateway at
 * real-time speed as a fake client, and print the live transcript. No ffmpeg; the
 * audio is sent as raw PCM (`codec: "pcm"`), so no Opus encoder is needed either.
 *
 *   pnpm replay --wav meeting.wav --mic mic.wav [--url ws://host/v1/live]
 *               [--token <access>] [--meetingId <id>] [--speed <n>]
 */

const FRAME_MS = 20;

interface Args {
  wav: string;
  mic?: string;
  url: string;
  token: string;
  meetingId: string;
  speed: number;
}

function parseArgs(argv: string[]): Args {
  const out: Record<string, string> = {};
  for (let i = 0; i < argv.length; i += 2) {
    const key = argv[i];
    if (key?.startsWith('--')) out[key.slice(2)] = argv[i + 1] ?? '';
  }
  if (!out.wav) {
    throw new Error('usage: replay --wav meeting.wav [--mic mic.wav] [--url ...] [--token ...]');
  }
  return {
    wav: out.wav,
    mic: out.mic,
    url: out.url ?? 'ws://localhost:8787/v1/live',
    token: out.token ?? 'dev-bypass',
    meetingId: out.meetingId ?? `replay-${Date.now()}`,
    speed: Number(out.speed ?? 1),
  };
}

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

/** Slice a mono Int16 stream into 20 ms base64 PCM frames. */
function frames(audio: WavAudio): string[] {
  const per = Math.round((audio.sampleRate * FRAME_MS) / 1000);
  const out: string[] = [];
  for (let start = 0; start < audio.samples.length; start += per) {
    const slice = audio.samples.subarray(start, Math.min(start + per, audio.samples.length));
    out.push(Buffer.from(slice.buffer, slice.byteOffset, slice.byteLength).toString('base64'));
  }
  return out;
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const meeting = await readWav(args.wav);
  const mic = args.mic ? await readWav(args.mic) : undefined;
  if (mic && mic.sampleRate !== meeting.sampleRate) {
    throw new Error('mic and meeting WAVs must share a sample rate');
  }

  const meetingFrames = frames(meeting);
  const micFrames = mic ? frames(mic) : [];
  const total = Math.max(meetingFrames.length, micFrames.length);

  const ws = new WebSocket(args.url, { headers: { Authorization: `Bearer ${args.token}` } });
  const send = (msg: ClientMessage): void => ws.send(JSON.stringify(msg));

  let segments = 0;
  const done = new Promise<void>((resolve, reject) => {
    ws.on('message', (raw: WebSocket.RawData) => {
      const parsed = ServerMessageSchema.safeParse(JSON.parse(raw.toString()));
      if (!parsed.success) return;
      const msg = parsed.data;
      if (msg.type === 'segment') {
        segments += 1;
        const s = msg.segment;
        process.stdout.write(`[${s.speaker}] ${s.text}\n`);
      } else if (msg.type === 'error') {
        process.stderr.write(`error ${msg.code}: ${msg.message}\n`);
        if (msg.fatal) reject(new Error(msg.message));
      } else if (msg.type === 'ended') {
        resolve();
      }
    });
    ws.on('error', reject);
  });

  await new Promise<void>((resolve, reject) => {
    ws.once('open', resolve);
    ws.once('error', reject);
  });

  send({
    type: 'start',
    protocolVersion: PROTOCOL_VERSION,
    meetingId: args.meetingId,
    platform: 'other',
    languages: ['en', 'zh-TW'],
    sampleRate: meeting.sampleRate,
    codec: 'pcm',
  });

  const pushFrame = (track: Track, data: string, seq: number): void =>
    send({ type: 'audio', track, seq, timestampMs: seq * FRAME_MS, data });

  for (let i = 0; i < total; i++) {
    if (i < micFrames.length) pushFrame('mic', micFrames[i]!, i);
    if (i < meetingFrames.length) pushFrame('meeting', meetingFrames[i]!, i);
    await sleep(FRAME_MS / args.speed);
  }

  send({ type: 'stop', meetingId: args.meetingId });
  await done;
  ws.close();
  process.stdout.write(`\n${segments} live segments\n`);
}

main().catch((err: unknown) => {
  process.stderr.write(`${err instanceof Error ? err.message : String(err)}\n`);
  process.exit(1);
});
