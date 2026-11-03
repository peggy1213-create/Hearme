import type { AddressInfo } from 'node:net';
import WebSocket from 'ws';
import { MockAdapter, type SttAdapter } from '@hearme/stt';
import {
  PROTOCOL_VERSION,
  ServerMessageSchema,
  type ClientMessage,
  type ServerAck,
  type ServerError,
  type ServerMessage,
  type Track,
  type TranscriptSegment,
} from '@hearme/shared';
import { loadConfig, type GatewayConfig } from '../config.js';
import { createDb, type GatewayDb } from '../db/client.js';
import { buildGateway, type Gateway } from '../server.js';

/**
 * Test harness: a real gateway on an ephemeral port backed by in-process PGlite
 * and the deterministic mock STT vendor. No Docker, no network, no vendor keys.
 */

export interface TestGateway {
  gateway: Gateway;
  db: GatewayDb;
  port: number;
  /** Everything the gateway logged, for privacy assertions. */
  logs: string;
  liveUrl: string;
  close: () => Promise<void>;
}

export interface StartGatewayOptions {
  config?: Partial<GatewayConfig>;
  buildLiveAdapter?: () => SttAdapter;
  buildBatchAdapter?: () => SttAdapter;
}

export async function startGateway(options: StartGatewayOptions = {}): Promise<TestGateway> {
  const db = await createDb({ url: 'memory://' });
  await db.migrate();

  let logs = '';
  const logStream = {
    write: (chunk: string): boolean => {
      logs += chunk;
      return true;
    },
  };

  const config: GatewayConfig = {
    ...loadConfig(),
    isDev: true,
    devBypassToken: 'dev-bypass',
    ...options.config,
  };
  const gateway = await buildGateway({
    db,
    config,
    logStream,
    buildLiveAdapter: options.buildLiveAdapter ?? (() => new MockAdapter({ framesPerSegment: 2 })),
    buildBatchAdapter: options.buildBatchAdapter ?? (() => new MockAdapter()),
  });

  await gateway.app.listen({ port: 0, host: '127.0.0.1' });
  const { port } = gateway.app.server.address() as AddressInfo;

  return {
    gateway,
    db,
    port,
    get logs() {
      return logs;
    },
    liveUrl: `ws://127.0.0.1:${port}/v1/live`,
    close: async () => {
      await gateway.close();
      await db.close();
    },
  };
}

/** A scripted WebSocket client for driving the gateway in tests. */
export class FakeClient {
  readonly segments: TranscriptSegment[] = [];
  readonly acks: ServerAck[] = [];
  readonly errors: ServerError[] = [];
  ended: { segmentCount: number } | undefined;
  private readonly ws: WebSocket;
  private readonly waiters: (() => void)[] = [];

  private constructor(ws: WebSocket) {
    this.ws = ws;
    ws.on('message', (raw: WebSocket.RawData) => {
      const parsed = ServerMessageSchema.safeParse(JSON.parse(raw.toString()));
      if (!parsed.success) return;
      this.record(parsed.data);
      for (const w of this.waiters.splice(0)) w();
    });
  }

  private record(msg: ServerMessage): void {
    if (msg.type === 'segment') this.segments.push(msg.segment);
    else if (msg.type === 'ack') this.acks.push(msg);
    else if (msg.type === 'error') this.errors.push(msg);
    else if (msg.type === 'ended') this.ended = { segmentCount: msg.segmentCount };
  }

  static connect(url: string, token = 'dev-bypass'): Promise<FakeClient> {
    const ws = new WebSocket(url, { headers: { Authorization: `Bearer ${token}` } });
    const client = new FakeClient(ws);
    return new Promise((resolve, reject) => {
      ws.once('open', () => resolve(client));
      ws.once('error', reject);
    });
  }

  send(msg: ClientMessage): void {
    this.ws.send(JSON.stringify(msg));
  }

  start(meetingId: string, extra: Partial<Record<string, unknown>> = {}): void {
    this.send({
      type: 'start',
      protocolVersion: PROTOCOL_VERSION,
      meetingId,
      platform: 'zoom',
      languages: ['en', 'zh-TW'],
      sampleRate: 16000,
      codec: 'pcm',
      ...extra,
    });
  }

  audio(track: Track, seq: number): void {
    this.send({ type: 'audio', track, seq, timestampMs: seq * 20, data: 'AAAA' });
  }

  stop(meetingId: string): void {
    this.send({ type: 'stop', meetingId });
  }

  /** Resolve once `predicate` holds, re-checking on every message (or on timeout). */
  waitUntil(predicate: () => boolean, timeoutMs = 2000): Promise<void> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('waitUntil timed out')), timeoutMs);
      const check = (): void => {
        if (predicate()) {
          clearTimeout(timer);
          resolve();
        } else {
          this.waiters.push(check);
        }
      };
      check();
    });
  }

  close(): void {
    this.ws.close();
  }
}
