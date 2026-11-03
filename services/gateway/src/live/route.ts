import type { FastifyInstance, FastifyRequest } from 'fastify';
import WebSocket from 'ws';
import {
  ClientMessageSchema,
  type ClientStart,
  type ServerErrorCode,
  type ServerMessage,
} from '@hearme/shared';
import type { GatewayContext } from '../context.js';
import { resolveUserId } from '../auth/authenticate.js';
import { checkQuota, getUser } from '../db/store.js';
import { gatewayMetrics } from '../otel.js';
import { LiveSession } from '../session.js';
import type { SessionRegistry } from './registry.js';

let connCounter = 0;

/**
 * The `GET /v1/live` WebSocket route: one socket per meeting. It authenticates the
 * `start`, enforces the monthly quota, relays audio to the live session, and
 * supports resume — a reconnect with the same `meetingId` rebinds the running
 * session instead of starting a new one. See PROTOCOL.md.
 */
export function registerLiveRoute(
  app: FastifyInstance,
  ctx: GatewayContext,
  registry: SessionRegistry,
): void {
  app.get('/v1/live', { websocket: true }, (socket: WebSocket, req: FastifyRequest) => {
    const connId = ++connCounter;
    let session: LiveSession | undefined;

    const emit = (msg: ServerMessage): void => {
      if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(msg));
    };
    // Close only after the queued frame has been written, so the client receives it.
    const closeWhenFlushed = (): void => setImmediate(() => socket.close());
    const fail = (code: ServerErrorCode, message: string, fatal = true): void => {
      if (socket.readyState !== WebSocket.OPEN) return;
      socket.send(JSON.stringify({ type: 'error', code, message, fatal }), () => {
        if (fatal) socket.close();
      });
    };

    const onStart = async (start: ClientStart): Promise<void> => {
      let userId: string;
      try {
        const token = (req.query as { token?: string }).token;
        userId = await resolveUserId(ctx, req.headers.authorization, token);
      } catch {
        fail('unauthorized', 'authentication required');
        return;
      }

      // Resume: a running session for this meeting takes over this socket.
      const existing = registry.get(start.meetingId);
      if (existing && !existing.isEnded) {
        session = existing;
        registry.bind(start.meetingId, existing, connId);
        existing.rebind(emit);
        return;
      }

      const user = await getUser(ctx.db, userId);
      if (!user) {
        fail('unauthorized', 'unknown user');
        return;
      }
      const quota = await checkQuota(ctx.db, user);
      if (!quota.allowed) {
        fail('quota_exceeded', `monthly quota of ${quota.quotaMinutes} minutes reached`);
        return;
      }

      const newSession = new LiveSession({ ctx, userId, start, emit });
      try {
        await newSession.open();
      } catch {
        gatewayMetrics.vendorErrors.add(1, { stage: 'open' });
        fail('stt_unavailable', 'could not open transcription stream');
        return;
      }
      session = newSession;
      registry.bind(start.meetingId, newSession, connId);
    };

    const handleMessage = async (raw: WebSocket.RawData): Promise<void> => {
      let parsed;
      try {
        parsed = ClientMessageSchema.parse(JSON.parse(raw.toString()));
      } catch {
        emit({ type: 'error', code: 'bad_request', message: 'malformed message', fatal: false });
        return;
      }

      if (parsed.type === 'start') {
        await onStart(parsed);
        return;
      }
      if (!session) {
        emit({ type: 'error', code: 'bad_request', message: 'no active session', fatal: false });
        return;
      }
      if (parsed.type === 'audio') {
        session.pushAudio(parsed);
        return;
      }
      // stop
      await session.stop();
      registry.remove(parsed.meetingId);
      closeWhenFlushed();
    };

    socket.on('message', (raw: WebSocket.RawData) => void handleMessage(raw));
    socket.on('close', () => {
      if (session && !session.isEnded && registry.isActiveConn(session.meetingId, connId)) {
        registry.scheduleAbort(session.meetingId, connId, ctx.config.resumeWindowMs);
      }
    });
    socket.on('error', () => {
      // A transport error is followed by `close`; resume handling happens there.
    });
  });
}
