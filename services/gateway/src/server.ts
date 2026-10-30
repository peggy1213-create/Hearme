import Fastify, { type FastifyInstance } from 'fastify';
import { PROTOCOL_VERSION } from '@hearme/shared';

/**
 * Build the Hearme gateway.
 *
 * The gateway relays audio to the STT vendor and text to the LLM, and holds only
 * accounts and usage. It stores NO meeting content — audio and transcripts live in
 * memory only while being relayed. Vendor API keys live here, never in the desktop
 * app. See CLAUDE.md. This is a buildable stub; routes are filled in by later
 * prompts.
 */
export function buildServer(): FastifyInstance {
  const app = Fastify({
    // Do not log request bodies: they may carry audio/transcript content.
    logger: { level: process.env.LOG_LEVEL ?? 'info' },
  });

  app.get('/health', async () => ({
    status: 'ok',
    service: 'hearme-gateway',
    protocolVersion: PROTOCOL_VERSION,
  }));

  return app;
}
