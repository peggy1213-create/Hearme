import Fastify, { type FastifyInstance } from 'fastify';
import websocket from '@fastify/websocket';
import { createSttAdapter, type SttAdapter } from '@hearme/stt';
import { PROTOCOL_VERSION } from '@hearme/shared';
import { loadConfig, type GatewayConfig } from './config.js';
import { createDb, type GatewayDb } from './db/client.js';
import { tokenConfigFromEnv } from './auth/tokens.js';
import { defaultLiveAdapter, type GatewayContext } from './context.js';
import { registerAuthRoutes } from './auth/routes.js';
import { registerBatchRoute } from './batch/route.js';
import { registerLiveRoute } from './live/route.js';
import { SessionRegistry } from './live/registry.js';

/**
 * Build the Hearme gateway.
 *
 * The gateway relays audio to the STT vendor and text to the LLM, and holds only
 * accounts and usage. It stores NO meeting content — audio and transcripts live in
 * memory only while being relayed. Vendor API keys live here, never in the desktop
 * app. See CLAUDE.md.
 */
export interface GatewayOptions {
  config?: GatewayConfig;
  /** Pre-opened database (tests inject PGlite). When omitted, one is created and owned. */
  db?: GatewayDb;
  /** Override the live-pass adapter factory (tests inject the mock vendor). */
  buildLiveAdapter?: () => SttAdapter;
  /** Override the final-pass adapter factory. */
  buildBatchAdapter?: () => SttAdapter;
  /** Capture log output (tests assert no transcript text is logged). */
  logStream?: { write: (chunk: string) => boolean };
}

export interface Gateway {
  app: FastifyInstance;
  context: GatewayContext;
  registry: SessionRegistry;
  /** Abort sessions, close the server, and close the db if this gateway owns it. */
  close: () => Promise<void>;
}

export async function buildGateway(options: GatewayOptions = {}): Promise<Gateway> {
  const config = options.config ?? loadConfig();
  const ownsDb = !options.db;
  const dbHandle = options.db ?? (await createDb());

  const context: GatewayContext = {
    config,
    tokens: tokenConfigFromEnv(),
    db: dbHandle.db,
    buildLiveAdapter: options.buildLiveAdapter ?? (() => defaultLiveAdapter(config)),
    buildBatchAdapter: options.buildBatchAdapter ?? (() => createSttAdapter(config.batchVendor)),
  };

  const registry = new SessionRegistry();

  const app = Fastify({
    // Never log request bodies (audio/transcript) and redact bearer tokens. WS
    // query strings can carry a token, so skip per-request url logging entirely.
    disableRequestLogging: true,
    logger: {
      level: process.env.LOG_LEVEL ?? 'info',
      redact: ['req.headers.authorization'],
      ...(options.logStream ? { stream: options.logStream } : {}),
    },
  });

  await app.register(websocket);

  app.get('/health', async () => ({
    status: 'ok',
    service: 'hearme-gateway',
    protocolVersion: PROTOCOL_VERSION,
  }));

  registerAuthRoutes(app, context);
  registerBatchRoute(app, context);
  registerLiveRoute(app, context, registry);

  const close = async (): Promise<void> => {
    await registry.closeAll();
    await app.close();
    if (ownsDb) await dbHandle.close();
  };

  return { app, context, registry, close };
}
