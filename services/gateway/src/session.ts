import type {
  ClientAudio,
  ClientStart,
  ServerMessage,
  Track,
  TranscriptSegment,
} from '@hearme/shared';
import type { SttAdapter, SttStream } from '@hearme/stt';
import type { GatewayContext } from './context.js';
import { addUsage } from './db/store.js';
import { createDecoder, type AudioDecoder } from './audio/decode.js';
import { ensureTraditional } from './zhtwGuard.js';
import { gatewayMetrics } from './otel.js';

const TRACKS: readonly Track[] = ['mic', 'meeting'];
const ACK_EVERY_FRAMES = 25;

export interface LiveSessionDeps {
  ctx: GatewayContext;
  userId: string;
  start: ClientStart;
  emit: (msg: ServerMessage) => void;
}

/**
 * One live meeting. Holds a vendor stream per track (mic → speaker `me`, meeting →
 * stable `S1,S2…`), decodes incoming audio, dedupes replayed frames for resume,
 * runs every segment through the Traditional-Chinese guard, and records usage on
 * stop. Nothing is written to disk; audio and text live only here, in memory.
 */
export class LiveSession {
  readonly meetingId: string;
  readonly userId: string;
  /** Highest sequence number consumed per track; the resume cursor. */
  private readonly lastSeq: Record<Track, number> = { mic: -1, meeting: -1 };
  private readonly decoders: Record<Track, AudioDecoder>;
  private readonly streams: Partial<Record<Track, SttStream>> = {};
  private readonly adapter: SttAdapter;
  /** Vendor speaker id → stable `S1,S2…`, so labels survive vendor failover. */
  private readonly speakerMap = new Map<string, string>();
  private speakerCount = 0;
  private segmentCount = 0;
  private maxTimestampMs = 0;
  private readonly lastPushWallMs: Record<Track, number> = { mic: 0, meeting: 0 };
  private framesSinceAck: Record<Track, number> = { mic: 0, meeting: 0 };
  private emit: (msg: ServerMessage) => void;
  private ctx: GatewayContext;
  private start: ClientStart;
  private ended = false;

  constructor(deps: LiveSessionDeps) {
    this.ctx = deps.ctx;
    this.userId = deps.userId;
    this.start = deps.start;
    this.emit = deps.emit;
    this.meetingId = deps.start.meetingId;
    this.adapter = deps.ctx.buildLiveAdapter();
    this.decoders = {
      mic: createDecoder(deps.start.codec, deps.start.sampleRate),
      meeting: createDecoder(deps.start.codec, deps.start.sampleRate),
    };
  }

  /** Open a vendor stream per track. Throws if both primary and fallback fail. */
  async open(): Promise<void> {
    const options = {
      meetingId: this.meetingId,
      platform: this.start.platform,
      languages: this.start.languages,
      sampleRate: this.start.sampleRate,
      vocabulary: this.start.vocabulary,
      normalizeChinese: true as const,
    };
    for (const track of TRACKS) {
      this.streams[track] = await this.adapter.openStream(options, {
        onSegment: (seg) => this.onVendorSegment(track, seg),
        onError: (err) => this.onStreamError(err),
      });
    }
    gatewayMetrics.activeMeetings.add(1);
    this.sendAcks();
  }

  /** Point the session at a new socket after a reconnect, and re-ack both tracks. */
  rebind(emit: (msg: ServerMessage) => void): void {
    this.emit = emit;
    this.sendAcks();
  }

  /** Emit the current resume cursor for both tracks. */
  sendAcks(): void {
    for (const track of TRACKS) this.emit({ type: 'ack', track, lastSeq: this.lastSeq[track] });
  }

  /** Feed one audio frame. Frames already seen (resume replay) are ignored. */
  pushAudio(frame: ClientAudio): void {
    if (this.ended) return;
    const { track, seq, timestampMs } = frame;
    if (seq <= this.lastSeq[track]) return; // duplicate from a resume; drop it
    this.lastSeq[track] = seq;

    const encoded = Buffer.from(frame.data, 'base64');
    const pcm = this.decoders[track].decode(new Uint8Array(encoded));
    this.lastPushWallMs[track] = Date.now();
    if (timestampMs > this.maxTimestampMs) this.maxTimestampMs = timestampMs;
    this.streams[track]?.pushAudio(track, pcm, timestampMs);

    this.framesSinceAck[track] += 1;
    if (this.framesSinceAck[track] >= ACK_EVERY_FRAMES) {
      this.framesSinceAck[track] = 0;
      this.emit({ type: 'ack', track, lastSeq: seq });
    }
  }

  private onVendorSegment(track: Track, raw: TranscriptSegment): void {
    if (this.ended) return;
    const speaker = track === 'mic' ? 'me' : this.stableSpeaker(raw.speaker);
    const segment = ensureTraditional({
      ...raw,
      meetingId: this.meetingId,
      track,
      speaker,
      pass: 'live',
    });
    const startedAt = this.lastPushWallMs[track];
    if (startedAt > 0) gatewayMetrics.segmentLatency.record(Date.now() - startedAt);
    this.segmentCount += 1;
    this.emit({ type: 'segment', segment });
  }

  private stableSpeaker(vendorKey: string): string {
    let mapped = this.speakerMap.get(vendorKey);
    if (!mapped) {
      this.speakerCount += 1;
      mapped = `S${this.speakerCount}`;
      this.speakerMap.set(vendorKey, mapped);
    }
    return mapped;
  }

  private onStreamError(err: Error): void {
    // Reaches here only when the fallback vendor also failed.
    gatewayMetrics.vendorErrors.add(1, { stage: 'fatal' });
    this.emit({
      type: 'error',
      code: 'stt_unavailable',
      message: `transcription unavailable: ${err.name}`,
      fatal: true,
    });
    void this.finalize(true);
  }

  /** Flush, record usage, emit `ended`, and return the live-segment count. */
  async stop(): Promise<number> {
    const count = this.segmentCount;
    await this.finalize(true);
    this.emit({ type: 'ended', meetingId: this.meetingId, segmentCount: count });
    return count;
  }

  /** Tear down without emitting `ended` (resume window expired, socket gone). */
  async abort(): Promise<void> {
    await this.finalize(true);
  }

  private async finalize(recordUsage: boolean): Promise<void> {
    if (this.ended) return;
    this.ended = true;
    for (const track of TRACKS) {
      try {
        await this.streams[track]?.close();
      } catch {
        // closing a failed stream can throw; ignore — the session is ending.
      }
      this.decoders[track].close();
    }
    gatewayMetrics.activeMeetings.add(-1);
    if (recordUsage && this.maxTimestampMs > 0) {
      const liveMinutes = this.maxTimestampMs / 60_000;
      try {
        await addUsage(this.ctx.db, this.userId, { liveMinutes });
      } catch {
        // Usage accounting must never take down a meeting.
      }
    }
  }

  get isEnded(): boolean {
    return this.ended;
  }
}
